//! Beta 1.7.3 block updates: scheduled ticks, random ticks, and neighbor
//! notifications, driven by the shared 20 Hz [`WorldTick`](super::tick::WorldTick).
//!
//! This is the machinery of Beta's `World` (`scheduleBlockUpdate`,
//! `TickUpdates`, `updateBlocksAndPlayCaveSounds`, `notifyBlocksOfNeighborChange`,
//! and the `setBlock*WithNotify` family) and the update hooks of its `Block`
//! class. Each world tick:
//!
//! 1. Pending [`BlockEvent`]s run first. Systems outside the tick pass, such
//!    as block editing and falling blocks, write chunks directly and report
//!    the change here so the right `onBlockRemoval`, `onBlockAdded`, and
//!    neighbor hooks still fire.
//! 2. Up to 1000 due scheduled ticks run in due order, as `TickUpdates` does.
//! 3. Every chunk within [`RANDOM_TICK_RADIUS`] of the player gets 80 random
//!    ticks at positions from Beta's LCG, plus the ice-freezing roll.
//!
//! Block behavior lives in [`behaviors`], one module per family. Each
//! implements [`BlockBehavior`] and is registered by block value in
//! `behaviors::table`. `docs/BLOCK_TICKS.md` explains how to add one.
//!
//! The world a behavior sees is [`TickWorld`]. Its writes are recorded as
//! [`BlockChange`]s so the tick system can remesh, relight, and save the
//! chunks they touch, and its item drops and entity spawns are queued as
//! [`TickEffect`]s for the ECS to apply after the pass.
//!
//! Deliberate differences from Beta:
//!
//! - A scheduled tick whose surroundings are not loaded is retried on the
//!   next tick instead of dropped, and a chunk that unloads saves its pending
//!   ticks, so water at the edge of the loaded world does not freeze in place.
//! - Light comes from [`LightCache`](super::lighting::LightCache), the light
//!   each chunk had at its last mesh job, rather than light arrays updated
//!   with every edit.
//! - There is no weather, so nothing is rained on and snow never accumulates.

use std::collections::HashMap;
use std::collections::VecDeque;

use bevy::math::IVec3;
use bevy::prelude::Resource;

use crate::block::definition;
use crate::block::id::Id;
use crate::item::ItemStack;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::PendingTick;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::meshing::same_appearance;
use crate::world::sky::celestial_angle;
use crate::world::sky::skylight_subtracted;

mod behavior;
pub mod behaviors;
mod scheduler;
mod systems;
mod world;

pub use behavior::BlockBehavior;
pub use behavior::behavior;
pub use behavior::ticks_randomly;
pub use scheduler::ScheduledTick;
pub use systems::BlockTickSet;
pub use systems::BlockTicksPlugin;
pub use world::NEIGHBORS;
pub use world::SCHEDULED_TICK_REACH;
pub use world::TickWorld;

/// Beta ticks the chunks within nine chunks of each player
/// (`updateBlocksAndPlayCaveSounds`' `byte0`). The render distance caps it.
pub const RANDOM_TICK_RADIUS: i32 = 9;
/// Random ticks per chunk per world tick.
pub const RANDOM_TICKS_PER_CHUNK: usize = 80;
/// Scheduled ticks run per world tick, at most (`TickUpdates`).
pub const MAX_SCHEDULED_PER_TICK: usize = 1000;

/// Something that changed the world outside a tick pass and still needs
/// Beta's block hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockEvent {
    /// The block at `position` was replaced; `previous` and `metadata` are
    /// what it held before. Runs `on_removed`, `on_added`, and notifies the
    /// neighbors, as `World.setBlockWithNotify` would have.
    Changed {
        position: IVec3,
        previous: Id,
        metadata: u8,
    },
    /// A player broke `block` with a tool that can harvest it.
    Harvested {
        position: IVec3,
        block: Id,
        metadata: u8,
    },
    /// The player started mining the block at `position`.
    Clicked { position: IVec3 },
    /// The player right-clicked the block at `position`.
    Activated { position: IVec3 },
    /// An entity took a step on the block at `position`.
    Walked { position: IVec3 },
}

/// A lightweight 20 Hz entity snapshot used by plates and detector rails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RedstoneOccupant {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub living: bool,
    pub minecart: bool,
}

/// A block or metadata write made by the tick pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockChange {
    pub position: IVec3,
    pub previous: Id,
    pub previous_metadata: u8,
    pub block: Id,
    pub metadata: u8,
}

impl BlockChange {
    /// Whether the change can alter light: the opacity or emission differs.
    /// Metadata-only changes and same-light swaps only need a remesh.
    pub fn changes_light(&self) -> bool {
        definition::light_opacity(self.previous) != definition::light_opacity(self.block)
            || definition::light_emission(self.previous) != definition::light_emission(self.block)
    }

    /// Whether any mesh can differ after the change. A flowing fluid settling
    /// into its still block, a leaf's decay flag, or a cactus's age change
    /// what the world simulates but nothing it draws.
    pub fn needs_remesh(&self) -> bool {
        self.changes_light()
            || !same_appearance(
                self.previous,
                self.previous_metadata,
                self.block,
                self.metadata,
            )
    }
}

/// Work a tick pass hands back to the ECS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickEffect {
    /// `Block.dropBlockAsItem`: spawn `block`'s natural drops in the cell.
    Drop {
        position: IVec3,
        block: Id,
        metadata: u8,
    },
    /// Spawn a falling block entity for `block`, which is still at
    /// `position`.
    FallingBlock { position: IVec3, block: Id },
    /// A note-block instrument 0..4, pitch 0..24.
    Note {
        position: IVec3,
        instrument: u8,
        pitch: u8,
    },
    /// A TNT entity with Beta's 80-tick fuse, or an explosion-shortened fuse.
    PrimedTnt { position: IVec3, fuse: u8 },
    /// The one item removed by `TileEntityDispenser.getRandomStackFromInventory`.
    Dispense {
        position: IVec3,
        facing: u8,
        stack: ItemStack,
    },
    /// A block-local inventory was broken by an explosion.
    DropStack { position: IVec3, stack: ItemStack },
    /// Move an entity out of the cell newly occupied by a piston extension.
    PistonPush { position: IVec3, direction: IVec3 },
}

/// The block update state of the loaded world.
#[derive(Resource)]
pub struct BlockTicks {
    scheduler: scheduler::TickScheduler,
    /// Beta `World.rand`.
    random: JavaRandom,
    /// Beta `World.field_9437_g`, the LCG that picks random tick cells.
    update_lcg: i32,
    time: u64,
    events: Vec<BlockEvent>,
    changes: Vec<BlockChange>,
    effects: Vec<TickEffect>,
    deferred: VecDeque<(IVec3, Id)>,
    torch_updates: VecDeque<(IVec3, u64)>,
    occupants: Vec<RedstoneOccupant>,
    /// Reused random tick candidates.
    candidates: Vec<IVec3>,
}

impl Default for BlockTicks {
    fn default() -> Self {
        Self::new(0)
    }
}

impl BlockTicks {
    /// Beta seeds both randoms from the clock; a fixed seed keeps runs
    /// reproducible.
    pub fn new(seed: u64) -> Self {
        let mut random = JavaRandom::new(seed);
        let update_lcg = random.next_bits(32) as i32;
        Self {
            scheduler: scheduler::TickScheduler::default(),
            random,
            update_lcg,
            time: 0,
            events: Vec::new(),
            changes: Vec::new(),
            effects: Vec::new(),
            deferred: VecDeque::new(),
            torch_updates: VecDeque::new(),
            occupants: Vec::new(),
            candidates: Vec::with_capacity(RANDOM_TICKS_PER_CHUNK),
        }
    }

    /// Replace the entity snapshot before each block-tick pass.
    pub fn set_occupants(&mut self, occupants: Vec<RedstoneOccupant>) {
        self.occupants = occupants;
    }

    /// The last world tick processed.
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Pending scheduled ticks in due order.
    pub fn scheduled(&self) -> impl Iterator<Item = ScheduledTick> + '_ {
        self.scheduler.iter()
    }

    pub fn scheduled_count(&self) -> usize {
        self.scheduler.len()
    }

    pub fn is_scheduled(&self, position: IVec3, block: Id) -> bool {
        self.scheduler.contains(position, block)
    }

    /// Schedule `block`'s update at `position` for world tick `due`.
    pub fn schedule(&mut self, position: IVec3, block: Id, due: u64) {
        self.scheduler.schedule(position, block, due);
    }

    /// Report a change or interaction for the next tick pass.
    pub fn push_event(&mut self, event: BlockEvent) {
        self.events.push(event);
    }

    /// Report that the block at `position` was replaced outside the tick
    /// pass. `previous` and `metadata` are what the cell held before.
    pub fn block_changed(&mut self, position: IVec3, previous: Id, metadata: u8) {
        self.push_event(BlockEvent::Changed {
            position,
            previous,
            metadata,
        });
    }

    /// Prime TNT destroyed by an explosion with a shortened fuse.
    pub fn prime_tnt(&mut self, position: IVec3, fuse: u8) {
        self.effects.push(TickEffect::PrimedTnt { position, fuse });
    }

    pub fn drop_stack(&mut self, position: IVec3, stack: ItemStack) {
        self.effects.push(TickEffect::DropStack { position, stack });
    }

    pub fn has_pending_events(&self) -> bool {
        !self.events.is_empty()
    }

    /// Run the pending events at world tick `time`.
    pub fn process_events(&mut self, chunks: &mut WorldChunks, light: &mut LightCache, time: u64) {
        if self.events.is_empty() {
            return;
        }
        let events = std::mem::take(&mut self.events);
        let mut world = self.world(chunks, light, time);
        for event in events {
            world.handle_event(event);
        }
        world.flush_deferred();
    }

    /// One world tick at `time`: due scheduled ticks, then random ticks in
    /// `random_chunks`, in order.
    pub fn tick(
        &mut self,
        chunks: &mut WorldChunks,
        light: &mut LightCache,
        time: u64,
        random_chunks: &[ChunkPosition],
    ) {
        self.time = time;
        let mut world = self.world(chunks, light, time);
        world.run_scheduled();
        world.tick_entity_contacts();
        world.flush_deferred();
        for &chunk in random_chunks {
            world.random_tick_chunk(chunk);
        }
        world.flush_deferred();
    }

    /// A world view for running updates by hand, as tests and tools do.
    pub fn world<'a>(
        &'a mut self,
        chunks: &'a mut WorldChunks,
        light: &'a mut LightCache,
        time: u64,
    ) -> TickWorld<'a> {
        let subtracted = skylight_subtracted(celestial_angle(time, 0.0));
        TickWorld::new(chunks, self, light, time, subtracted)
    }

    /// Writes made since the last call, in order.
    pub fn take_changes(&mut self) -> Vec<BlockChange> {
        std::mem::take(&mut self.changes)
    }

    /// Effects queued since the last call, in order.
    pub fn take_effects(&mut self) -> Vec<TickEffect> {
        std::mem::take(&mut self.effects)
    }

    /// Move `position`'s pending ticks into its chunk before it leaves the
    /// world, keeping each tick's remaining delay.
    pub fn unload_chunk(&mut self, position: ChunkPosition, chunk: &mut Chunk) {
        let pending = self
            .scheduler
            .take_chunk(position)
            .into_iter()
            .map(|tick| self.pending_tick(tick))
            .collect();
        chunk.set_pending_ticks(pending);
    }

    /// Every pending tick, grouped by the chunk it belongs to, in the form a
    /// saved chunk stores them. One pass, so a save that covers many chunks does
    /// not rescan the scheduler for each of them.
    pub fn pending_ticks_by_chunk(&self) -> HashMap<ChunkPosition, Vec<PendingTick>> {
        let mut grouped: HashMap<ChunkPosition, Vec<PendingTick>> = HashMap::new();
        for tick in self.scheduler.iter() {
            grouped
                .entry(ChunkPosition::from_block(tick.position.x, tick.position.z))
                .or_default()
                .push(self.pending_tick(tick));
        }
        grouped
    }

    fn pending_tick(&self, tick: ScheduledTick) -> PendingTick {
        let size = CHUNK_SIZE as i32;
        PendingTick {
            index: Chunk::index(
                tick.position.x.rem_euclid(size) as usize,
                tick.position.y as usize,
                tick.position.z.rem_euclid(size) as usize,
            ) as u16,
            block: tick.block,
            delay: tick.due.saturating_sub(self.time).min(u64::from(u32::MAX)) as u32,
        }
    }

    /// Schedule the ticks a chunk carried in from disk.
    pub fn load_chunk(&mut self, position: ChunkPosition, chunk: &mut Chunk) {
        let size = CHUNK_SIZE;
        for tick in chunk.take_pending_ticks() {
            let index = usize::from(tick.index);
            let local = IVec3::new(
                (index % size) as i32,
                (index / (size * size)) as i32,
                (index / size % size) as i32,
            );
            let world = IVec3::new(position.x * size as i32, 0, position.z * size as i32) + local;
            self.scheduler
                .schedule(world, tick.block, self.time + u64::from(tick.delay));
        }
    }

    /// Forget every pending tick and event, as regenerating the world does.
    pub fn clear(&mut self) {
        self.scheduler.clear();
        self.torch_updates.clear();
        self.events.clear();
        self.changes.clear();
        self.effects.clear();
        self.deferred.clear();
    }
}

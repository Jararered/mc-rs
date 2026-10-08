//! The world a block behavior sees: Beta's `World` block, light, and update
//! methods over the loaded chunks.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::lighting::column_channels;
use crate::world::lighting::combined_light;

use super::BlockChange;
use super::BlockEvent;
use super::BlockTicks;
use super::MAX_SCHEDULED_PER_TICK;
use super::RANDOM_TICKS_PER_CHUNK;
use super::TickEffect;
use super::behavior::behavior;
use super::behavior::ticks_randomly;

/// The six face neighbors in `World.notifyBlocksOfNeighborChange` order.
pub const NEIGHBORS: [IVec3; 6] = [
    IVec3::NEG_X,
    IVec3::X,
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
];

/// Beta's checked radius around a scheduled tick (`TickUpdates`' `byte0`).
pub const SCHEDULED_TICK_REACH: i32 = 8;

const POWER_SIDES: [IVec3; 6] = [
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
    IVec3::NEG_X,
    IVec3::X,
];

/// Neighbor notifications nest through behaviors, as in Beta. Past this
/// depth they are queued and delivered once the current update unwinds.
const MAX_NOTIFY_DEPTH: u32 = 64;

/// Mutable access to the loaded world for one block update.
///
/// Reads outside the world's height or in an unloaded chunk see air and
/// metadata zero, like Beta's `World.getBlockId`. Writes there do nothing.
/// Every successful write is recorded so streaming can remesh and relight
/// the change and persistence can save its chunk.
pub struct TickWorld<'a> {
    chunks: &'a mut WorldChunks,
    ticks: &'a mut BlockTicks,
    light: &'a mut LightCache,
    time: u64,
    raining: bool,
    skylight_subtracted: u8,
    /// Beta `World.editingBlocks`: suppresses neighbor notifications.
    editing: bool,
    wire_power_enabled: bool,
    /// How many pistons are in the middle of moving their blocks. Pistons
    /// put off neighbor changes meanwhile, like Beta's
    /// `BlockPistonBase.ignoreUpdates`.
    piston_moves: u32,
    /// Beta `World.scheduledUpdatesAreImmediate`.
    immediate: bool,
    depth: u32,
}

impl<'a> TickWorld<'a> {
    pub(super) fn new(
        chunks: &'a mut WorldChunks,
        ticks: &'a mut BlockTicks,
        light: &'a mut LightCache,
        time: u64,
        skylight_subtracted: u8,
    ) -> Self {
        let raining = ticks.raining;
        Self {
            chunks,
            ticks,
            light,
            time,
            raining,
            skylight_subtracted,
            editing: false,
            wire_power_enabled: true,
            piston_moves: 0,
            immediate: false,
            depth: 0,
        }
    }

    /// `WorldProvider.isHellWorld`.
    pub fn is_hell(&self) -> bool {
        self.ticks.dimension.is_hell()
    }

    /// The world tick being simulated.
    pub fn time(&self) -> u64 {
        self.time
    }

    /// Beta's `World.rand`, the random every `updateTick` receives.
    pub fn random(&mut self) -> &mut JavaRandom {
        &mut self.ticks.random
    }

    pub fn chunks(&self) -> &WorldChunks {
        self.chunks
    }

    pub fn note_mut(&mut self, position: IVec3) -> Option<&mut crate::world::chunk::NoteState> {
        self.chunks.note_at_mut(position.x, position.y, position.z)
    }
    pub fn dispenser(&self, position: IVec3) -> Option<&crate::world::dispenser::Dispenser> {
        self.chunks.dispenser_at(position.x, position.y, position.z)
    }
    pub fn dispense(&mut self, position: IVec3, facing: u8, slot: usize) {
        let Some(dispenser) = self
            .chunks
            .dispenser_at_mut(position.x, position.y, position.z)
        else {
            return;
        };
        let Some(stack) = dispenser.slots[slot] else {
            return;
        };
        dispenser.slots[slot] =
            crate::item::ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
        let single = crate::item::ItemStack::with_data(stack.item(), 1, stack.data())
            .expect("valid dispenser item");
        self.mark_state_dirty(position);
        self.ticks.effects.push(TickEffect::Dispense {
            position,
            facing,
            stack: single,
        });
    }
    /// A tile entity changed without replacing its block or metadata.
    pub fn mark_state_dirty(&mut self, position: IVec3) {
        let block = self.block(position);
        let metadata = self.metadata(position);
        self.ticks.changes.push(BlockChange {
            position,
            previous: block,
            previous_metadata: metadata,
            block,
            metadata,
        });
    }

    // --- Reading blocks -------------------------------------------------

    /// `World.getBlockId`. Air outside the world or a loaded chunk.
    pub fn block(&self, position: IVec3) -> Block {
        self.chunks
            .block_at(position.x, position.y, position.z)
            .unwrap_or(Block::Air)
    }

    /// `World.getBlockMetadata`.
    pub fn metadata(&self, position: IVec3) -> u8 {
        self.chunks.metadata_at(position.x, position.y, position.z)
    }

    /// `World.isAirBlock`.
    pub fn is_air(&self, position: IVec3) -> bool {
        self.block(position) == Block::Air
    }

    /// Whether the cell is inside the world height and its chunk is loaded.
    pub fn is_loaded(&self, position: IVec3) -> bool {
        (0..CHUNK_HEIGHT as i32).contains(&position.y)
            && self
                .chunks
                .contains(ChunkPosition::from_block(position.x, position.z))
    }

    /// `World.checkChunksExist` over the cube `radius` blocks around
    /// `position`. Only the columns matter: chunks span the world's height.
    pub fn area_loaded(&self, position: IVec3, radius: i32) -> bool {
        let min = ChunkPosition::from_block(position.x - radius, position.z - radius);
        let max = ChunkPosition::from_block(position.x + radius, position.z + radius);
        (min.x..=max.x)
            .all(|x| (min.z..=max.z).all(|z| self.chunks.contains(ChunkPosition { x, z })))
    }

    /// `World.isBlockOpaqueCube`.
    pub fn is_opaque_cube(&self, position: IVec3) -> bool {
        (self.block(position)).is_opaque_cube()
    }

    /// `World.isBlockNormalCube`: a full, opaque cube that torches, ladders,
    /// and snow can rest against.
    pub fn is_normal_cube(&self, position: IVec3) -> bool {
        (self.block(position)).is_normal_cube()
    }

    /// `World.getBlockMaterial(...).isSolid()`.
    pub fn is_solid(&self, position: IVec3) -> bool {
        (self.block(position)).is_solid_material()
    }

    /// World.isBlockProvidingPowerTo: strong output from the queried block.
    pub fn block_providing_power_to(&mut self, position: IVec3, side: u8) -> bool {
        let block = self.block(position);
        (block != Block::RedstoneWire || self.wire_power_enabled)
            && behavior(block).strong_power(self, position, side)
    }

    /// World.isBlockGettingPowered: strong power from any adjacent face.
    pub fn block_getting_powered(&mut self, position: IVec3) -> bool {
        POWER_SIDES
            .iter()
            .enumerate()
            .any(|(side, offset)| self.block_providing_power_to(position + *offset, side as u8))
    }

    /// World.isBlockIndirectlyProvidingPowerTo: normal blocks relay strong
    /// power; other blocks provide their own weak power on this face.
    pub fn block_indirectly_providing_power_to(&mut self, position: IVec3, side: u8) -> bool {
        if self.is_normal_cube(position) {
            self.block_getting_powered(position)
        } else {
            let block = self.block(position);
            (block != Block::RedstoneWire || self.wire_power_enabled)
                && behavior(block).weak_power(self, position, side)
        }
    }

    /// World.isBlockIndirectlyGettingPowered.
    pub fn block_indirectly_getting_powered(&mut self, position: IVec3) -> bool {
        POWER_SIDES.iter().enumerate().any(|(side, offset)| {
            self.block_indirectly_providing_power_to(position + *offset, side as u8)
        })
    }

    /// Suppress wire output while calculating its strength, not all sources.
    pub fn without_wire_power(&mut self, f: impl FnOnce(&mut Self) -> bool) -> bool {
        let was_enabled = self.wire_power_enabled;
        self.wire_power_enabled = false;
        let result = f(self);
        self.wire_power_enabled = was_enabled;
        result
    }

    /// Record a torch switching off; eight toggles in 100 ticks burn out.
    pub fn torch_burned_out(&mut self, position: IVec3, record: bool) -> bool {
        while self
            .ticks
            .torch_updates
            .front()
            .is_some_and(|(_, at)| self.time.saturating_sub(*at) > 100)
        {
            self.ticks.torch_updates.pop_front();
        }
        if record {
            self.ticks.torch_updates.push_back((position, self.time));
        }
        self.ticks
            .torch_updates
            .iter()
            .filter(|(cell, _)| *cell == position)
            .count()
            >= 8
    }

    /// A piston starts moving its blocks; pair with [`Self::end_piston_move`].
    pub fn begin_piston_move(&mut self) {
        self.piston_moves += 1;
    }
    pub fn end_piston_move(&mut self) {
        self.piston_moves = self.piston_moves.saturating_sub(1);
    }
    pub fn piston_moving(&self) -> bool {
        self.piston_moves > 0
    }

    /// Beta's inset plate/detector bounding box against current entities.
    pub fn occupant_on(&self, position: IVec3, block: Block) -> bool {
        let inset = 0.125;
        self.ticks.occupants.iter().any(|body| {
            let eligible = match block {
                Block::StonePressurePlate => body.living,
                Block::DetectorRail => body.minecart,
                _ => true,
            };
            eligible
                && body.max[0] > position.x as f32 + inset
                && body.min[0] < position.x as f32 + 1.0 - inset
                && body.max[1] > position.y as f32
                && body.min[1] < position.y as f32 + 0.25
                && body.max[2] > position.z as f32 + inset
                && body.min[2] < position.z as f32 + 1.0 - inset
        })
    }

    /// A contact is checked every tick, independently of footstep distance.
    /// As in `Entity.moveEntity`, every cell the body's box overlaps is
    /// touched, so standing on the edge of a plate presses it.
    pub fn tick_entity_contacts(&mut self) {
        let mut cells = Vec::new();
        for body in &self.ticks.occupants {
            let y = body.min[1].floor() as i32;
            let min_x = (body.min[0] + 0.001).floor() as i32;
            let max_x = (body.max[0] - 0.001).floor() as i32;
            let min_z = (body.min[2] + 0.001).floor() as i32;
            let max_z = (body.max[2] - 0.001).floor() as i32;
            for x in min_x..=max_x {
                for z in min_z..=max_z {
                    cells.push(IVec3::new(x, y, z));
                    cells.push(IVec3::new(x, y - 1, z));
                }
            }
        }
        for position in cells {
            let block = self.block(position);
            if matches!(
                block,
                Block::StonePressurePlate | Block::WoodenPressurePlate | Block::DetectorRail
            ) {
                behavior(block).entity_collided(self, position);
            }
        }
    }

    // --- Light and sky --------------------------------------------------

    /// Cached light. Under streaming only chunks outside the rendered area are
    /// ever missing, reached when a tick at the render edge reads across the
    /// border, and those are estimated from their column: relighting a chunk
    /// here would stall the frame. Without streaming the chunk is lit now.
    fn light_channels(&mut self, position: IVec3) -> (u8, u8) {
        if let Some(channels) = self.light.channels(position.x, position.y, position.z) {
            return channels;
        }
        if self.light.is_streamed() {
            return column_channels(self.chunks, position.x, position.y, position.z);
        }
        let chunk = ChunkPosition::from_block(position.x, position.z);
        if self.chunks.contains(chunk) {
            self.light.relight(self.chunks, chunk);
        }
        self.light
            .channels(position.x, position.y, position.z)
            .unwrap_or(if self.ticks.dimension.has_sky() {
                (15, 0)
            } else {
                (0, 0)
            })
    }

    /// `World.getBlockLightValue`: the brighter of sky light, dimmed by the
    /// time of day, and block light.
    pub fn light(&mut self, position: IVec3) -> u8 {
        let (sky, block) = self.light_channels(position);
        combined_light(sky, block, self.skylight_subtracted)
    }

    /// `World.getFullBlockLightValue`: [`Self::light`] at full daylight.
    pub fn full_light(&mut self, position: IVec3) -> u8 {
        let (sky, block) = self.light_channels(position);
        combined_light(sky, block, 0)
    }

    /// `World.getSavedLightValue(EnumSkyBlock.Block, ...)`.
    pub fn block_light(&mut self, position: IVec3) -> u8 {
        self.light_channels(position).1
    }

    /// `World.getSavedLightValue(EnumSkyBlock.Sky, ...)`.
    pub fn sky_light(&mut self, position: IVec3) -> u8 {
        self.light_channels(position).0
    }

    /// `World.canBlockSeeTheSky`: nothing above the cell stops any light.
    /// Beta keeps this as a heightmap of light-blocking blocks; the chunk
    /// heightmap here tracks the ground instead, so the column is scanned.
    pub fn sees_sky(&self, position: IVec3) -> bool {
        (position.y.max(0)..CHUNK_HEIGHT as i32).all(|y| {
            self.block(IVec3::new(position.x, y, position.z))
                .light_opacity()
                == 0
        })
    }

    /// `World.canBlockBeRainedOn`: precipitation reaches this cell only if
    /// the sky is unobstructed and the local biome receives rain, not snow.
    pub fn rained_on(&self, position: IVec3) -> bool {
        self.raining
            && self.sees_sky(position)
            && self.top_solid_block(position.x, position.z) <= position.y
            && self
                .chunks
                .climate_at(position.x, position.z)
                .is_some_and(|climate| {
                    !matches!(
                        climate.biome,
                        Biome::Taiga
                            | Biome::Tundra
                            | Biome::IceDesert
                            | Biome::Desert
                            | Biome::Hell
                    )
                })
    }

    pub fn is_raining(&self) -> bool {
        self.raining
    }

    /// `BiomeGenBase.getEnableSnow` for the column's biome.
    pub fn snows_at(&self, x: i32, z: i32) -> bool {
        self.chunks.climate_at(x, z).is_some_and(|climate| {
            matches!(
                climate.biome,
                Biome::Taiga | Biome::Tundra | Biome::IceDesert
            )
        })
    }

    /// `World.findTopSolidBlock`: one above the highest solid or liquid
    /// block in the column, or `-1` for an empty column.
    pub fn top_solid_block(&self, x: i32, z: i32) -> i32 {
        self.chunks.top_solid_block(x, z)
    }

    // --- Writing blocks -------------------------------------------------

    /// `World.setBlock`: replace the block with metadata zero, running the
    /// old block's [`on_removed`](super::BlockBehavior::on_removed) and the new
    /// block's [`on_added`](super::BlockBehavior::on_added). Neighbors are
    /// not notified. Returns `false` if the cell already held `block` or is
    /// not loaded.
    pub fn set_block(&mut self, position: IVec3, block: Block) -> bool {
        self.write(position, block, None)
    }

    /// `World.setBlockAndMetadata`. Unlike [`Self::set_block`], a change of
    /// metadata alone counts as a change and reruns both hooks.
    pub fn set_block_and_metadata(&mut self, position: IVec3, block: Block, metadata: u8) -> bool {
        self.write(position, block, Some(metadata))
    }

    /// `World.setBlockMetadata`, without hooks or notifications.
    pub fn set_metadata(&mut self, position: IVec3, metadata: u8) -> bool {
        let previous = self.metadata(position);
        if !self
            .chunks
            .set_metadata(position.x, position.y, position.z, metadata)
        {
            return false;
        }
        if previous != metadata & 0x0f {
            let block = self.block(position);
            self.ticks.changes.push(BlockChange {
                position,
                previous: block,
                previous_metadata: previous,
                block,
                metadata: metadata & 0x0f,
            });
        }
        true
    }

    /// `World.setBlockWithNotify`: [`Self::set_block`], then notify the six
    /// neighbors of the change.
    pub fn set_block_notify(&mut self, position: IVec3, block: Block) -> bool {
        if !self.set_block(position, block) {
            return false;
        }
        self.notify_neighbors(position, block);
        true
    }

    /// `World.setBlockAndMetadataWithNotify`.
    pub fn set_block_and_metadata_notify(
        &mut self,
        position: IVec3,
        block: Block,
        metadata: u8,
    ) -> bool {
        if !self.set_block_and_metadata(position, block, metadata) {
            return false;
        }
        self.notify_neighbors(position, block);
        true
    }

    /// `World.setBlockMetadataWithNotify`. Beta notifies the neighbors even
    /// when the value is unchanged.
    pub fn set_metadata_notify(&mut self, position: IVec3, metadata: u8) {
        if self.set_metadata(position, metadata) {
            let block = self.block(position);
            self.notify_neighbors(position, block);
        }
    }

    fn write(&mut self, position: IVec3, block: Block, metadata: Option<u8>) -> bool {
        if !self.is_loaded(position) {
            return false;
        }
        let previous = self.block(position);
        let previous_metadata = self.metadata(position);
        let changed = match metadata {
            None => previous != block,
            Some(metadata) => previous != block || previous_metadata != metadata & 0x0f,
        };
        if !changed {
            return false;
        }
        self.chunks.set_block_with_metadata(
            position.x,
            position.y,
            position.z,
            block,
            metadata.unwrap_or(0),
        );
        self.ticks.changes.push(BlockChange {
            position,
            previous,
            previous_metadata,
            block,
            metadata: metadata.unwrap_or(0) & 0x0f,
        });
        if previous != Block::Air {
            behavior(previous).on_removed(self, position, previous, previous_metadata);
        }
        if block != Block::Air {
            behavior(block).on_added(self, position);
        }
        true
    }

    // --- Updates --------------------------------------------------------

    /// `World.notifyBlocksOfNeighborChange`: tell each face neighbor that
    /// `block` changed at `position`.
    pub fn notify_neighbors(&mut self, position: IVec3, block: Block) {
        for offset in NEIGHBORS {
            self.notify_neighbor(position + offset, block);
        }
    }

    /// `World.notifyBlockOfNeighborChange`.
    pub fn notify_neighbor(&mut self, position: IVec3, neighbor: Block) {
        if self.editing || !self.is_loaded(position) {
            return;
        }
        let block = self.block(position);
        if block == Block::Air {
            return;
        }
        if self.depth >= MAX_NOTIFY_DEPTH {
            self.ticks.deferred.push_back((position, neighbor));
            return;
        }
        self.depth += 1;
        behavior(block).neighbor_changed(self, position, neighbor);
        self.depth -= 1;
    }

    /// `World.scheduleBlockUpdate`: run `block`'s update at `position` after
    /// `delay` ticks, if the cell still holds it then. A second request for
    /// the same cell and block while one is pending is ignored.
    pub fn schedule(&mut self, position: IVec3, block: Block, delay: u32) {
        if self.immediate {
            if self.area_loaded(position, SCHEDULED_TICK_REACH)
                && block != Block::Air
                && self.block(position) == block
            {
                behavior(block).update_tick(self, position);
            }
            return;
        }
        if (0..CHUNK_HEIGHT as i32).contains(&position.y) {
            self.ticks
                .scheduler
                .schedule(position, block, self.time + u64::from(delay));
        }
    }

    /// Whether a tick for `block` at `position` is pending.
    pub fn is_scheduled(&self, position: IVec3, block: Block) -> bool {
        self.ticks.scheduler.contains(position, block)
    }

    /// Beta `World.editingBlocks`, which silences neighbor notifications
    /// while a block rewrites itself.
    pub fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
    }

    /// Beta `World.scheduledUpdatesAreImmediate`: while set, scheduling runs
    /// the update at once instead of queueing it.
    pub fn set_immediate(&mut self, immediate: bool) {
        self.immediate = immediate;
    }

    /// Run a block's update now, as a random tick or a due scheduled tick
    /// would.
    pub fn update_tick(&mut self, position: IVec3) {
        let block = self.block(position);
        if block != Block::Air {
            behavior(block).update_tick(self, position);
        }
    }

    /// Beta's `TickUpdates(false)`: run up to [`MAX_SCHEDULED_PER_TICK`] due
    /// entries in order. An entry whose surroundings are not all loaded waits
    /// for the next tick.
    pub(super) fn run_scheduled(&mut self) {
        let count = self.ticks.scheduler.len().min(MAX_SCHEDULED_PER_TICK);
        for _ in 0..count {
            let Some(tick) = self.ticks.scheduler.pop_due(self.time) else {
                break;
            };
            if !self.area_loaded(tick.position, SCHEDULED_TICK_REACH) {
                self.ticks
                    .scheduler
                    .schedule(tick.position, tick.block, self.time + 1);
                continue;
            }
            if tick.block != Block::Air && self.block(tick.position) == tick.block {
                behavior(tick.block).update_tick(self, tick.position);
            }
        }
    }

    /// Beta's `World.field_9437_g` step, then `>> 2` as it is always read.
    fn next_update_position(&mut self) -> i32 {
        self.ticks.update_lcg = self
            .ticks
            .update_lcg
            .wrapping_mul(3)
            .wrapping_add(1_013_904_223);
        self.ticks.update_lcg >> 2
    }

    /// One chunk's share of `updateBlocksAndPlayCaveSounds`: the ice-freezing
    /// roll, then [`RANDOM_TICKS_PER_CHUNK`] random ticks.
    pub(super) fn random_tick_chunk(&mut self, chunk: ChunkPosition) {
        let origin_x = chunk.x * CHUNK_SIZE as i32;
        let origin_z = chunk.z * CHUNK_SIZE as i32;
        if self.ticks.random.next_int(16) == 0 {
            let cell = self.next_update_position();
            let x = origin_x + (cell & 15);
            let z = origin_z + (cell >> 8 & 15);
            self.freeze_column(x, z);
        }

        let Some(generated) = self.chunks.get(chunk) else {
            return;
        };
        let raw = generated.chunk.raw_blocks();
        let mut candidates = std::mem::take(&mut self.ticks.candidates);
        candidates.clear();
        for _ in 0..RANDOM_TICKS_PER_CHUNK {
            self.ticks.update_lcg = self
                .ticks
                .update_lcg
                .wrapping_mul(3)
                .wrapping_add(1_013_904_223);
            let cell = self.ticks.update_lcg >> 2;
            let x = (cell & 15) as usize;
            let z = (cell >> 8 & 15) as usize;
            let y = (cell >> 16 & 127) as usize;
            if ticks_randomly(raw[Chunk::index(x, y, z)]) {
                candidates.push(IVec3::new(
                    origin_x + x as i32,
                    y as i32,
                    origin_z + z as i32,
                ));
            }
        }
        // Beta reads each sampled cell as it goes; an earlier tick may have
        // replaced a later candidate, so check it again.
        for &position in &candidates {
            let block = self.block(position);
            if behavior(block).ticks_randomly(block) {
                behavior(block).update_tick(self, position);
            }
        }
        self.ticks.candidates = candidates;
    }

    /// The snowy-biome half of the per-chunk weather roll: still water under
    /// the column's top freezes where block light is below 10. During
    /// precipitation a clear, supported cell also collects a snow layer.
    fn freeze_column(&mut self, x: i32, z: i32) {
        let y = self.top_solid_block(x, z);
        let top = IVec3::new(x, y, z);
        if !self.snows_at(x, z)
            || !(0..CHUNK_HEIGHT as i32).contains(&y)
            || self.block_light(top) >= 10
        {
            return;
        }
        let below = top - IVec3::Y;
        if self.block(below) == Block::Water && self.metadata(below) == 0 {
            self.set_block_notify(below, Block::Ice);
        }
        if self.raining
            && self.block(top) == Block::Air
            && super::behaviors::snow::SnowLayer::can_stay(self, top)
            && self.block(below) != Block::Ice
        {
            self.set_block_notify(top, Block::SnowLayer);
        }
    }

    pub(super) fn handle_event(&mut self, event: BlockEvent) {
        match event {
            BlockEvent::Changed {
                position,
                previous,
                metadata,
            } => {
                let block = self.block(position);
                if block == previous && self.metadata(position) == metadata {
                    return;
                }
                if previous != Block::Air {
                    behavior(previous).on_removed(self, position, previous, metadata);
                }
                if block != Block::Air && self.block(position) == block {
                    behavior(block).on_added(self, position);
                }
                let block = self.block(position);
                self.notify_neighbors(position, block);
            }
            BlockEvent::Harvested {
                position,
                block,
                metadata,
            } => behavior(block).harvested(self, position, block, metadata),
            BlockEvent::Clicked { position } => {
                let block = self.block(position);
                behavior(block).clicked(self, position);
            }
            BlockEvent::Activated { position } => {
                let block = self.block(position);
                behavior(block).activated(self, position);
            }
            BlockEvent::Walked { position } => {
                let block = self.block(position);
                behavior(block).entity_walked(self, position);
            }
        }
    }

    /// Deliver notifications that were queued past the nesting limit.
    pub(crate) fn flush_deferred(&mut self) {
        // A pathological chain could keep refilling the queue; leave the rest
        // for the next tick rather than stall a frame.
        for _ in 0..4096 {
            let Some((position, neighbor)) = self.ticks.deferred.pop_front() else {
                return;
            };
            self.notify_neighbor(position, neighbor);
        }
    }

    // --- Effects --------------------------------------------------------

    /// `Block.dropBlockAsItem`: pop the block's natural drops at `position`.
    /// The items spawn after the tick pass, from `entity::drops`.
    pub fn drop_block_as_item(&mut self, position: IVec3, block: Block, metadata: u8) {
        self.ticks.effects.push(TickEffect::Drop {
            position,
            block,
            metadata,
        });
    }

    /// Spawn an `EntityFallingSand` for `block` at `position`. The block stays
    /// in the world until the entity's first tick removes it, as in Beta.
    pub fn spawn_falling_block(&mut self, position: IVec3, block: Block) {
        self.ticks
            .effects
            .push(TickEffect::FallingBlock { position, block });
    }

    /// Beta `BlockTNT.onBlockDestroyedByPlayer`: the block becomes a primed entity.
    pub fn prime_tnt(&mut self, position: IVec3, fuse: u16) {
        self.ticks
            .effects
            .push(TickEffect::PrimedTnt { position, fuse });
    }

    pub fn piston_push(&mut self, position: IVec3, direction: IVec3) {
        self.ticks.effects.push(TickEffect::PistonPush {
            position,
            direction,
        });
    }

    /// Queue an ECS effect after the block tick pass.
    pub fn emit(&mut self, effect: TickEffect) {
        self.ticks.effects.push(effect);
    }
}

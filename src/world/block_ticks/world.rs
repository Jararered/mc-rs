//! The world a block behavior sees: Beta's `World` block, light, and update
//! methods over the loaded chunks.

use bevy::math::IVec3;

use crate::block::definition::light_opacity;
use crate::block::fluids::is_liquid;
use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::is_solid_material;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
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
    skylight_subtracted: u8,
    /// Beta `World.editingBlocks`: suppresses neighbor notifications.
    editing: bool,
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
        Self {
            chunks,
            ticks,
            light,
            time,
            skylight_subtracted,
            editing: false,
            immediate: false,
            depth: 0,
        }
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

    // --- Reading blocks -------------------------------------------------

    /// `World.getBlockId`. Air outside the world or a loaded chunk.
    pub fn block(&self, position: IVec3) -> Id {
        self.chunks
            .block_at(position.x, position.y, position.z)
            .unwrap_or(Id::Air)
    }

    /// `World.getBlockMetadata`.
    pub fn metadata(&self, position: IVec3) -> u8 {
        self.chunks.metadata_at(position.x, position.y, position.z)
    }

    /// `World.isAirBlock`.
    pub fn is_air(&self, position: IVec3) -> bool {
        self.block(position) == Id::Air
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
        is_opaque_cube(self.block(position))
    }

    /// `World.isBlockNormalCube`: a full, opaque cube that torches, ladders,
    /// and snow can rest against. The block definitions' opaque-cube flag
    /// already excludes translucent materials and shaped blocks.
    pub fn is_normal_cube(&self, position: IVec3) -> bool {
        is_opaque_cube(self.block(position))
    }

    /// `World.getBlockMaterial(...).isSolid()`.
    pub fn is_solid(&self, position: IVec3) -> bool {
        is_solid_material(self.block(position))
    }

    // --- Light and sky --------------------------------------------------

    /// Cached light, lighting the chunk now if streaming has not. Only chunks
    /// outside the rendered area are ever missing, and only rarely reached.
    fn light_channels(&mut self, position: IVec3) -> (u8, u8) {
        if let Some(channels) = self.light.channels(position.x, position.y, position.z) {
            return channels;
        }
        let chunk = ChunkPosition::from_block(position.x, position.z);
        if self.chunks.contains(chunk) {
            self.light.relight(self.chunks, chunk);
        }
        self.light
            .channels(position.x, position.y, position.z)
            .unwrap_or((15, 0))
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
        (position.y.max(0)..CHUNK_HEIGHT as i32)
            .all(|y| light_opacity(self.block(IVec3::new(position.x, y, position.z))) == 0)
    }

    /// `World.canBlockBeRainedOn`. There is no weather yet, so nothing is
    /// rained on; behaviors that react to rain call this so weather can land
    /// in one place.
    pub fn rained_on(&self, _position: IVec3) -> bool {
        false
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
        let mut y = CHUNK_HEIGHT as i32 - 1;
        while y > 0 {
            let block = self.block(IVec3::new(x, y, z));
            if is_solid_material(block) || is_liquid(block) {
                return y + 1;
            }
            y -= 1;
        }
        -1
    }

    // --- Writing blocks -------------------------------------------------

    /// `World.setBlock`: replace the block with metadata zero, running the
    /// old block's [`on_removed`](super::BlockBehavior::on_removed) and the new
    /// block's [`on_added`](super::BlockBehavior::on_added). Neighbors are
    /// not notified. Returns `false` if the cell already held `block` or is
    /// not loaded.
    pub fn set_block(&mut self, position: IVec3, block: Id) -> bool {
        self.write(position, block, None)
    }

    /// `World.setBlockAndMetadata`. Unlike [`Self::set_block`], a change of
    /// metadata alone counts as a change and reruns both hooks.
    pub fn set_block_and_metadata(&mut self, position: IVec3, block: Id, metadata: u8) -> bool {
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
    pub fn set_block_notify(&mut self, position: IVec3, block: Id) -> bool {
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
        block: Id,
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

    fn write(&mut self, position: IVec3, block: Id, metadata: Option<u8>) -> bool {
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
        if previous != Id::Air {
            behavior(previous).on_removed(self, position, previous, previous_metadata);
        }
        if block != Id::Air {
            behavior(block).on_added(self, position);
        }
        true
    }

    // --- Updates --------------------------------------------------------

    /// `World.notifyBlocksOfNeighborChange`: tell each face neighbor that
    /// `block` changed at `position`.
    pub fn notify_neighbors(&mut self, position: IVec3, block: Id) {
        for offset in NEIGHBORS {
            self.notify_neighbor(position + offset, block);
        }
    }

    /// `World.notifyBlockOfNeighborChange`.
    pub fn notify_neighbor(&mut self, position: IVec3, neighbor: Id) {
        if self.editing || !self.is_loaded(position) {
            return;
        }
        let block = self.block(position);
        if block == Id::Air {
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
    pub fn schedule(&mut self, position: IVec3, block: Id, delay: u32) {
        if self.immediate {
            if self.area_loaded(position, SCHEDULED_TICK_REACH)
                && block != Id::Air
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
    pub fn is_scheduled(&self, position: IVec3, block: Id) -> bool {
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
        if block != Id::Air {
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
            if tick.block != Id::Air && self.block(tick.position) == tick.block {
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
    /// the column's top freezes where block light is below 10. Snow cover
    /// needs rain, which does not exist yet.
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
        if self.block(below) == Id::Water && self.metadata(below) == 0 {
            self.set_block_notify(below, Id::Ice);
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
                if previous != Id::Air {
                    behavior(previous).on_removed(self, position, previous, metadata);
                }
                if block != Id::Air && self.block(position) == block {
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
    pub fn drop_block_as_item(&mut self, position: IVec3, block: Id, metadata: u8) {
        self.ticks.effects.push(TickEffect::Drop {
            position,
            block,
            metadata,
        });
    }

    /// Spawn an `EntityFallingSand` for `block` at `position`. The block stays
    /// in the world until the entity's first tick removes it, as in Beta.
    pub fn spawn_falling_block(&mut self, position: IVec3, block: Id) {
        self.ticks
            .effects
            .push(TickEffect::FallingBlock { position, block });
    }

    /// Queue a presentation-only effect.
    pub fn emit(&mut self, effect: TickEffect) {
        self.ticks.effects.push(effect);
    }
}

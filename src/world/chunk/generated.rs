//! Chunk data shared by generators, storage, streaming, and simulation.

use crate::item::ItemStack;
use crate::world::biome::BiomeMap;

use super::Chunk;
use super::Heightmap;

pub struct GeneratedChunk {
    pub chunk: Chunk,
    pub heightmap: Heightmap,
    pub biomes: BiomeMap,
    /// Dropped items stored with this chunk. Live entities are authoritative
    /// while the chunk is loaded; this list is the on-disk copy.
    pub items: Vec<ChunkDroppedItem>,
    /// Whether this chunk's population pass has run. The pass writes into this
    /// chunk and its `+x`, `+z`, and `+x+z` neighbors, so a chunk is finished
    /// only once it and its `-x`, `-z`, and `-x-z` neighbors are populated.
    pub populated: bool,
}

/// A dropped item saved inside the chunk that contains its position.
#[derive(Clone, Debug)]
pub struct ChunkDroppedItem {
    pub stack: ItemStack,
    pub position: [f32; 3],
    pub motion: [f32; 3],
    pub age_ticks: u32,
    pub pickup_delay_ticks: u16,
    pub hover_start: f32,
    pub rng_state: u64,
    /// `EntityItem.health`: what fire, lava, and cactus wear down.
    pub health: u8,
    /// `Entity.fire`: ticks left burning, or negative while not alight.
    pub fire: i16,
}

impl ChunkDroppedItem {
    /// The health a new `EntityItem` starts with.
    pub const FULL_HEALTH: u8 = 5;
}

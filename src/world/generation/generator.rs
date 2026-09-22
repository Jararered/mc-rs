use crate::item::ItemStack;
use crate::random::JavaRandom;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Mutex;

use super::biome::BiomeGenerator;
use super::biome::BiomeMap;
use super::caves;
use super::heightmap::Heightmap;
use super::population;
use super::surface::apply_surface;
use super::terrain::TerrainGenerator;
use super::trees::decorate;

pub struct GeneratedChunk {
    pub chunk: Chunk,
    pub heightmap: Heightmap,
    pub biomes: BiomeMap,
    /// Dropped items stored with this chunk. Live entities are authoritative
    /// while the chunk is loaded; this list is the on-disk copy.
    pub items: Vec<ChunkDroppedItem>,
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
}

/// Beta-style climate, density, and surface generation for a full chunk.
///
/// Derived from the local C++ reference under
/// `docs/minecraft-beta-alpha-terrain-generation-cpp/`. That reference only
/// computes a narrow chunk slice for seed filtering; the density interpolation
/// here covers all 4×4×8 cells of the visible chunk.
pub struct WorldGenerator {
    seed: u64,
    biomes: BiomeGenerator,
    terrain: TerrainGenerator,
    cache: Mutex<VecDeque<(ChunkPos, Chunk, BiomeMap, HashMap<ChunkPos, JavaRandom>)>>,
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            biomes: BiomeGenerator::new(seed),
            terrain: TerrainGenerator::new(seed),
            cache: Mutex::new(VecDeque::new()),
        }
    }

    pub fn generate(&self, position: ChunkPos) -> GeneratedChunk {
        let (mut chunk, biomes, population_rng) = self.generate_undecorated(position);
        // Capture the ground after caves and population, before vegetation.
        let heightmap = Heightmap::from_chunk(&chunk);
        decorate(
            &mut chunk,
            position,
            &self.terrain,
            &population_rng,
            |wx, wz| self.biomes.climate_at(wx, wz),
            |remote| self.generate_undecorated(remote).0,
        );
        GeneratedChunk {
            chunk,
            heightmap,
            biomes,
            items: Vec::new(),
        }
    }

    fn generate_undecorated(
        &self,
        position: ChunkPos,
    ) -> (Chunk, BiomeMap, HashMap<ChunkPos, JavaRandom>) {
        if let Some((_, chunk, biomes, random)) = self
            .cache
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.0 == position)
        {
            return (chunk.clone(), biomes.clone(), random.clone());
        }
        let biomes = self.biomes.generate(position);
        let mut chunk = self.terrain.generate_base(position, &biomes);
        apply_surface(&mut chunk, position, &biomes, &self.terrain);
        caves::carve(&mut chunk, position, self.seed);
        let random =
            population::populate(&mut chunk, position, self.seed, &self.terrain, &self.biomes);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() == 64 {
            cache.pop_front();
        }
        cache.push_back((position, chunk.clone(), biomes.clone(), random.clone()));
        (chunk, biomes, random)
    }

    /// Topmost solid block in the base density terrain at a world column.
    ///
    /// Decoration uses this to place trees whose trunks start outside the chunk
    /// being generated, so canopies stay seamless across chunk borders.
    pub fn column_top(&self, world_x: i32, world_z: i32) -> usize {
        self.terrain
            .column_top(world_x, world_z, |wx, wz| self.biomes.climate_at(wx, wz))
    }
}

/// Retain the original one-chunk convenience API with a deterministic seed.
pub fn generate_chunk(position: ChunkPos) -> Chunk {
    WorldGenerator::new(0).generate(position).chunk
}

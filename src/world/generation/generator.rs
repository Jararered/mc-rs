use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::BiomeGenerator;
use super::biome::BiomeMap;
use super::heightmap::Heightmap;
use super::surface::apply_surface;
use super::terrain::TerrainGenerator;

pub struct GeneratedChunk {
    pub chunk: Chunk,
    pub heightmap: Heightmap,
    pub biomes: BiomeMap,
}

/// Beta-style climate, density, and surface generation for a full chunk.
///
/// Derived from the local C++ reference under
/// `docs/minecraft-beta-alpha-terrain-generation-cpp/`. That reference only
/// computes a narrow chunk slice for seed filtering; the density interpolation
/// here covers all 4×4×8 cells of the visible chunk.
pub struct WorldGenerator {
    biomes: BiomeGenerator,
    terrain: TerrainGenerator,
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            biomes: BiomeGenerator::new(seed),
            terrain: TerrainGenerator::new(seed),
        }
    }

    pub fn generate(&self, position: ChunkPos) -> GeneratedChunk {
        let biomes = self.biomes.generate(position);
        let mut chunk = self.terrain.generate_base(position, &biomes);
        apply_surface(&mut chunk, position, &biomes, &self.terrain);
        let heightmap = Heightmap::from_chunk(&chunk);
        GeneratedChunk {
            chunk,
            heightmap,
            biomes,
        }
    }
}

/// Retain the original one-chunk convenience API with a deterministic seed.
pub fn generate_chunk(position: ChunkPos) -> Chunk {
    WorldGenerator::new(0).generate(position).chunk
}

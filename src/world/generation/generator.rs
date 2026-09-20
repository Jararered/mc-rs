use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::BiomeGenerator;
use super::biome::BiomeMap;
use super::heightmap::Heightmap;
use super::surface::apply_surface;
use super::terrain::TerrainGenerator;
use super::trees::decorate;

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
    seed: u64,
    biomes: BiomeGenerator,
    terrain: TerrainGenerator,
}

impl WorldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            biomes: BiomeGenerator::new(seed),
            terrain: TerrainGenerator::new(seed),
        }
    }

    pub fn generate(&self, position: ChunkPos) -> GeneratedChunk {
        let biomes = self.biomes.generate(position);
        let mut chunk = self.terrain.generate_base(position, &biomes);
        apply_surface(&mut chunk, position, &biomes, &self.terrain);
        // The heightmap stays terrain-only: it describes the ground surface and
        // is used for spawn placement and terrain-continuity checks.
        let heightmap = Heightmap::from_chunk(&chunk);
        decorate(&mut chunk, position, &self.terrain, self.seed, |wx, wz| {
            self.biomes.climate_at(wx, wz)
        });
        GeneratedChunk {
            chunk,
            heightmap,
            biomes,
        }
    }

    /// Topmost solid block in a world column, matching the terrain that
    /// [`Self::generate`] produces for the chunk containing that column.
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

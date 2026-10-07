use crate::world::biome::BiomeMap;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;

use super::super::ChunkGenerator;
use super::super::overworld::population::source_random;
use super::caves;
use super::population;
use super::terrain::NetherTerrain;
use super::world::PopulationWorld;

/// Beta 1.7.3 `ChunkProviderHell`, built from the same seed as the Overworld.
///
/// [`ChunkGenerator::generate_base`] is `provideChunk`: terrain, the soul
/// sand, gravel and bedrock surface pass, and caves. Those match Beta block
/// for block. [`ChunkGenerator::populate`] is `populate`, with the one
/// difference described in `population`.
pub struct NetherGenerator {
    seed: u64,
    terrain: NetherTerrain,
}

impl NetherGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            terrain: NetherTerrain::new(seed),
        }
    }
}

impl ChunkGenerator for NetherGenerator {
    fn generate_base(&self, position: ChunkPosition) -> GeneratedChunk {
        let mut blocks = self.terrain.generate_base(position);
        caves::carve(&mut blocks, position, self.seed);
        let chunk = Chunk::from_raw(blocks);
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::hell(),
            items: Vec::new(),
            populated: false,
        }
    }

    fn populate(&self, source: ChunkPosition, chunks: [GeneratedChunk; 4]) -> [GeneratedChunk; 4] {
        let mut rand = source_random(self.seed, source);
        let [a, b, c, d] = population::lava_falls(source, self.seed, &mut rand, chunks);
        let mut rest = [
            (a.biomes, a.items, a.populated),
            (b.biomes, b.items, b.populated),
            (c.biomes, c.items, c.populated),
            (d.biomes, d.items, d.populated),
        ]
        .into_iter();
        let mut world = PopulationWorld::new(source, [a.chunk, b.chunk, c.chunk, d.chunk]);
        population::decorate(&mut world, &mut rand);
        let mut populated = world.into_chunks().map(|chunk| {
            let (biomes, items, populated) = rest.next().unwrap();
            GeneratedChunk {
                heightmap: Heightmap::from_chunk(&chunk),
                chunk,
                biomes,
                items,
                populated,
            }
        });
        populated[0].populated = true;
        populated
    }
}

use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Mutex;

use super::super::ChunkGenerator;
use super::biome::BiomeGenerator;
use super::caves;
use super::population;
use super::snow::place_snow;
use super::springs;
use super::surface::apply_surface;
use super::terrain::TerrainGenerator;
use super::world::PopulationWorld;
use crate::world::biome::BiomeMap;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;

/// Beta 1.7.3 `ChunkProviderGenerate`.
///
/// Generation has Beta's two stages. [`Self::generate_base`] builds a chunk's
/// terrain, surface, and caves on its own. [`Self::populate`] then decorates
/// a chunk once its three `+x`/`+z` neighbors exist, writing into all four.
/// The streaming world runs population as chunks arrive, as Beta does;
/// [`Self::generate`] produces one finished chunk in isolation.
pub struct OverworldGenerator {
    seed: u64,
    biomes: BiomeGenerator,
    terrain: TerrainGenerator,
    /// Base chunks for [`Self::generate_area`]. Neighboring calls to
    /// [`Self::generate`] rebuild overlapping 3×3 neighborhoods.
    base_cache: Mutex<VecDeque<(ChunkPosition, Chunk, BiomeMap)>>,
}

const BASE_CACHE_CAPACITY: usize = 32;

impl OverworldGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            biomes: BiomeGenerator::new(seed),
            terrain: TerrainGenerator::new(seed),
            base_cache: Mutex::new(VecDeque::new()),
        }
    }

    /// `provideChunk`: terrain, surface, and caves, not yet populated.
    pub fn generate_base(&self, position: ChunkPosition) -> GeneratedChunk {
        let biomes = self.biomes.generate(position);
        let mut blocks = self.terrain.generate_base(position, &biomes);
        apply_surface(&mut blocks, position, &biomes, &self.terrain);
        caves::carve(&mut blocks, position, self.seed);
        let chunk = Chunk::from_raw(blocks);
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes,
            items: Vec::new(),
            populated: false,
        }
    }

    /// `populate` for `source`. `chunks` are `source` and its `+x`, `+z`, and
    /// `+x+z` neighbors, in that order, and come back in the same order.
    pub fn populate(
        &self,
        source: ChunkPosition,
        chunks: [GeneratedChunk; 4],
    ) -> [GeneratedChunk; 4] {
        let [a, b, c, d] = chunks;
        let mut rest = [
            (a.biomes, a.items, a.populated),
            (b.biomes, b.items, b.populated),
            (c.biomes, c.items, c.populated),
            (d.biomes, d.items, d.populated),
        ]
        .into_iter();
        let mut world = PopulationWorld::new(source, [a.chunk, b.chunk, c.chunk, d.chunk]);
        let mut rand = population::source_random(self.seed, source);
        population::populate(&mut world, &self.terrain, &self.biomes, &mut rand);

        // `WorldGenLiquids` calls the block update system during population,
        // before snow. Temporarily give it the same four live chunks that
        // every preceding feature used, preserving metadata and pending ticks.
        let generated = world.into_chunks().map(|chunk| {
            let (biomes, items, populated) = rest.next().unwrap();
            GeneratedChunk {
                heightmap: Heightmap::from_chunk(&chunk),
                chunk,
                biomes,
                items,
                populated,
            }
        });
        let generated = springs::populate(source, self.seed, &mut rand, generated);

        let [a, b, c, d] = generated;
        let mut rest = [
            (a.biomes, a.items, a.populated),
            (b.biomes, b.items, b.populated),
            (c.biomes, c.items, c.populated),
            (d.biomes, d.items, d.populated),
        ]
        .into_iter();
        let mut world = PopulationWorld::new(source, [a.chunk, b.chunk, c.chunk, d.chunk]);
        place_snow(&mut world, &self.biomes);
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

    /// One finished chunk, built alone: base chunks for its 3×3 neighborhood,
    /// then the four population passes that write into it.
    ///
    /// Chunks built separately need not agree where they meet, since each
    /// runs its neighbors' passes over different surroundings. Use
    /// [`Self::generate_area`] for chunks that must fit together.
    pub fn generate(&self, position: ChunkPosition) -> GeneratedChunk {
        self.generate_area(position, 0).remove(&position).unwrap()
    }

    /// A consistent area, as a streaming world would build it: base chunks up
    /// to `radius + 1` from `center`, then every population pass whose chunks
    /// are all present, `x`-major. Chunks within `radius` are finished; the
    /// outer ring is partly populated.
    pub fn generate_area(
        &self,
        center: ChunkPosition,
        radius: i32,
    ) -> HashMap<ChunkPosition, GeneratedChunk> {
        super::super::generate_area_with(self, center, radius, |position| {
            self.cached_base(position)
        })
    }

    fn cached_base(&self, position: ChunkPosition) -> GeneratedChunk {
        let cached = self
            .base_cache
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.0 == position)
            .map(|(_, chunk, biomes)| (chunk.clone(), biomes.clone()));
        if let Some((chunk, biomes)) = cached {
            return GeneratedChunk {
                heightmap: Heightmap::from_chunk(&chunk),
                chunk,
                biomes,
                items: Vec::new(),
                populated: false,
            };
        }
        let generated = self.generate_base(position);
        let mut cache = self.base_cache.lock().unwrap();
        if cache.len() == BASE_CACHE_CAPACITY {
            cache.pop_front();
        }
        cache.push_back((position, generated.chunk.clone(), generated.biomes.clone()));
        generated
    }
}

impl ChunkGenerator for OverworldGenerator {
    fn generate_base(&self, position: ChunkPosition) -> GeneratedChunk {
        OverworldGenerator::generate_base(self, position)
    }

    fn populate(&self, source: ChunkPosition, chunks: [GeneratedChunk; 4]) -> [GeneratedChunk; 4] {
        OverworldGenerator::populate(self, source, chunks)
    }
}

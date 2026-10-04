//! Block ticks: the scheduler, random ticks, neighbor notifications, and each
//! block family's behavior, run against small hand-built worlds.

mod engine;
mod falling;
mod fire;
mod fluids;
mod leaves;
mod misc;
mod plants;
mod soil;
mod systems;

use bevy::math::IVec3;
use game::block::blocks::Block;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::block_ticks::BlockChange;
use game::world::block_ticks::BlockEvent;
use game::world::block_ticks::BlockTicks;
use game::world::block_ticks::TickEffect;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::lighting::LightCache;

pub fn generated(chunk: Chunk, biome: Biome) -> GeneratedChunk {
    GeneratedChunk {
        heightmap: Heightmap::from_chunk(&chunk),
        biomes: BiomeMap::from_cells(
            [Climate {
                temperature: 0.5,
                humidity: 0.5,
                biome,
            }; CHUNK_SIZE * CHUNK_SIZE],
        ),
        chunk,
        items: Vec::new(),
        populated: true,
    }
}

/// Empty chunks around the origin, a block tick state, and a clock.
pub struct TestWorld {
    pub chunks: WorldChunks,
    pub ticks: BlockTicks,
    pub light: LightCache,
    pub time: u64,
}

impl TestWorld {
    /// Chunks within `radius` of chunk (0, 0). Falling blocks need a radius
    /// of 2 to become entities; scheduled ticks need 1 around their cell.
    pub fn new(radius: i32) -> Self {
        Self::with_biome(radius, Biome::Plains)
    }

    pub fn with_biome(radius: i32, biome: Biome) -> Self {
        let mut chunks = WorldChunks::default();
        for x in -radius..=radius {
            for z in -radius..=radius {
                chunks.insert(ChunkPosition { x, z }, generated(Chunk::new(), biome));
            }
        }
        Self {
            chunks,
            ticks: BlockTicks::new(1234),
            light: LightCache::default(),
            time: 0,
        }
    }

    /// Write a block directly, without hooks, as world generation would.
    pub fn set(&mut self, position: IVec3, block: Block) {
        self.set_with_metadata(position, block, 0);
    }

    pub fn set_with_metadata(&mut self, position: IVec3, block: Block, metadata: u8) {
        self.chunks
            .set_block_with_metadata(position.x, position.y, position.z, block, metadata)
            .expect("test cells are loaded");
    }

    /// Fill the inclusive box between two corners.
    pub fn fill(&mut self, from: IVec3, to: IVec3, block: Block) {
        for x in from.x.min(to.x)..=from.x.max(to.x) {
            for y in from.y.min(to.y)..=from.y.max(to.y) {
                for z in from.z.min(to.z)..=from.z.max(to.z) {
                    self.set(IVec3::new(x, y, z), block);
                }
            }
        }
    }

    /// Replace a block the way the player does: write it, then let the tick
    /// pass run the hooks and notify the neighbors.
    pub fn place(&mut self, position: IVec3, block: Block) {
        let previous = self.block(position);
        let metadata = self.metadata(position);
        self.set(position, block);
        self.ticks.block_changed(position, previous, metadata);
        self.process_events();
    }

    pub fn event(&mut self, event: BlockEvent) {
        self.ticks.push_event(event);
        self.process_events();
    }

    pub fn process_events(&mut self) {
        self.ticks
            .process_events(&mut self.chunks, &mut self.light, self.time);
    }

    pub fn block(&self, position: IVec3) -> Block {
        self.chunks
            .block_at(position.x, position.y, position.z)
            .unwrap_or(Block::Air)
    }

    pub fn metadata(&self, position: IVec3) -> u8 {
        self.chunks.metadata_at(position.x, position.y, position.z)
    }

    /// Light every loaded chunk from its current blocks.
    pub fn relight(&mut self) {
        let positions: Vec<_> = self.chunks.positions().collect();
        for position in positions {
            self.light.relight(&self.chunks, position);
        }
    }

    /// Advance the clock `count` ticks, running scheduled ticks only.
    pub fn run(&mut self, count: u64) {
        self.run_with_random(count, &[]);
    }

    /// Advance the clock, also giving `random_chunks` their random ticks.
    pub fn run_with_random(&mut self, count: u64, random_chunks: &[ChunkPosition]) {
        for _ in 0..count {
            self.time += 1;
            self.ticks
                .tick(&mut self.chunks, &mut self.light, self.time, random_chunks);
        }
    }

    /// Run `position`'s `updateTick` `count` times, as that many random ticks
    /// landing on it would.
    pub fn random_ticks(&mut self, position: IVec3, count: usize) {
        for _ in 0..count {
            let mut world = self
                .ticks
                .world(&mut self.chunks, &mut self.light, self.time);
            world.update_tick(position);
        }
    }

    pub fn effects(&mut self) -> Vec<TickEffect> {
        self.ticks.take_effects()
    }

    pub fn changes(&mut self) -> Vec<BlockChange> {
        self.ticks.take_changes()
    }

    /// The blocks dropped as items since the last call.
    pub fn drops(&mut self) -> Vec<(IVec3, Block, u8)> {
        self.effects()
            .into_iter()
            .filter_map(|effect| match effect {
                TickEffect::Drop {
                    position,
                    block,
                    metadata,
                } => Some((position, block, metadata)),
                TickEffect::FallingBlock { .. } => None,
                TickEffect::PrimedTnt { .. } => None,
            })
            .collect()
    }
}

pub const fn at(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

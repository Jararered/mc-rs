//! Dimension-independent generation contract and shared generation utilities.
//!
//! Streaming schedules base chunks and nonoverlapping four-chunk population
//! passes through `ChunkGenerator`. A dimension supplies the implementation;
//! storage, lighting, and rendering consume the same chunk data.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;

pub mod math;
pub mod nether;
pub mod noise;
pub mod overworld;
mod world;

pub use world::PopulationWorld;

/// Raw block bytes of one chunk in [`Chunk::index`] order, the working form
/// of Beta's `byte[]` during base generation.
pub(crate) type RawBlocks = Vec<u8>;

pub(crate) const fn raw_index(x: usize, y: usize, z: usize) -> usize {
    (y * crate::world::chunk::CHUNK_SIZE + z) * crate::world::chunk::CHUNK_SIZE + x
}

/// Base terrain generation and decoration of a source plus its +x/+z neighbors.
/// Implementations must be safe to share across background generation jobs.
pub trait ChunkGenerator: Send + Sync + 'static {
    fn generate_base(&self, position: ChunkPosition) -> GeneratedChunk;

    /// Chunks arrive and return in `population_footprint(source)` order.
    /// Preserve neighboring population flags and set the source flag when done.
    fn populate(&self, source: ChunkPosition, chunks: [GeneratedChunk; 4]) -> [GeneratedChunk; 4];
}

/// Optional client startup override. Insert before `Startup` to select a
/// generator; otherwise streaming creates the Overworld using the saved seed.
/// This selects generation for one world, not a live dimension transition.
#[derive(Resource, Clone)]
pub struct WorldGeneration(pub Arc<dyn ChunkGenerator>);

impl WorldGeneration {
    pub fn new(generator: impl ChunkGenerator) -> Self {
        Self(Arc::new(generator))
    }
}

/// Shared footprint for offline generation and streaming's population jobs.
pub fn population_footprint(source: ChunkPosition) -> [ChunkPosition; 4] {
    [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dz)| ChunkPosition {
        x: source.x + dx,
        z: source.z + dz,
    })
}

/// Build a consistent area using the same two-stage contract as streaming.
/// The extra ring receives partial population; chunks inside `radius` finish.
pub fn generate_area(
    generator: &dyn ChunkGenerator,
    center: ChunkPosition,
    radius: i32,
) -> HashMap<ChunkPosition, GeneratedChunk> {
    generate_area_with(generator, center, radius, |position| {
        generator.generate_base(position)
    })
}

pub(crate) fn generate_area_with(
    generator: &dyn ChunkGenerator,
    center: ChunkPosition,
    radius: i32,
    mut base: impl FnMut(ChunkPosition) -> GeneratedChunk,
) -> HashMap<ChunkPosition, GeneratedChunk> {
    assert!(radius >= 0, "generation radius must be nonnegative");
    let reach = radius + 1;
    let mut area: HashMap<_, _> = (-reach..=reach)
        .flat_map(|dx| (-reach..=reach).map(move |dz| (dx, dz)))
        .map(|(dx, dz)| {
            let position = ChunkPosition {
                x: center.x + dx,
                z: center.z + dz,
            };
            (position, base(position))
        })
        .collect();
    for dx in -reach..reach {
        for dz in -reach..reach {
            let source = ChunkPosition {
                x: center.x + dx,
                z: center.z + dz,
            };
            let footprint = population_footprint(source);
            let taken = footprint.map(|position| area.remove(&position).unwrap());
            area.extend(footprint.into_iter().zip(generator.populate(source, taken)));
        }
    }
    area
}

/// One-chunk convenience API with a deterministic Overworld seed.
pub fn generate_chunk(position: ChunkPosition) -> Chunk {
    overworld::OverworldGenerator::new(0)
        .generate(position)
        .chunk
}

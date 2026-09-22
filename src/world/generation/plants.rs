//! Beta flower and tall-grass decoration.
//!
//! Ports the patch counts in `ChunkProviderGenerate.populate` and the scatter
//! loops in `WorldGenFlowers` and `WorldGenTallGrass`. Each source chunk draws
//! from the same random sequence the tree pass just finished, and only cells
//! inside the chunk being filled are written.

use crate::world::block::block::BlockId;
use crate::world::block::properties::plant_grows_on;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::Biome;
use super::java_random::JavaRandom;

pub(super) fn place_plants(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    biome: Biome,
    mut surface_air_y: impl FnMut(i32, i32) -> i32,
) {
    let yellow = match biome {
        Biome::Forest => 2,
        Biome::SeasonalForest => 4,
        Biome::Taiga => 2,
        Biome::Plains => 3,
        _ => 0,
    };
    for _ in 0..yellow {
        flower_patch(chunk, target, source, rand, BlockId::Dandelion);
    }

    let grass = match biome {
        Biome::Forest => 2,
        Biome::Rainforest => 10,
        Biome::SeasonalForest => 2,
        Biome::Taiga => 1,
        Biome::Plains => 10,
        _ => 0,
    };
    for _ in 0..grass {
        tall_grass_patch(chunk, target, source, rand, biome, &mut surface_air_y);
    }

    if rand.next_int(2) == 0 {
        flower_patch(chunk, target, source, rand, BlockId::Rose);
    }
}

fn flower_patch(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    block: BlockId,
) {
    let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    let origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
    let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    for _ in 0..64 {
        let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        try_plant(chunk, target, x, y, z, block);
    }
}

fn tall_grass_patch(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    biome: Biome,
    surface_air_y: &mut impl FnMut(i32, i32) -> i32,
) {
    let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    let mut origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
    let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    // Walk down through air. The pre-decoration surface is the same column
    // every neighbour replays, so a trunk in one chunk cannot stop the walk
    // early in that chunk only.
    let surface = surface_air_y(origin_x, origin_z);
    if origin_y >= surface {
        origin_y = (surface - 1).max(0);
    }
    // Rainforest rolls fern (metadata 2) unless `nextInt(3) == 0`.
    let block = if biome == Biome::Rainforest && rand.next_int(3) != 0 {
        BlockId::Fern
    } else {
        BlockId::TallGrass
    };
    for _ in 0..128 {
        let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        try_plant(chunk, target, x, y, z, block);
    }
}

fn try_plant(chunk: &mut Chunk, target: ChunkPos, x: i32, y: i32, z: i32, block: BlockId) {
    let local_x = x - target.x * CHUNK_SIZE as i32;
    let local_z = z - target.z * CHUNK_SIZE as i32;
    if !(0..CHUNK_SIZE as i32).contains(&local_x)
        || !(0..CHUNK_HEIGHT as i32).contains(&y)
        || !(0..CHUNK_SIZE as i32).contains(&local_z)
        || y == 0
    {
        return;
    }
    let (local_x, local_z) = (local_x as usize, local_z as usize);
    let y = y as usize;
    if chunk.get(local_x, y, local_z) != Some(BlockId::Air) {
        return;
    }
    if chunk
        .get(local_x, y - 1, local_z)
        .is_some_and(plant_grows_on)
    {
        chunk.set(local_x, y, local_z, block);
    }
}

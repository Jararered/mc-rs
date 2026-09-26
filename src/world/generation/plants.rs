//! Beta flower and tall-grass decoration.
//!
//! Ports the patch counts in `ChunkProviderGenerate.populate` and the scatter
//! loops in `WorldGenFlowers` and `WorldGenTallGrass`. Each source chunk draws
//! from the same random sequence the tree pass just finished, and only cells
//! inside the chunk being filled are written.

use crate::block::id::Id;
use crate::block::properties::plant_grows_on;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use std::collections::HashMap;

use super::biome::Biome;
use crate::random::JavaRandom;

pub(super) fn place_plants(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
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
        flower_patch(chunk, target, source, rand, Id::Dandelion);
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

    // Dead bushes are generated between tall grass and these low-frequency
    // flower/mushroom patches in Beta's populate method.
}

pub(super) fn place_plant_extras(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
    rand: &mut JavaRandom,
) {
    if rand.next_int(2) == 0 {
        flower_patch(chunk, target, source, rand, Id::Rose);
    }

    // ChunkProviderGenerate.populate rolls mushrooms after the flowers. Their
    // placement uses the same WorldGenFlowers scatter, but BlockMushroom can
    // stay on any opaque block and requires block light below 13. During
    // generation there is no propagated lighting yet, so direct sky exposure
    // is the useful discriminator: exposed positions have skylight 15, while
    // positions under a cave roof are dark enough absent placed light sources.
    if rand.next_int(4) == 0 {
        mushroom_patch(chunk, target, source, rand, Id::BrownMushroom);
    }
    if rand.next_int(8) == 0 {
        mushroom_patch(chunk, target, source, rand, Id::RedMushroom);
    }
}

/// Replay the desert `WorldGenDeadBush` attempts for `source` into `target`.
/// Each of the two populate attempts scatters four placements. Origins descend
/// through air and leaves, and bushes can only stay on sand.
pub(super) fn place_dead_bushes(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
    rand: &mut JavaRandom,
    biome: Biome,
    remote_chunks: &mut HashMap<ChunkPosition, Chunk>,
    remote_chunk: &impl Fn(ChunkPosition) -> Chunk,
) {
    if biome != Biome::Desert {
        return;
    }

    for _ in 0..2 {
        let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
        let mut origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
        let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;

        while origin_y > 0
            && matches!(
                block_at(
                    chunk,
                    target,
                    origin_x,
                    origin_y,
                    origin_z,
                    remote_chunks,
                    remote_chunk
                ),
                Id::Air | Id::Leaves | Id::BirchLeaves | Id::SpruceLeaves
            )
        {
            origin_y -= 1;
        }

        for _ in 0..4 {
            let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
            let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
            let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
            if !(0..CHUNK_HEIGHT as i32).contains(&y)
                || block_at(chunk, target, x, y, z, remote_chunks, remote_chunk) != Id::Air
                || block_at(chunk, target, x, y - 1, z, remote_chunks, remote_chunk) != Id::Sand
            {
                continue;
            }
            if let Some((local_x, local_z)) = local_column(target, x, z) {
                chunk.set(local_x, y as usize, local_z, Id::DeadBush);
            }
        }
    }
}

fn block_at(
    chunk: &Chunk,
    target: ChunkPosition,
    x: i32,
    y: i32,
    z: i32,
    remote_chunks: &mut HashMap<ChunkPosition, Chunk>,
    remote_chunk: &impl Fn(ChunkPosition) -> Chunk,
) -> Id {
    if !(0..CHUNK_HEIGHT as i32).contains(&y) {
        return Id::Air;
    }
    let pos = ChunkPosition {
        x: x.div_euclid(CHUNK_SIZE as i32),
        z: z.div_euclid(CHUNK_SIZE as i32),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
    if pos == target {
        chunk.get(local_x, y as usize, local_z).unwrap_or(Id::Air)
    } else {
        remote_chunks
            .entry(pos)
            .or_insert_with(|| remote_chunk(pos))
            .get(local_x, y as usize, local_z)
            .unwrap_or(Id::Air)
    }
}

fn local_column(target: ChunkPosition, x: i32, z: i32) -> Option<(usize, usize)> {
    let local_x = x - target.x * CHUNK_SIZE as i32;
    let local_z = z - target.z * CHUNK_SIZE as i32;
    ((0..CHUNK_SIZE as i32).contains(&local_x) && (0..CHUNK_SIZE as i32).contains(&local_z))
        .then_some((local_x as usize, local_z as usize))
}

fn flower_patch(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
    rand: &mut JavaRandom,
    block: Id,
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

fn mushroom_patch(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
    rand: &mut JavaRandom,
    block: Id,
) {
    let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    let origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
    let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    for _ in 0..64 {
        let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        try_mushroom(chunk, target, x, y, z, block);
    }
}

fn try_mushroom(chunk: &mut Chunk, target: ChunkPosition, x: i32, y: i32, z: i32, block: Id) {
    let local_x = x - target.x * CHUNK_SIZE as i32;
    let local_z = z - target.z * CHUNK_SIZE as i32;
    if !(0..CHUNK_SIZE as i32).contains(&local_x)
        || !(0..CHUNK_HEIGHT as i32).contains(&y)
        || !(0..CHUNK_SIZE as i32).contains(&local_z)
        || y == 0
    {
        return;
    }
    let (local_x, local_z, y) = (local_x as usize, local_z as usize, y as usize);
    if chunk.get(local_x, y, local_z) != Some(Id::Air)
        || !(y + 1..CHUNK_HEIGHT).any(|above| {
            chunk
                .get(local_x, above, local_z)
                .is_some_and(|b| b != Id::Air)
        })
        || !chunk
            .get(local_x, y - 1, local_z)
            .is_some_and(crate::block::properties::is_opaque_cube)
    {
        return;
    }
    chunk.set(local_x, y, local_z, block);
}

fn tall_grass_patch(
    chunk: &mut Chunk,
    target: ChunkPosition,
    source: ChunkPosition,
    rand: &mut JavaRandom,
    biome: Biome,
    surface_air_y: &mut impl FnMut(i32, i32) -> i32,
) {
    let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    let mut origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
    let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(CHUNK_SIZE as u32) as i32 + 8;
    // Scatter offsets are within seven blocks. An out-of-range patch still
    // consumes its random draws, but needs no remote ground lookup.
    let target_x = target.x * CHUNK_SIZE as i32;
    let target_z = target.z * CHUNK_SIZE as i32;
    if origin_x + 7 >= target_x
        && origin_x - 7 < target_x + CHUNK_SIZE as i32
        && origin_z + 7 >= target_z
        && origin_z - 7 < target_z + CHUNK_SIZE as i32
    {
        // Walk down through air. The pre-decoration surface is the same column
        // every neighbour replays, so a trunk in one chunk cannot stop the walk
        // early in that chunk only.
        let surface = surface_air_y(origin_x, origin_z);
        if origin_y >= surface {
            origin_y = (surface - 1).max(0);
        }
    }
    // Rainforest rolls fern (metadata 2) unless `nextInt(3) == 0`.
    let block = if biome == Biome::Rainforest && rand.next_int(3) != 0 {
        Id::Fern
    } else {
        Id::TallGrass
    };
    for _ in 0..128 {
        let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        try_plant(chunk, target, x, y, z, block);
    }
}

fn try_plant(chunk: &mut Chunk, target: ChunkPosition, x: i32, y: i32, z: i32, block: Id) {
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
    if chunk.get(local_x, y, local_z) != Some(Id::Air) {
        return;
    }
    if chunk
        .get(local_x, y - 1, local_z)
        .is_some_and(plant_grows_on)
    {
        chunk.set(local_x, y, local_z, block);
    }
}

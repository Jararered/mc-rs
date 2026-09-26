//! Beta pumpkin patches from `WorldGenPumpkin`.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::block::id::Id;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

/// Replay the rare pumpkin patch for `source` into `target` in any biome.
/// The 3x3 source neighbourhood covers the patch's maximum seven-block scatter.
pub(super) fn place_pumpkins(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    placed: &mut HashSet<(i32, i32, i32)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) {
    if rand.next_int(32) != 0 {
        return;
    }

    let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
    let origin_y = rand.next_int(128) as i32;
    let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
    for _ in 0..64 {
        let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let position = (x, y, z);
        if !(0..CHUNK_HEIGHT as i32).contains(&y) || placed.contains(&position) {
            continue;
        }
        if block_at(chunk, target, x, y, z, remote_chunks, remote_chunk) != Id::Air
            || block_at(chunk, target, x, y - 1, z, remote_chunks, remote_chunk) != Id::Grass
        {
            continue;
        }

        let pumpkin = Id::pumpkin_from_metadata(rand.next_int(4));
        placed.insert(position);
        if let Some((local_x, local_z)) = local_column(target, x, z) {
            chunk.set(local_x, y as usize, local_z, pumpkin);
        }
    }
}

fn block_at(
    chunk: &Chunk,
    target: ChunkPos,
    x: i32,
    y: i32,
    z: i32,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) -> Id {
    if !(0..CHUNK_HEIGHT as i32).contains(&y) {
        return Id::Air;
    }
    let position = ChunkPos {
        x: x.div_euclid(CHUNK_SIZE as i32),
        z: z.div_euclid(CHUNK_SIZE as i32),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
    if position == target {
        chunk.get(local_x, y as usize, local_z).unwrap_or(Id::Air)
    } else {
        remote_chunks
            .entry(position)
            .or_insert_with(|| remote_chunk(position))
            .get(local_x, y as usize, local_z)
            .unwrap_or(Id::Air)
    }
}

fn local_column(target: ChunkPos, x: i32, z: i32) -> Option<(usize, usize)> {
    let local_x = x - target.x * CHUNK_SIZE as i32;
    let local_z = z - target.z * CHUNK_SIZE as i32;
    ((0..CHUNK_SIZE as i32).contains(&local_x) && (0..CHUNK_SIZE as i32).contains(&local_z))
        .then_some((local_x as usize, local_z as usize))
}

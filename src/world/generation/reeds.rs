//! Beta reed (sugar cane) patches from `WorldGenReed`.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::random::JavaRandom;
use crate::world::block::block::BlockId;
use crate::world::block::properties::sugar_cane_can_stay;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

/// Replay the ten reed attempts for `source`, writing every segment whose
/// column reaches `target`. Reed scatter extends at most three blocks from its
/// origin, so the usual 3x3 source neighborhood covers all target overlap.
pub(super) fn place_reeds(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    placed: &mut HashSet<(i32, i32, i32)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) {
    for _ in 0..10 {
        let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
        let origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
        let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;

        for _ in 0..20 {
            let x = origin_x + rand.next_int(4) as i32 - rand.next_int(4) as i32;
            let z = origin_z + rand.next_int(4) as i32 - rand.next_int(4) as i32;
            let y = origin_y;
            if !(0..CHUNK_HEIGHT as i32).contains(&y)
                || block_at(chunk, target, x, y, z, placed, remote_chunks, remote_chunk)
                    != BlockId::Air
            {
                continue;
            }

            let water = adjacent_water(
                chunk,
                target,
                x,
                y - 1,
                z,
                placed,
                remote_chunks,
                remote_chunk,
            );
            if !water.into_iter().any(|is_water| is_water) {
                continue;
            }

            let height_bound = rand.next_int(3) + 1;
            let height = 2 + rand.next_int(height_bound) as i32;
            for offset in 0..height {
                let cane_y = y + offset;
                if !(0..CHUNK_HEIGHT as i32).contains(&cane_y) {
                    continue;
                }
                let below = block_at(
                    chunk,
                    target,
                    x,
                    cane_y - 1,
                    z,
                    placed,
                    remote_chunks,
                    remote_chunk,
                );
                if !sugar_cane_can_stay(below, water) {
                    continue;
                }
                placed.insert((x, cane_y, z));
                if let Some((local_x, local_z)) = local_column(target, x, z) {
                    chunk.set(local_x, cane_y as usize, local_z, BlockId::SugarCane);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn adjacent_water(
    chunk: &Chunk,
    target: ChunkPos,
    x: i32,
    y: i32,
    z: i32,
    placed: &HashSet<(i32, i32, i32)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) -> [bool; 4] {
    [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)].map(|(x, z)| {
        matches!(
            block_at(chunk, target, x, y, z, placed, remote_chunks, remote_chunk),
            BlockId::Water | BlockId::FlowingWater
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn block_at(
    chunk: &Chunk,
    target: ChunkPos,
    x: i32,
    y: i32,
    z: i32,
    placed: &HashSet<(i32, i32, i32)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) -> BlockId {
    if !(0..CHUNK_HEIGHT as i32).contains(&y) {
        return BlockId::Air;
    }
    if placed.contains(&(x, y, z)) {
        return BlockId::SugarCane;
    }
    let pos = ChunkPos {
        x: x.div_euclid(CHUNK_SIZE as i32),
        z: z.div_euclid(CHUNK_SIZE as i32),
    };
    let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
    if pos == target {
        chunk
            .get(local_x, y as usize, local_z)
            .unwrap_or(BlockId::Air)
    } else {
        remote_chunks
            .entry(pos)
            .or_insert_with(|| remote_chunk(pos))
            .get(local_x, y as usize, local_z)
            .unwrap_or(BlockId::Air)
    }
}

fn local_column(target: ChunkPos, x: i32, z: i32) -> Option<(usize, usize)> {
    let local_x = x - target.x * CHUNK_SIZE as i32;
    let local_z = z - target.z * CHUNK_SIZE as i32;
    ((0..CHUNK_SIZE as i32).contains(&local_x) && (0..CHUNK_SIZE as i32).contains(&local_z))
        .then_some((local_x as usize, local_z as usize))
}

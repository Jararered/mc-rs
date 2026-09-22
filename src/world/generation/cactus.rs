//! Beta desert cactus patches from `WorldGenCactus`.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::random::JavaRandom;
use crate::world::block::block::BlockId;
use crate::world::block::properties::cactus_can_stay;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::Biome;

/// Replay the desert population pass for each source chunk whose cactus
/// attempts can reach `target`. Cactus attempts scatter at most seven blocks,
/// so the same 3x3 source neighbourhood used by plants covers every overlap.
pub(super) fn place_cacti(
    chunk: &mut Chunk,
    target: ChunkPos,
    source: ChunkPos,
    rand: &mut JavaRandom,
    biome: Biome,
    placed: &mut HashSet<(i32, i32, i32)>,
    remote_chunks: &mut HashMap<ChunkPos, Chunk>,
    remote_chunk: &impl Fn(ChunkPos) -> Chunk,
) {
    if biome != Biome::Desert {
        return;
    }
    for _ in 0..10 {
        let origin_x = source.x * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
        let origin_y = rand.next_int(CHUNK_HEIGHT as u32) as i32;
        let origin_z = source.z * CHUNK_SIZE as i32 + rand.next_int(16) as i32 + 8;
        for _ in 0..10 {
            let x = origin_x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
            let y = origin_y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
            let z = origin_z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
            if !(0..CHUNK_HEIGHT as i32).contains(&y)
                || block_at(chunk, target, x, y, z, placed, remote_chunks, remote_chunk)
                    != BlockId::Air
            {
                continue;
            }

            let height_bound = rand.next_int(3) + 1;
            let height = 1 + rand.next_int(height_bound) as i32;
            for offset in 0..height {
                let cactus_y = y + offset;
                if !(0..CHUNK_HEIGHT as i32).contains(&cactus_y) {
                    continue;
                }
                let below = block_at(
                    chunk,
                    target,
                    x,
                    cactus_y - 1,
                    z,
                    placed,
                    remote_chunks,
                    remote_chunk,
                );
                let neighbors = [
                    block_at(
                        chunk,
                        target,
                        x - 1,
                        cactus_y,
                        z,
                        placed,
                        remote_chunks,
                        remote_chunk,
                    ),
                    block_at(
                        chunk,
                        target,
                        x + 1,
                        cactus_y,
                        z,
                        placed,
                        remote_chunks,
                        remote_chunk,
                    ),
                    block_at(
                        chunk,
                        target,
                        x,
                        cactus_y,
                        z - 1,
                        placed,
                        remote_chunks,
                        remote_chunk,
                    ),
                    block_at(
                        chunk,
                        target,
                        x,
                        cactus_y,
                        z + 1,
                        placed,
                        remote_chunks,
                        remote_chunk,
                    ),
                ];
                if !cactus_can_stay(below, neighbors) {
                    continue;
                }
                placed.insert((x, cactus_y, z));
                if let Some((local_x, local_z)) = local_column(target, x, z) {
                    chunk.set(local_x, cactus_y as usize, local_z, BlockId::Cactus);
                }
            }
        }
    }
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
        return BlockId::Cactus;
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

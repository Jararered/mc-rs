use crate::block::id::Id;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;

use super::terrain::RawBlocks;
use super::terrain::TerrainGenerator;
use super::terrain::raw_index;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::biome::BiomeMap;

const SEA_LEVEL: i32 = 64;

/// `provideChunk` seeds the chunk random before terrain; only this pass uses it.
fn surface_chunk_seed(position: ChunkPosition) -> u64 {
    i64::from(position.x)
        .wrapping_mul(341_873_128_712)
        .wrapping_add(i64::from(position.z).wrapping_mul(132_897_987_541)) as u64
}

/// Biome `topBlock` and `fillerBlock`. Only the two deserts differ.
fn biome_blocks(biome: Biome) -> (Id, Id) {
    if matches!(biome, Biome::Desert | Biome::IceDesert) {
        (Id::Sand, Id::Sand)
    } else {
        (Id::Grass, Id::Dirt)
    }
}

/// `ChunkProviderGenerate.replaceBlocksForBiome`: topsoil, beaches, sandstone,
/// and the bedrock floor.
pub(super) fn apply_surface(
    blocks: &mut RawBlocks,
    position: ChunkPosition,
    biomes: &BiomeMap,
    terrain: &TerrainGenerator,
) {
    let mut random = JavaRandom::new(surface_chunk_seed(position));
    let origin_x = f64::from(position.x * CHUNK_SIZE as i32);
    let origin_z = f64::from(position.z * CHUNK_SIZE as i32);
    let scale = 0.03125;
    // Beta fills these as grids; all three index `x * 16 + z`. Sand puts world
    // z in the grid's y axis, and gravel takes the 2D path, which drops its y.
    let sand = terrain.shore.grid(
        [origin_x, origin_z, 0.0],
        [CHUNK_SIZE, CHUNK_SIZE, 1],
        [scale, scale, 1.0],
    );
    let gravel = terrain.shore.grid(
        [origin_x, 109.0134, origin_z],
        [CHUNK_SIZE, 1, CHUNK_SIZE],
        [scale, 1.0, scale],
    );
    let stone = terrain.surface_elevation.grid(
        [origin_x, origin_z, 0.0],
        [CHUNK_SIZE, CHUNK_SIZE, 1],
        [scale * 2.0; 3],
    );

    let air = Id::Air.as_u8();
    let stone_id = Id::Stone.as_u8();
    // Columns go z-outer, x-inner, so each column draws the same random values
    // it does in Beta.
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let column = x * CHUNK_SIZE + z;
            let (biome_top, biome_filler) = biome_blocks(biomes.get(x, z).biome);
            let sandy = sand[column] + random.next_double() * 0.2 > 0.0;
            let gravelly = gravel[column] + random.next_double() * 0.2 > 3.0;
            let depth = (stone[column] / 3.0 + 3.0 + random.next_double() * 0.25) as i32;
            let mut remaining = -1;
            let mut top = biome_top;
            let mut filler = biome_filler;

            for y in (0..CHUNK_HEIGHT as i32).rev() {
                let index = raw_index(x, y as usize, z);
                if y <= random.next_int(5) as i32 {
                    blocks[index] = Id::Bedrock.as_u8();
                    continue;
                }
                let block = blocks[index];
                // Water and ice neither reset nor continue the soil run.
                if block == air {
                    remaining = -1;
                } else if block == stone_id {
                    if remaining == -1 {
                        if depth <= 0 {
                            top = Id::Air;
                            filler = Id::Stone;
                        } else if (SEA_LEVEL - 4..=SEA_LEVEL + 1).contains(&y) {
                            top = biome_top;
                            filler = biome_filler;
                            if gravelly {
                                top = Id::Air;
                                filler = Id::Gravel;
                            }
                            if sandy {
                                top = Id::Sand;
                                filler = Id::Sand;
                            }
                        }
                        if y < SEA_LEVEL && top == Id::Air {
                            top = Id::Water;
                        }
                        remaining = depth;
                        blocks[index] = if y >= SEA_LEVEL - 1 { top } else { filler }.as_u8();
                    } else if remaining > 0 {
                        remaining -= 1;
                        blocks[index] = filler.as_u8();
                        if remaining == 0 && filler == Id::Sand {
                            remaining = random.next_int(4) as i32;
                            filler = Id::Sandstone;
                        }
                    }
                }
            }
        }
    }
}

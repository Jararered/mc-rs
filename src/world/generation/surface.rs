use crate::world::block::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::BiomeMap;
use super::biome::Climate;
use super::java_random::JavaRandom;
use super::terrain::TerrainGenerator;

const SEA_LEVEL: usize = 64;

fn surface_chunk_seed(position: ChunkPos) -> u64 {
    (position.x as i64 as u64)
        .wrapping_mul(0x4f9939f508)
        .wrapping_add((position.z as i64 as u64).wrapping_mul(0x1ef1565bd5))
}

/// Advance the surface RNG by one column. Must stay in lockstep with
/// [`apply_surface`]: three `next_double` jitters plus a bedrock roll per Y.
fn skip_column_rng(random: &mut JavaRandom) {
    let _ = random.next_double();
    let _ = random.next_double();
    let _ = random.next_double();
    for _ in 0..CHUNK_HEIGHT {
        let _ = random.next_int(5);
    }
}

/// Tree origin Y and the block under it for a world column, matching the
/// surface [`apply_surface`] would produce in the chunk that owns the column.
///
/// Decoration uses this for trunks that start outside the chunk being filled,
/// so a beach is sand in every neighbour rather than guessed as grass.
pub(super) fn ground_column(
    terrain: &TerrainGenerator,
    world_x: i32,
    world_z: i32,
    climate_at: impl Fn(f64, f64) -> Climate,
) -> (i32, BlockId) {
    let chunk = ChunkPos {
        x: world_x.div_euclid(CHUNK_SIZE as i32),
        z: world_z.div_euclid(CHUNK_SIZE as i32),
    };
    let local_x = world_x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = world_z.rem_euclid(CHUNK_SIZE as i32) as usize;
    let mut random = JavaRandom::new(surface_chunk_seed(chunk));
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            if x == local_x && z == local_z {
                return column_after_surface(terrain, world_x, world_z, &mut random, climate_at);
            }
            skip_column_rng(&mut random);
        }
    }
    unreachable!("local column is inside the chunk")
}

fn column_after_surface(
    terrain: &TerrainGenerator,
    world_x: i32,
    world_z: i32,
    random: &mut JavaRandom,
    climate_at: impl Fn(f64, f64) -> Climate,
) -> (i32, BlockId) {
    let wx = world_x as f64;
    let wz = world_z as f64;
    let sandy = terrain
        .shore
        .sample_3d(wx, wz, 0.0, [0.03125, 0.03125, 1.0])
        + random.next_double() * 0.2
        > 0.0;
    let gravelly =
        terrain.shore.sample_2d(wz, wx, 0.03125, 0.03125) + random.next_double() * 0.2 > 3.0;
    let elevation = (terrain
        .surface_elevation
        .sample_3d(wx, wz, 0.0, [0.0625, 0.0625, 0.0625])
        / 3.0
        + 3.0
        + random.next_double() * 0.25) as i32;

    let top_solid = terrain.column_top(world_x, world_z, &climate_at) as i32;
    let sea = SEA_LEVEL as i32;
    if top_solid >= sea - 1 {
        let ground = if elevation <= 0 {
            BlockId::Stone
        } else {
            let mut top = BlockId::Grass;
            if top_solid <= sea + 1 {
                if gravelly {
                    top = BlockId::Gravel;
                }
                if sandy {
                    top = BlockId::Sand;
                }
            }
            top
        };
        (top_solid + 1, ground)
    } else {
        let temperature = climate_at(wx, wz).temperature;
        let ground = if temperature < 0.5 {
            BlockId::Ice
        } else {
            BlockId::Water
        };
        (top_solid.max(sea - 1) + 1, ground)
    }
}

/// Replace exposed stone with Beta-style topsoil, beaches, and bedrock.
pub(super) fn apply_surface(
    chunk: &mut Chunk,
    position: ChunkPos,
    biomes: &BiomeMap,
    terrain: &TerrainGenerator,
) {
    let mut random = JavaRandom::new(surface_chunk_seed(position));

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let world_x = (position.x * CHUNK_SIZE as i32 + x as i32) as f64;
            let world_z = (position.z * CHUNK_SIZE as i32 + z as i32) as f64;
            let sandy = terrain
                .shore
                .sample_3d(world_x, world_z, 0.0, [0.03125, 0.03125, 1.0])
                + random.next_double() * 0.2
                > 0.0;
            let gravelly = terrain.shore.sample_2d(world_z, world_x, 0.03125, 0.03125)
                + random.next_double() * 0.2
                > 3.0;
            let elevation = (terrain.surface_elevation.sample_3d(
                world_x,
                world_z,
                0.0,
                [0.0625, 0.0625, 0.0625],
            ) / 3.0
                + 3.0
                + random.next_double() * 0.25) as i32;

            let mut remaining = -1;
            let mut filler = BlockId::Dirt;
            for y in (0..CHUNK_HEIGHT).rev() {
                let bedrock_roll = random.next_int(5) as usize;
                if y <= bedrock_roll {
                    chunk.set(x, y, z, BlockId::Bedrock);
                    continue;
                }

                match chunk.get(x, y, z).unwrap() {
                    BlockId::Air | BlockId::Water => {
                        remaining = -1;
                    }
                    BlockId::Stone if remaining == -1 => {
                        remaining = elevation;
                        if elevation <= 0 {
                            filler = BlockId::Stone;
                            if y < SEA_LEVEL {
                                chunk.set(x, y, z, BlockId::Water);
                            }
                        } else {
                            let mut top = BlockId::Grass;
                            filler = BlockId::Dirt;
                            if y <= SEA_LEVEL + 1 {
                                if gravelly {
                                    top = BlockId::Gravel;
                                    filler = BlockId::Gravel;
                                }
                                if sandy {
                                    top = BlockId::Sand;
                                    filler = BlockId::Sand;
                                }
                            }
                            // Beta only places the biome top (grass) at y >= 63.
                            // Lower underwater surfaces get filler, so lake beds
                            // are dirt (or sand/gravel) rather than grass.
                            if y >= SEA_LEVEL - 1 {
                                chunk.set(x, y, z, top);
                            } else {
                                chunk.set(x, y, z, filler);
                            }
                        }
                    }
                    BlockId::Stone if remaining > 0 => {
                        remaining -= 1;
                        chunk.set(x, y, z, filler);
                    }
                    _ => {}
                }
            }

            if biomes.get(x, z).temperature < 0.5
                && chunk.get(x, SEA_LEVEL - 1, z) == Some(BlockId::Water)
            {
                chunk.set(x, SEA_LEVEL - 1, z, BlockId::Ice);
            }
        }
    }
}

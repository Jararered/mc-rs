use crate::block::id::Id;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;

use super::biome::Biome;
use super::biome::BiomeMap;
use super::terrain::TerrainGenerator;
use crate::random::JavaRandom;

const SEA_LEVEL: usize = 64;

fn surface_chunk_seed(position: ChunkPosition) -> u64 {
    (position.x as i64 as u64)
        .wrapping_mul(0x4f9939f508)
        .wrapping_add((position.z as i64 as u64).wrapping_mul(0x1ef1565bd5))
}

/// Replace exposed stone with Beta-style topsoil, beaches, and bedrock.
pub(super) fn apply_surface(
    chunk: &mut Chunk,
    position: ChunkPosition,
    biomes: &BiomeMap,
    terrain: &TerrainGenerator,
) {
    let mut random = JavaRandom::new(surface_chunk_seed(position));

    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            let world_x = (position.x * CHUNK_SIZE as i32 + x as i32) as f64;
            let world_z = (position.z * CHUNK_SIZE as i32 + z as i32) as f64;
            let shore_sandy =
                terrain
                    .shore
                    .sample_3d(world_x, world_z, 0.0, [0.03125, 0.03125, 1.0])
                    + random.next_double() * 0.2
                    > 0.0;
            // Beta's desert and ice-desert biomes use sand for both their top
            // and filler blocks, independent of the shoreline noise.
            let desert_surface = matches!(biomes.get(x, z).biome, Biome::Desert | Biome::IceDesert);
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
            let mut filler = Id::Dirt;
            for y in (0..CHUNK_HEIGHT).rev() {
                let bedrock_roll = random.next_int(5) as usize;
                if y <= bedrock_roll {
                    chunk.set(x, y, z, Id::Bedrock);
                    continue;
                }

                match chunk.get(x, y, z).unwrap() {
                    Id::Air | Id::Water => {
                        remaining = -1;
                    }
                    Id::Stone if remaining == -1 => {
                        remaining = elevation;
                        if elevation <= 0 {
                            filler = Id::Stone;
                            if y < SEA_LEVEL {
                                chunk.set(x, y, z, Id::Water);
                            }
                        } else {
                            let mut top = if desert_surface { Id::Sand } else { Id::Grass };
                            filler = if desert_surface { Id::Sand } else { Id::Dirt };
                            if y <= SEA_LEVEL + 1 {
                                if gravelly {
                                    top = Id::Gravel;
                                    filler = Id::Gravel;
                                }
                                if shore_sandy {
                                    top = Id::Sand;
                                    filler = Id::Sand;
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
                    Id::Stone if remaining > 0 => {
                        remaining -= 1;
                        chunk.set(x, y, z, filler);
                        if remaining == 0 && filler == Id::Sand {
                            remaining = random.next_int(4) as i32;
                            filler = Id::Sandstone;
                        }
                    }
                    _ => {}
                }
            }

            if biomes.get(x, z).temperature < 0.5
                && chunk.get(x, SEA_LEVEL - 1, z) == Some(Id::Water)
            {
                chunk.set(x, SEA_LEVEL - 1, z, Id::Ice);
            }
        }
    }
}

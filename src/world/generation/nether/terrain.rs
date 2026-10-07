//! `ChunkProviderHell`'s base terrain: `generateNetherTerrain`, the density
//! grid `func_4057_a`, and the surface pass `func_4058_b`.

use crate::block::blocks::Block;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;

use super::super::RawBlocks;
use super::super::raw_index;
use super::noise::PerlinOctaves;

const GRID: usize = 5;
const VERTICAL_GRID: usize = 17;
/// Everything open below this is the lava sea.
const LAVA_LEVEL: usize = 32;
/// The band around this height is where soul sand and gravel replace the
/// surface, as beaches do around the Overworld's sea level.
const SHORE_LEVEL: i32 = 64;

pub(super) struct NetherTerrain {
    min_limit: PerlinOctaves,
    max_limit: PerlinOctaves,
    main_limit: PerlinOctaves,
    /// Soul sand and, sampled on another plane, gravel.
    shore: PerlinOctaves,
    surface_depth: PerlinOctaves,
}

impl NetherTerrain {
    pub fn new(seed: u64) -> Self {
        let mut random = JavaRandom::new(seed);
        // Preserve the reference's octave construction order: it affects every permutation.
        let min_limit = PerlinOctaves::new(&mut random, 16);
        let max_limit = PerlinOctaves::new(&mut random, 16);
        let main_limit = PerlinOctaves::new(&mut random, 8);
        let shore = PerlinOctaves::new(&mut random, 4);
        let surface_depth = PerlinOctaves::new(&mut random, 4);
        // Beta builds two more generators (10 and 16 octaves) and samples
        // them, but nothing reads the result and nothing is constructed after
        // them, so they are left out.
        Self {
            min_limit,
            max_limit,
            main_limit,
            shore,
            surface_depth,
        }
    }

    /// `provideChunk` up to the caves: terrain, then the surface pass on the
    /// chunk-seeded random.
    pub fn generate_base(&self, position: ChunkPosition) -> RawBlocks {
        let mut blocks = self.terrain(position);
        self.surface(&mut blocks, position);
        blocks
    }

    /// `generateNetherTerrain`: netherrack inside the interpolated density
    /// surface and still lava in whatever is open below the lava level.
    fn terrain(&self, position: ChunkPosition) -> RawBlocks {
        let density = self.density(position);
        let index = |x: usize, z: usize, y: usize| (x * GRID + z) * VERTICAL_GRID + y;
        let mut blocks = vec![Block::Air.as_u8(); CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE];
        for gx in 0..4 {
            for gz in 0..4 {
                for gy in 0..CHUNK_HEIGHT / 8 {
                    let mut d00 = density[index(gx, gz, gy)];
                    let mut d01 = density[index(gx, gz + 1, gy)];
                    let mut d10 = density[index(gx + 1, gz, gy)];
                    let mut d11 = density[index(gx + 1, gz + 1, gy)];
                    let step00 = (density[index(gx, gz, gy + 1)] - d00) * 0.125;
                    let step01 = (density[index(gx, gz + 1, gy + 1)] - d01) * 0.125;
                    let step10 = (density[index(gx + 1, gz, gy + 1)] - d10) * 0.125;
                    let step11 = (density[index(gx + 1, gz + 1, gy + 1)] - d11) * 0.125;
                    for dy in 0..8 {
                        let y = gy * 8 + dy;
                        let mut near = d00;
                        let mut far = d01;
                        let near_step = (d10 - d00) * 0.25;
                        let far_step = (d11 - d01) * 0.25;
                        for dx in 0..4 {
                            let x = gx * 4 + dx;
                            let mut value = near;
                            let z_step = (far - near) * 0.25;
                            for dz in 0..4 {
                                let z = gz * 4 + dz;
                                let mut block = Block::Air;
                                if y < LAVA_LEVEL {
                                    block = Block::Lava;
                                }
                                if value > 0.0 {
                                    block = Block::Netherrack;
                                }
                                blocks[raw_index(x, y, z)] = block.as_u8();
                                value += z_step;
                            }
                            near += near_step;
                            far += far_step;
                        }
                        d00 += step00;
                        d01 += step01;
                        d10 += step10;
                        d11 += step11;
                    }
                }
            }
        }
        blocks
    }

    /// `func_4057_a`: the 5×17×5 density grid, indexed `(x * 5 + z) * 17 + y`.
    fn density(&self, position: ChunkPosition) -> Vec<f64> {
        const HORIZONTAL: f64 = 684.412;
        const VERTICAL: f64 = 2053.236;
        let origin = [f64::from(position.x * 4), 0.0, f64::from(position.z * 4)];
        let size = [GRID, VERTICAL_GRID, GRID];
        let main = self.main_limit.grid(
            origin,
            size,
            [HORIZONTAL / 80.0, VERTICAL / 60.0, HORIZONTAL / 80.0],
        );
        let min = self
            .min_limit
            .grid(origin, size, [HORIZONTAL, VERTICAL, HORIZONTAL]);
        let max = self
            .max_limit
            .grid(origin, size, [HORIZONTAL, VERTICAL, HORIZONTAL]);

        // Solid toward the floor and the roof, with three bands of caverns
        // between them.
        let mut offsets = [0.0f64; VERTICAL_GRID];
        for (gy, offset) in offsets.iter_mut().enumerate() {
            *offset = (gy as f64 * std::f64::consts::PI * 6.0 / VERTICAL_GRID as f64).cos() * 2.0;
            let mut edge = gy as f64;
            if gy > VERTICAL_GRID / 2 {
                edge = (VERTICAL_GRID - 1 - gy) as f64;
            }
            if edge < 4.0 {
                edge = 4.0 - edge;
                *offset -= edge * edge * edge * 10.0;
            }
        }

        let mut density = vec![0.0; GRID * VERTICAL_GRID * GRID];
        let mut index = 0;
        for _ in 0..GRID * GRID {
            for (gy, offset) in offsets.iter().enumerate() {
                let low = min[index] / 512.0;
                let high = max[index] / 512.0;
                let blend = (main[index] / 10.0 + 1.0) / 2.0;
                let mut value = if blend < 0.0 {
                    low
                } else if blend > 1.0 {
                    high
                } else {
                    low + (high - low) * blend
                };
                value -= offset;
                if gy > VERTICAL_GRID - 4 {
                    // Beta divides in float before widening.
                    let top = f64::from((gy - (VERTICAL_GRID - 4)) as f32 / 3.0);
                    value = value * (1.0 - top) + -10.0 * top;
                }
                density[index] = value;
                index += 1;
            }
        }
        density
    }

    /// `func_4058_b`: soul sand and gravel around the shore level, and the
    /// ragged bedrock floor and roof.
    fn surface(&self, blocks: &mut RawBlocks, position: ChunkPosition) {
        // `provideChunk` seeds the chunk random; only this pass draws from it.
        let seed = i64::from(position.x)
            .wrapping_mul(341_873_128_712)
            .wrapping_add(i64::from(position.z).wrapping_mul(132_897_987_541));
        let mut random = JavaRandom::new(seed as u64);
        let origin_x = f64::from(position.x * CHUNK_SIZE as i32);
        let origin_z = f64::from(position.z * CHUNK_SIZE as i32);
        let scale = 0.03125;
        // All three index `x * 16 + z`, as in the Overworld's surface pass.
        let soul_sand = self.shore.grid(
            [origin_x, origin_z, 0.0],
            [CHUNK_SIZE, CHUNK_SIZE, 1],
            [scale, scale, 1.0],
        );
        let gravel = self.shore.grid(
            [origin_x, 109.0134, origin_z],
            [CHUNK_SIZE, 1, CHUNK_SIZE],
            [scale, 1.0, scale],
        );
        let depths = self.surface_depth.grid(
            [origin_x, origin_z, 0.0],
            [CHUNK_SIZE, CHUNK_SIZE, 1],
            [scale * 2.0; 3],
        );

        let air = Block::Air.as_u8();
        let netherrack = Block::Netherrack.as_u8();
        // Columns go z-outer, x-inner, so each column draws the same random
        // values it does in Beta.
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let column = x * CHUNK_SIZE + z;
                let sandy = soul_sand[column] + random.next_double() * 0.2 > 0.0;
                let gravelly = gravel[column] + random.next_double() * 0.2 > 0.0;
                let depth = (depths[column] / 3.0 + 3.0 + random.next_double() * 0.25) as i32;
                let mut remaining = -1;
                let mut top = Block::Netherrack;
                let mut filler = Block::Netherrack;

                for y in (0..CHUNK_HEIGHT as i32).rev() {
                    let index = raw_index(x, y as usize, z);
                    // The floor roll is only drawn when the roof test fails.
                    if y >= 127 - random.next_int(5) as i32 || y <= random.next_int(5) as i32 {
                        blocks[index] = Block::Bedrock.as_u8();
                        continue;
                    }
                    let block = blocks[index];
                    if block == air {
                        remaining = -1;
                    } else if block == netherrack {
                        if remaining == -1 {
                            if depth <= 0 {
                                top = Block::Air;
                                filler = Block::Netherrack;
                            } else if (SHORE_LEVEL - 4..=SHORE_LEVEL + 1).contains(&y) {
                                top = Block::Netherrack;
                                filler = Block::Netherrack;
                                if gravelly {
                                    top = Block::Gravel;
                                }
                                if sandy {
                                    top = Block::SoulSand;
                                    filler = Block::SoulSand;
                                }
                            }
                            if y < SHORE_LEVEL && top == Block::Air {
                                top = Block::Lava;
                            }
                            remaining = depth;
                            blocks[index] = if y >= SHORE_LEVEL - 1 { top } else { filler }.as_u8();
                        } else if remaining > 0 {
                            remaining -= 1;
                            blocks[index] = filler.as_u8();
                        }
                    }
                }
            }
        }
    }
}

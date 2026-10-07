use crate::block::blocks::Block;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;

use super::noise::PerlinOctaves;
use crate::random::JavaRandom;
use crate::world::biome::BiomeMap;

const GRID: usize = 5;
const VERTICAL_GRID: usize = CHUNK_HEIGHT / 8 + 1;
const SEA_LEVEL: usize = 64;
/// Beta's density grid spacing, `684.412` in every noise axis.
const DENSITY_SCALE: f64 = 684.412;

pub(super) use super::super::RawBlocks;
pub(super) use super::super::raw_index;

pub(super) struct TerrainGenerator {
    min_limit: PerlinOctaves,
    max_limit: PerlinOctaves,
    main_limit: PerlinOctaves,
    pub shore: PerlinOctaves,
    pub surface_elevation: PerlinOctaves,
    scale: PerlinOctaves,
    depth: PerlinOctaves,
    /// The reference's `mobSpawnerNoise`, used to vary tree density per chunk.
    pub mob_spawner: PerlinOctaves,
}

impl TerrainGenerator {
    pub fn new(seed: u64) -> Self {
        let mut random = JavaRandom::new(seed);
        // Preserve the reference's octave construction order: it affects every permutation.
        let min_limit = PerlinOctaves::new(&mut random, 16);
        let max_limit = PerlinOctaves::new(&mut random, 16);
        let main_limit = PerlinOctaves::new(&mut random, 8);
        let shore = PerlinOctaves::new(&mut random, 4);
        let surface_elevation = PerlinOctaves::new(&mut random, 4);
        let scale = PerlinOctaves::new(&mut random, 10);
        let depth = PerlinOctaves::new(&mut random, 16);
        // Constructed after `depth` so the earlier octave permutations are unchanged.
        let mob_spawner = PerlinOctaves::new(&mut random, 8);
        Self {
            min_limit,
            max_limit,
            main_limit,
            shore,
            surface_elevation,
            scale,
            depth,
            mob_spawner,
        }
    }

    /// `ChunkProviderGenerate.generateTerrain`: stone below the interpolated
    /// density surface, water below sea level, and ice on cold sea surfaces.
    pub fn generate_base(&self, position: ChunkPosition, biomes: &BiomeMap) -> RawBlocks {
        let density = self.density(position, biomes);
        let index = |x: usize, z: usize, y: usize| (x * GRID + z) * VERTICAL_GRID + y;
        let mut blocks = vec![Block::Air.as_u8(); CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE];
        // Each 4×8×4 cell interpolates by accumulating steps, as Beta does,
        // rather than evaluating a lerp per block.
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
                                if y < SEA_LEVEL {
                                    block = if biomes.get(x, z).temperature < 0.5
                                        && y >= SEA_LEVEL - 1
                                    {
                                        Block::Ice
                                    } else {
                                        Block::Water
                                    };
                                }
                                if value > 0.0 {
                                    block = Block::Stone;
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

    /// `func_4061_a`: the 5×17×5 density grid, indexed `(x * 5 + z) * 17 + y`.
    fn density(&self, position: ChunkPosition, biomes: &BiomeMap) -> Vec<f64> {
        let x0 = position.x * 4;
        let z0 = position.z * 4;
        let origin = [f64::from(x0), 0.0, f64::from(z0)];
        let size = [GRID, VERTICAL_GRID, GRID];
        let scale = self.scale.grid_2d(x0, z0, [GRID, GRID], [1.121, 1.121]);
        let depth = self.depth.grid_2d(x0, z0, [GRID, GRID], [200.0, 200.0]);
        let main = self.main_limit.grid(
            origin,
            size,
            [
                DENSITY_SCALE / 80.0,
                DENSITY_SCALE / 160.0,
                DENSITY_SCALE / 80.0,
            ],
        );
        let min = self.min_limit.grid(origin, size, [DENSITY_SCALE; 3]);
        let max = self.max_limit.grid(origin, size, [DENSITY_SCALE; 3]);

        let mut density = vec![0.0; GRID * VERTICAL_GRID * GRID];
        let mut index = 0;
        let mut column = 0;
        for gx in 0..GRID {
            for gz in 0..GRID {
                let climate = biomes.get(gx * 3 + 1, gz * 3 + 1);
                let mut aridity = 1.0 - climate.humidity * climate.temperature;
                aridity *= aridity;
                aridity *= aridity;
                aridity = 1.0 - aridity;
                let mut surface = (scale[column] + 256.0) / 512.0 * aridity;
                if surface > 1.0 {
                    surface = 1.0;
                }
                let mut height = depth[column] / 8000.0;
                if height < 0.0 {
                    height = -height * 0.3;
                }
                height = height * 3.0 - 2.0;
                if height < 0.0 {
                    height /= 2.0;
                    if height < -1.0 {
                        height = -1.0;
                    }
                    height /= 1.4;
                    height /= 2.0;
                    surface = 0.0;
                } else {
                    if height > 1.0 {
                        height = 1.0;
                    }
                    height /= 8.0;
                }
                if surface < 0.0 {
                    surface = 0.0;
                }
                surface += 0.5;
                height = height * VERTICAL_GRID as f64 / 16.0;
                let center = VERTICAL_GRID as f64 / 2.0 + height * 4.0;
                column += 1;

                for gy in 0..VERTICAL_GRID {
                    let mut falloff = (gy as f64 - center) * 12.0 / surface;
                    if falloff < 0.0 {
                        falloff *= 4.0;
                    }
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
                    value -= falloff;
                    if gy > VERTICAL_GRID - 4 {
                        // Beta divides in float before widening.
                        let top = f64::from((gy - (VERTICAL_GRID - 4)) as f32 / 3.0);
                        value = value * (1.0 - top) + -10.0 * top;
                    }
                    density[index] = value;
                    index += 1;
                }
            }
        }
        density
    }
}

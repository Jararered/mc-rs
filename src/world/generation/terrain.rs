use crate::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, Chunk, ChunkPos},
};

use super::{
    biome::BiomeMap,
    java_random::JavaRandom,
    noise::{PerlinOctaves, lerp},
};

const GRID: usize = 5;
const VERTICAL_GRID: usize = CHUNK_HEIGHT / 8 + 1;
const SEA_LEVEL: usize = 64;

pub(super) struct TerrainGenerator {
    min_limit: PerlinOctaves,
    max_limit: PerlinOctaves,
    main_limit: PerlinOctaves,
    pub shore: PerlinOctaves,
    pub surface_elevation: PerlinOctaves,
    scale: PerlinOctaves,
    depth: PerlinOctaves,
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
        Self {
            min_limit,
            max_limit,
            main_limit,
            shore,
            surface_elevation,
            scale,
            depth,
        }
    }

    pub fn generate_base(&self, position: ChunkPos, biomes: &BiomeMap) -> Chunk {
        let mut density = [0.0; GRID * GRID * VERTICAL_GRID];
        for gx in 0..GRID {
            for gz in 0..GRID {
                let climate = biomes.get(gx * 3 + 1, gz * 3 + 1);
                let world_x = (position.x * 4 + gx as i32) as f64;
                let world_z = (position.z * 4 + gz as i32) as f64;
                let mut aridity = 1.0 - climate.humidity * climate.temperature;
                aridity *= aridity;
                aridity *= aridity;
                aridity = 1.0 - aridity;
                let mut surface = ((self.scale.sample_2d(world_x, world_z, 1.121, 1.121) / 512.0
                    + 0.5)
                    * aridity)
                    .min(1.0);
                let mut depth = self.depth.sample_2d(world_x, world_z, 200.0, 200.0) / 8000.0;
                if depth < 0.0 {
                    depth *= -0.3;
                }
                depth = depth * 3.0 - 2.0;
                if depth < 0.0 {
                    depth = (depth / 2.0).max(-1.0) / 1.4 / 2.0;
                    surface = 0.0;
                } else {
                    depth = depth.min(1.0) / 8.0;
                }
                surface = surface.max(0.0) + 0.5;
                let depth_column = 8.5 + depth * (17.0 / 16.0) * 4.0;

                for gy in 0..VERTICAL_GRID {
                    let mut column = ((gy as f64 - depth_column) * 12.0) / surface;
                    if column < 0.0 {
                        column *= 4.0;
                    }
                    let scale = [684.412, 684.412, 684.412];
                    let min = self.min_limit.sample_3d(world_x, gy as f64, world_z, scale) / 512.0;
                    let max = self.max_limit.sample_3d(world_x, gy as f64, world_z, scale) / 512.0;
                    let blend = (self.main_limit.sample_3d(
                        world_x,
                        gy as f64,
                        world_z,
                        [scale[0] / 80.0, scale[1] / 160.0, scale[2] / 80.0],
                    ) / 10.0
                        + 1.0)
                        / 2.0;
                    let mut value = lerp(blend.clamp(0.0, 1.0), min, max) - column;
                    if gy > 13 {
                        value = lerp(((gy - 13) as f64 / 3.0).min(1.0), value, -10.0);
                    }
                    density[density_index(gx, gz, gy)] = value;
                }
            }
        }

        let mut chunk = Chunk::new();
        // Each 4×4×8 cell is a bounded generation slice of the full chunk.
        for gx in 0..4 {
            for gz in 0..4 {
                for gy in 0..CHUNK_HEIGHT / 8 {
                    let a00 = density[density_index(gx, gz, gy)];
                    let b00 = density[density_index(gx, gz, gy + 1)];
                    let a01 = density[density_index(gx, gz + 1, gy)];
                    let b01 = density[density_index(gx, gz + 1, gy + 1)];
                    let a10 = density[density_index(gx + 1, gz, gy)];
                    let b10 = density[density_index(gx + 1, gz, gy + 1)];
                    let a11 = density[density_index(gx + 1, gz + 1, gy)];
                    let b11 = density[density_index(gx + 1, gz + 1, gy + 1)];
                    for dy in 0..8 {
                        let ty = dy as f64 / 8.0;
                        let n00 = lerp(ty, a00, b00);
                        let n01 = lerp(ty, a01, b01);
                        let n10 = lerp(ty, a10, b10);
                        let n11 = lerp(ty, a11, b11);
                        let y = gy * 8 + dy;
                        for dx in 0..4 {
                            let tx = dx as f64 / 4.0;
                            let z0 = lerp(tx, n00, n10);
                            let z1 = lerp(tx, n01, n11);
                            let z_step = (z1 - z0) / 4.0;
                            let mut limit = z0;
                            for dz in 0..4 {
                                let block = if limit > 0.0 {
                                    BlockId::Stone
                                } else if y < SEA_LEVEL {
                                    BlockId::Water
                                } else {
                                    BlockId::Air
                                };
                                chunk.set(gx * 4 + dx, y, gz * 4 + dz, block);
                                limit += z_step;
                            }
                        }
                    }
                }
            }
        }
        chunk
    }
}

const fn density_index(x: usize, z: usize, y: usize) -> usize {
    (x * GRID + z) * VERTICAL_GRID + y
}

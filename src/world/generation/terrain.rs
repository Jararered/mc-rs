use crate::block::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

use super::biome::BiomeMap;
use super::biome::Climate;
use super::noise::PerlinOctaves;
use super::noise::lerp;
use crate::random::JavaRandom;

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

    pub fn generate_base(&self, position: ChunkPos, biomes: &BiomeMap) -> Chunk {
        let mut density = [0.0; GRID * GRID * VERTICAL_GRID];
        for gx in 0..GRID {
            for gz in 0..GRID {
                let climate = biomes.get(gx * 3 + 1, gz * 3 + 1);
                let world_x = (position.x * 4 + gx as i32) as f64;
                let world_z = (position.z * 4 + gz as i32) as f64;
                let column = self.density_column(world_x, world_z, climate);
                for gy in 0..VERTICAL_GRID {
                    density[density_index(gx, gz, gy)] = column[gy];
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

    /// Density values at the 17 vertical grid points for one 4×4 cell column.
    ///
    /// `world_x`/`world_z` are the noise-space coordinates the reference samples
    /// at (chunk coordinate times four plus the grid index), not block coordinates.
    fn density_column(&self, world_x: f64, world_z: f64, climate: Climate) -> [f64; VERTICAL_GRID] {
        let mut aridity = 1.0 - climate.humidity * climate.temperature;
        aridity *= aridity;
        aridity *= aridity;
        aridity = 1.0 - aridity;
        let mut surface = ((self.scale.sample_2d(world_x, world_z, 1.121, 1.121) / 512.0 + 0.5)
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

        let mut values = [0.0; VERTICAL_GRID];
        for (gy, slot) in values.iter_mut().enumerate() {
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
            *slot = value;
        }
        values
    }

    /// Topmost solid block (density above zero) in a world column.
    ///
    /// This mirrors the density interpolation in [`Self::generate_base`] for a
    /// single column, so decoration can find the ground height outside the chunk
    /// it is generating without generating a whole neighbouring chunk.
    pub fn column_top(
        &self,
        world_x: i32,
        world_z: i32,
        climate_at: impl Fn(f64, f64) -> Climate,
    ) -> usize {
        let chunk_x = world_x.div_euclid(CHUNK_SIZE as i32);
        let chunk_z = world_z.div_euclid(CHUNK_SIZE as i32);
        let local_x = world_x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = world_z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let gx = local_x / 4;
        let gz = local_z / 4;

        // Corner grid columns of the 4×4 cell the column falls in, in the same
        // order `generate_base` reads them: (gx,gz), (gx+1,gz), (gx,gz+1), (gx+1,gz+1).
        let corners = [(gx, gz), (gx + 1, gz), (gx, gz + 1), (gx + 1, gz + 1)];
        let mut columns = [[0.0; VERTICAL_GRID]; 4];
        for (column, (corner_x, corner_z)) in columns.iter_mut().zip(corners) {
            let noise_x = (chunk_x * 4 + corner_x as i32) as f64;
            let noise_z = (chunk_z * 4 + corner_z as i32) as f64;
            let climate = climate_at(
                (chunk_x * CHUNK_SIZE as i32 + corner_x as i32 * 3 + 1) as f64,
                (chunk_z * CHUNK_SIZE as i32 + corner_z as i32 * 3 + 1) as f64,
            );
            *column = self.density_column(noise_x, noise_z, climate);
        }

        let tx = (local_x % 4) as f64 / 4.0;
        let tz = (local_z % 4) as f64 / 4.0;
        for y in (0..CHUNK_HEIGHT).rev() {
            let gy = y / 8;
            let ty = (y % 8) as f64 / 8.0;
            let n00 = lerp(ty, columns[0][gy], columns[0][gy + 1]);
            let n10 = lerp(ty, columns[1][gy], columns[1][gy + 1]);
            let n01 = lerp(ty, columns[2][gy], columns[2][gy + 1]);
            let n11 = lerp(ty, columns[3][gy], columns[3][gy + 1]);
            let z0 = lerp(tx, n00, n10);
            let z1 = lerp(tx, n01, n11);
            if lerp(tz, z0, z1) > 0.0 {
                return y;
            }
        }
        0
    }
}

const fn density_index(x: usize, z: usize, y: usize) -> usize {
    (x * GRID + z) * VERTICAL_GRID + y
}

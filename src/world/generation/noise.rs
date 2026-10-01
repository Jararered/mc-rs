//! Beta `NoiseGeneratorPerlin`, `NoiseGeneratorOctaves`, and the simplex
//! `NoiseGenerator2` used for climate.
//!
//! Terrain and surface noise are generated the way Beta generates them: as
//! grids, through [`PerlinNoise::add_grid`]. Its 3D path caches the gradient
//! lerps of each y cell using the y fraction of the first sample in that cell,
//! and reuses them for later samples in the same cell. The low-frequency
//! octaves that dominate terrain density are exactly the ones this affects,
//! so evaluating textbook Perlin per sample does not reproduce Beta terrain.

use crate::random::JavaRandom;

/// Improved Perlin noise with Java-seeded permutation tables.
pub struct PerlinNoise {
    permutation: [u8; 512],
    x_offset: f64,
    y_offset: f64,
    z_offset: f64,
}

/// `(int)v`, then one lower when truncation rounded up, as Beta floors.
fn floor(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated - 1
    } else {
        truncated
    }
}

impl PerlinNoise {
    pub fn new(random: &mut JavaRandom) -> Self {
        let x_offset = random.next_double() * 256.0;
        let y_offset = random.next_double() * 256.0;
        let z_offset = random.next_double() * 256.0;
        let mut permutation = [0; 512];
        for (i, value) in permutation[..256].iter_mut().enumerate() {
            *value = i as u8;
        }
        for i in 0..256 {
            let j = i + random.next_int((256 - i) as u32) as usize;
            permutation.swap(i, j);
        }
        for i in 0..256 {
            permutation[i + 256] = permutation[i];
        }
        Self {
            permutation,
            x_offset,
            y_offset,
            z_offset,
        }
    }

    fn p(&self, index: usize) -> usize {
        usize::from(self.permutation[index])
    }

    /// `generateNoise`: one 3D Perlin sample.
    pub fn noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut x = x + self.x_offset;
        let mut y = y + self.y_offset;
        let mut z = z + self.z_offset;
        let (xi, yi, zi) = (floor(x), floor(y), floor(z));
        let (a, b, c) = (
            (xi & 255) as usize,
            (yi & 255) as usize,
            (zi & 255) as usize,
        );
        x -= f64::from(xi);
        y -= f64::from(yi);
        z -= f64::from(zi);
        let (u, v, w) = (fade(x), fade(y), fade(z));
        let aa = self.p(a) + b;
        let aaa = self.p(aa) + c;
        let aab = self.p(aa + 1) + c;
        let ab = self.p(a + 1) + b;
        let aba = self.p(ab) + c;
        let abb = self.p(ab + 1) + c;
        lerp(
            w,
            lerp(
                v,
                lerp(
                    u,
                    grad(self.p(aaa), x, y, z),
                    grad(self.p(aba), x - 1.0, y, z),
                ),
                lerp(
                    u,
                    grad(self.p(aab), x, y - 1.0, z),
                    grad(self.p(abb), x - 1.0, y - 1.0, z),
                ),
            ),
            lerp(
                v,
                lerp(
                    u,
                    grad(self.p(aaa + 1), x, y, z - 1.0),
                    grad(self.p(aba + 1), x - 1.0, y, z - 1.0),
                ),
                lerp(
                    u,
                    grad(self.p(aab + 1), x, y - 1.0, z - 1.0),
                    grad(self.p(abb + 1), x - 1.0, y - 1.0, z - 1.0),
                ),
            ),
        )
    }

    /// `func_805_a`: add one octave over a grid to `out`, indexed
    /// `(x * size_z + z) * size_y + y`. A single-row y dimension takes Beta's
    /// 2D path, which ignores `y` entirely.
    pub fn add_grid(
        &self,
        out: &mut [f64],
        origin: [f64; 3],
        size: [usize; 3],
        scale: [f64; 3],
        amplitude: f64,
    ) {
        let [x0, y0, z0] = origin;
        let [size_x, size_y, size_z] = size;
        let [scale_x, scale_y, scale_z] = scale;
        let weight = 1.0 / amplitude;
        let mut index = 0;
        if size_y == 1 {
            for ix in 0..size_x {
                let mut x = (x0 + ix as f64) * scale_x + self.x_offset;
                let xi = floor(x);
                let a = (xi & 255) as usize;
                x -= f64::from(xi);
                let u = fade(x);
                for iz in 0..size_z {
                    let mut z = (z0 + iz as f64) * scale_z + self.z_offset;
                    let zi = floor(z);
                    let c = (zi & 255) as usize;
                    z -= f64::from(zi);
                    let w = fade(z);
                    let aa = self.p(self.p(a)) + c;
                    let ba = self.p(self.p(a + 1)) + c;
                    let near = lerp(
                        u,
                        grad(self.p(aa), x, 0.0, z),
                        grad(self.p(ba), x - 1.0, 0.0, z),
                    );
                    let far = lerp(
                        u,
                        grad(self.p(aa + 1), x, 0.0, z - 1.0),
                        grad(self.p(ba + 1), x - 1.0, 0.0, z - 1.0),
                    );
                    out[index] += lerp(w, near, far) * weight;
                    index += 1;
                }
            }
            return;
        }

        let mut cached_cell = -1;
        let (mut l0, mut l1, mut l2, mut l3) = (0.0, 0.0, 0.0, 0.0);
        for ix in 0..size_x {
            let mut x = (x0 + ix as f64) * scale_x + self.x_offset;
            let xi = floor(x);
            let a = (xi & 255) as usize;
            x -= f64::from(xi);
            let u = fade(x);
            for iz in 0..size_z {
                let mut z = (z0 + iz as f64) * scale_z + self.z_offset;
                let zi = floor(z);
                let c = (zi & 255) as usize;
                z -= f64::from(zi);
                let w = fade(z);
                for iy in 0..size_y {
                    let mut y = (y0 + iy as f64) * scale_y + self.y_offset;
                    let yi = floor(y);
                    let cell = yi & 255;
                    y -= f64::from(yi);
                    let v = fade(y);
                    // Beta recomputes these only when the y cell changes, with
                    // the fraction of the sample that entered the cell.
                    if iy == 0 || cell != cached_cell {
                        cached_cell = cell;
                        let b = cell as usize;
                        let aa = self.p(a) + b;
                        let aaa = self.p(aa) + c;
                        let aab = self.p(aa + 1) + c;
                        let ab = self.p(a + 1) + b;
                        let aba = self.p(ab) + c;
                        let abb = self.p(ab + 1) + c;
                        l0 = lerp(
                            u,
                            grad(self.p(aaa), x, y, z),
                            grad(self.p(aba), x - 1.0, y, z),
                        );
                        l1 = lerp(
                            u,
                            grad(self.p(aab), x, y - 1.0, z),
                            grad(self.p(abb), x - 1.0, y - 1.0, z),
                        );
                        l2 = lerp(
                            u,
                            grad(self.p(aaa + 1), x, y, z - 1.0),
                            grad(self.p(aba + 1), x - 1.0, y, z - 1.0),
                        );
                        l3 = lerp(
                            u,
                            grad(self.p(aab + 1), x, y - 1.0, z - 1.0),
                            grad(self.p(abb + 1), x - 1.0, y - 1.0, z - 1.0),
                        );
                    }
                    let near = lerp(v, l0, l1);
                    let far = lerp(v, l2, l3);
                    out[index] += lerp(w, near, far) * weight;
                    index += 1;
                }
            }
        }
    }

    pub fn sample_simplex_2d(&self, x: f64, z: f64) -> f64 {
        const F2: f64 = 0.3660254037844386;
        const G2: f64 = 0.21132486540518713;
        const GRADIENTS: [[f64; 2]; 12] = [
            [1.0, 1.0],
            [-1.0, 1.0],
            [1.0, -1.0],
            [-1.0, -1.0],
            [1.0, 0.0],
            [-1.0, 0.0],
            [1.0, 0.0],
            [-1.0, 0.0],
            [0.0, 1.0],
            [0.0, -1.0],
            [0.0, 1.0],
            [0.0, -1.0],
        ];
        let x = x + self.x_offset;
        let z = z + self.y_offset;
        let skew = (x + z) * F2;
        let cell_x = (x + skew).floor() as i64;
        let cell_z = (z + skew).floor() as i64;
        let unskew = (cell_x + cell_z) as f64 * G2;
        let x0 = x - (cell_x as f64 - unskew);
        let z0 = z - (cell_z as f64 - unskew);
        let (ix, iz) = if x0 > z0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - ix as f64 + G2;
        let z1 = z0 - iz as f64 + G2;
        let x2 = x0 - 1.0 + 2.0 * G2;
        let z2 = z0 - 1.0 + 2.0 * G2;
        let cx = cell_x as usize & 255;
        let cz = cell_z as usize & 255;
        let p = &self.permutation;
        let h0 = p[cx + p[cz] as usize] as usize % 12;
        let h1 = p[cx + ix + p[cz + iz] as usize] as usize % 12;
        let h2 = p[cx + 1 + p[cz + 1] as usize] as usize % 12;
        let contribution = |dx: f64, dz: f64, h: usize| {
            let t = 0.5 - dx * dx - dz * dz;
            if t <= 0.0 {
                0.0
            } else {
                let t = t * t;
                t * t * (GRADIENTS[h][0] * dx + GRADIENTS[h][1] * dz)
            }
        };
        70.0 * (contribution(x0, z0, h0) + contribution(x1, z1, h1) + contribution(x2, z2, h2))
    }
}

/// `NoiseGeneratorOctaves`: octave `i` has frequency `2^-i` and weight `2^i`.
pub struct PerlinOctaves(pub Vec<PerlinNoise>);

impl PerlinOctaves {
    pub fn new(random: &mut JavaRandom, count: usize) -> Self {
        Self((0..count).map(|_| PerlinNoise::new(random)).collect())
    }

    /// `generateNoiseOctaves`, indexed like [`PerlinNoise::add_grid`].
    pub fn grid(&self, origin: [f64; 3], size: [usize; 3], scale: [f64; 3]) -> Vec<f64> {
        let mut out = vec![0.0; size[0] * size[1] * size[2]];
        let mut amplitude = 1.0;
        for octave in &self.0 {
            octave.add_grid(
                &mut out,
                origin,
                size,
                scale.map(|axis| axis * amplitude),
                amplitude,
            );
            amplitude /= 2.0;
        }
        out
    }

    /// `func_4109_a`: a 2D grid through [`Self::grid`]'s single-row path.
    pub fn grid_2d(&self, x: i32, z: i32, size: [usize; 2], scale: [f64; 2]) -> Vec<f64> {
        self.grid(
            [f64::from(x), 10.0, f64::from(z)],
            [size[0], 1, size[1]],
            [scale[0], 1.0, scale[1]],
        )
    }

    /// `func_806_a`: a single sample of 3D noise at `(x, z, 0)` per octave,
    /// used for the population tree count.
    pub fn point(&self, x: f64, z: f64) -> f64 {
        let mut amplitude = 1.0;
        let mut value = 0.0;
        for octave in &self.0 {
            value += octave.noise(x * amplitude, z * amplitude, 0.0) / amplitude;
            amplitude /= 2.0;
        }
        value
    }
}

pub struct SimplexOctaves(Vec<PerlinNoise>);

impl SimplexOctaves {
    pub fn new(seed: u64, count: usize) -> Self {
        let mut random = JavaRandom::new(seed);
        Self((0..count).map(|_| PerlinNoise::new(&mut random)).collect())
    }

    pub fn sample(&self, x: f64, z: f64, scale: f64, frequency_step: f64) -> f64 {
        let mut frequency = 1.0;
        let mut amplitude = 0.55;
        let mut value = 0.0;
        for octave in &self.0 {
            value +=
                octave.sample_simplex_2d(x * scale * frequency, z * scale * frequency) * amplitude;
            frequency *= frequency_step;
            amplitude *= 2.0;
        }
        value
    }
}

pub fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn grad(hash: usize, x: f64, y: f64, z: f64) -> f64 {
    match hash & 15 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        11 => -y - z,
        12 => y + x,
        13 => -y + z,
        14 => y - x,
        _ => -y - z,
    }
}

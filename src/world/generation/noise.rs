use crate::random::JavaRandom;

/// Improved Perlin noise with Java-seeded permutation tables.
pub(super) struct PerlinNoise {
    permutation: [u8; 512],
    x_offset: f64,
    y_offset: f64,
    z_offset: f64,
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

    pub fn sample_2d(&self, x: f64, z: f64) -> f64 {
        let x = x + self.x_offset;
        let z = z + self.z_offset;
        let xi = x.floor() as i64 as usize & 255;
        let zi = z.floor() as i64 as usize & 255;
        let xf = x - x.floor();
        let zf = z - z.floor();
        let u = fade(xf);
        let v = fade(zf);
        let p = &self.permutation;
        let a = p[p[p[xi] as usize] as usize + zi] as usize;
        let b = p[p[p[xi + 1] as usize] as usize + zi] as usize;
        let c = p[p[p[xi] as usize] as usize + zi + 1] as usize;
        let d = p[p[p[xi + 1] as usize] as usize + zi + 1] as usize;
        lerp(
            v,
            lerp(u, grad(a, xf, 0.0, zf), grad(b, xf - 1.0, 0.0, zf)),
            lerp(
                u,
                grad(c, xf, 0.0, zf - 1.0),
                grad(d, xf - 1.0, 0.0, zf - 1.0),
            ),
        )
    }

    pub fn sample_3d(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.x_offset;
        let y = y + self.y_offset;
        let z = z + self.z_offset;
        let xi = x.floor() as i64 as usize & 255;
        let yi = y.floor() as i64 as usize & 255;
        let zi = z.floor() as i64 as usize & 255;
        let xf = x - x.floor();
        let yf = y - y.floor();
        let zf = z - z.floor();
        let u = fade(xf);
        let v = fade(yf);
        let w = fade(zf);
        let p = &self.permutation;
        let a = p[xi] as usize + yi;
        let b = p[xi + 1] as usize + yi;
        let aa = p[a] as usize + zi;
        let ab = p[a + 1] as usize + zi;
        let ba = p[b] as usize + zi;
        let bb = p[b + 1] as usize + zi;
        lerp(
            w,
            lerp(
                v,
                lerp(
                    u,
                    grad(p[aa] as usize, xf, yf, zf),
                    grad(p[ba] as usize, xf - 1.0, yf, zf),
                ),
                lerp(
                    u,
                    grad(p[ab] as usize, xf, yf - 1.0, zf),
                    grad(p[bb] as usize, xf - 1.0, yf - 1.0, zf),
                ),
            ),
            lerp(
                v,
                lerp(
                    u,
                    grad(p[aa + 1] as usize, xf, yf, zf - 1.0),
                    grad(p[ba + 1] as usize, xf - 1.0, yf, zf - 1.0),
                ),
                lerp(
                    u,
                    grad(p[ab + 1] as usize, xf, yf - 1.0, zf - 1.0),
                    grad(p[bb + 1] as usize, xf - 1.0, yf - 1.0, zf - 1.0),
                ),
            ),
        )
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

pub(super) struct PerlinOctaves(pub Vec<PerlinNoise>);

impl PerlinOctaves {
    pub fn new(random: &mut JavaRandom, count: usize) -> Self {
        Self((0..count).map(|_| PerlinNoise::new(random)).collect())
    }

    pub fn sample_2d(&self, x: f64, z: f64, x_scale: f64, z_scale: f64) -> f64 {
        let mut octave_size = 1.0;
        let mut value = 0.0;
        for octave in &self.0 {
            value += octave.sample_2d(x * x_scale * octave_size, z * z_scale * octave_size)
                / octave_size;
            octave_size *= 0.5;
        }
        value
    }

    pub fn sample_3d(&self, x: f64, y: f64, z: f64, scale: [f64; 3]) -> f64 {
        let mut octave_size = 1.0;
        let mut value = 0.0;
        for octave in &self.0 {
            value += octave.sample_3d(
                x * scale[0] * octave_size,
                y * scale[1] * octave_size,
                z * scale[2] * octave_size,
            ) / octave_size;
            octave_size *= 0.5;
        }
        value
    }
}

pub(super) struct SimplexOctaves(Vec<PerlinNoise>);

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

pub(super) fn lerp(t: f64, a: f64, b: f64) -> f64 {
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

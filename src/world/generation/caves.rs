//! Beta 1.7.3 `MapGenBase` and `MapGenCaves`.
use crate::block::id::BlockId;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;

const PI: f32 = 3.1415927;

pub(super) fn carve(chunk: &mut Chunk, target: ChunkPos, seed: u64) {
    let mut random = JavaRandom::new(seed);
    let salt_x = random.next_long() / 2 * 2 + 1;
    let salt_z = random.next_long() / 2 * 2 + 1;
    for sx in target.x - 8..=target.x + 8 {
        for sz in target.z - 8..=target.z + 8 {
            let source_seed = (sx as i64)
                .wrapping_mul(salt_x)
                .wrapping_add((sz as i64).wrapping_mul(salt_z))
                ^ seed as i64;
            let mut random = JavaRandom::new(source_seed as u64);
            let bound = random.next_int(40) + 1;
            let bound = random.next_int(bound) + 1;
            let count = random.next_int(bound);
            let count = if random.next_int(15) == 0 { count } else { 0 };
            for _ in 0..count {
                let x = (sx * 16 + random.next_int(16) as i32) as f64;
                let y_bound = random.next_int(120) + 8;
                let y = random.next_int(y_bound) as f64;
                let z = (sz * 16 + random.next_int(16) as i32) as f64;
                let mut tunnels = 1;
                if random.next_int(4) == 0 {
                    let radius = 1.0 + random.next_float() * 6.0;
                    node(
                        chunk,
                        target,
                        &mut random,
                        [x, y, z],
                        radius,
                        0.0,
                        0.0,
                        -1,
                        0,
                        0.5,
                    );
                    tunnels += random.next_int(4);
                }
                for _ in 0..tunnels {
                    let yaw = random.next_float() * PI * 2.0;
                    let pitch = (random.next_float() - 0.5) * 2.0 / 8.0;
                    let radius = random.next_float() * 2.0 + random.next_float();
                    node(
                        chunk,
                        target,
                        &mut random,
                        [x, y, z],
                        radius,
                        yaw,
                        pitch,
                        0,
                        0,
                        1.0,
                    );
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn node(
    chunk: &mut Chunk,
    target: ChunkPos,
    parent: &mut JavaRandom,
    mut p: [f64; 3],
    radius: f32,
    mut yaw: f32,
    mut pitch: f32,
    mut step: i32,
    mut length: i32,
    vertical: f64,
) {
    let center_x = (target.x * 16 + 8) as f64;
    let center_z = (target.z * 16 + 8) as f64;
    let mut yaw_velocity = 0.0f32;
    let mut pitch_velocity = 0.0f32;
    let mut random = JavaRandom::new(parent.next_long() as u64);
    if length <= 0 {
        length = 112 - random.next_int(28) as i32;
    }
    let large = step == -1;
    if large {
        step = length / 2;
    }
    let branch_step = random.next_int((length / 2) as u32) as i32 + length / 4;
    let gentle = random.next_int(6) == 0;
    while step < length {
        let width = 1.5 + (step as f32 * PI / length as f32).sin() as f64 * radius as f64;
        let height = width * vertical;
        let cos_pitch = pitch.cos();
        p[0] += (yaw.cos() * cos_pitch) as f64;
        p[1] += pitch.sin() as f64;
        p[2] += (yaw.sin() * cos_pitch) as f64;
        pitch *= if gentle { 0.92 } else { 0.7 };
        pitch += pitch_velocity * 0.1;
        yaw += yaw_velocity * 0.1;
        pitch_velocity *= 0.9;
        yaw_velocity *= 0.75;
        pitch_velocity += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
        yaw_velocity += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
        if !large && step == branch_step && radius > 1.0 {
            let left_radius = random.next_float() * 0.5 + 0.5;
            node(
                chunk,
                target,
                &mut random,
                p,
                left_radius,
                yaw - PI / 2.0,
                pitch / 3.0,
                step,
                length,
                1.0,
            );
            let right_radius = random.next_float() * 0.5 + 0.5;
            node(
                chunk,
                target,
                &mut random,
                p,
                right_radius,
                yaw + PI / 2.0,
                pitch / 3.0,
                step,
                length,
                1.0,
            );
            return;
        }
        if large || random.next_int(4) != 0 {
            let dx = p[0] - center_x;
            let dz = p[2] - center_z;
            let remaining = (length - step) as f64;
            let reach = (radius + 18.0) as f64;
            if dx * dx + dz * dz - remaining * remaining > reach * reach {
                return;
            }
            if p[0] >= center_x - 16.0 - width * 2.0
                && p[0] <= center_x + 16.0 + width * 2.0
                && p[2] >= center_z - 16.0 - width * 2.0
                && p[2] <= center_z + 16.0 + width * 2.0
            {
                let x0 = ((p[0] - width).floor() as i32 - target.x * 16 - 1).max(0);
                let x1 = ((p[0] + width).floor() as i32 - target.x * 16 + 1).min(16);
                let y0 = ((p[1] - height).floor() as i32 - 1).max(1);
                let y1 = ((p[1] + height).floor() as i32 + 1).min(120);
                let z0 = ((p[2] - width).floor() as i32 - target.z * 16 - 1).max(0);
                let z1 = ((p[2] + width).floor() as i32 - target.z * 16 + 1).min(16);
                let mut water = false;
                'scan: for x in x0..x1 {
                    for z in z0..z1 {
                        for y in (y0 - 1)..=y1 + 1 {
                            if (0..CHUNK_HEIGHT as i32).contains(&y)
                                && matches!(
                                    chunk.get(x as usize, y as usize, z as usize),
                                    Some(BlockId::Water | BlockId::FlowingWater)
                                )
                            {
                                water = true;
                                break 'scan;
                            }
                        }
                    }
                }
                if !water {
                    for x in x0..x1 {
                        for z in z0..z1 {
                            let nx = (x + target.x * 16) as f64 + 0.5 - p[0];
                            let nz = (z + target.z * 16) as f64 + 0.5 - p[2];
                            let horizontal = (nx * nx + nz * nz) / (width * width);
                            if horizontal >= 1.0 {
                                continue;
                            }
                            let mut grass = false;
                            for y in (y0..y1).rev() {
                                let ny = (y as f64 + 0.5 - p[1]) / height;
                                if ny <= -0.7 || horizontal + ny * ny >= 1.0 {
                                    continue;
                                }
                                let old = chunk.get(x as usize, y as usize, z as usize).unwrap();
                                if old == BlockId::Grass {
                                    grass = true;
                                }
                                if matches!(old, BlockId::Stone | BlockId::Dirt | BlockId::Grass) {
                                    chunk.set(
                                        x as usize,
                                        y as usize,
                                        z as usize,
                                        if y < 10 {
                                            BlockId::FlowingLava
                                        } else {
                                            BlockId::Air
                                        },
                                    );
                                    if y >= 10
                                        && grass
                                        && y > 0
                                        && chunk.get(x as usize, (y - 1) as usize, z as usize)
                                            == Some(BlockId::Dirt)
                                    {
                                        chunk.set(
                                            x as usize,
                                            (y - 1) as usize,
                                            z as usize,
                                            BlockId::Grass,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if large {
                        break;
                    }
                }
            }
        }
        step += 1;
    }
}

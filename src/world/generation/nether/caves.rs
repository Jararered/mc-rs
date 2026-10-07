//! Beta 1.7.3 `MapGenBase` and `MapGenCavesHell`.
//!
//! The Nether's tunnels are wider and flatter than Overworld caves, start at
//! any height, stop at lava instead of water, and leave no lava behind.
use crate::block::blocks::Block;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;

use super::super::RawBlocks;
use super::super::raw_index;
use super::math;
use super::math::PI;

/// `MapGenBase.range`.
const RANGE: i32 = 8;

pub(super) fn carve(blocks: &mut RawBlocks, target: ChunkPosition, seed: u64) {
    let mut random = JavaRandom::new(seed);
    let salt_x = random.next_long() / 2 * 2 + 1;
    let salt_z = random.next_long() / 2 * 2 + 1;
    for sx in target.x - RANGE..=target.x + RANGE {
        for sz in target.z - RANGE..=target.z + RANGE {
            let source_seed = i64::from(sx)
                .wrapping_mul(salt_x)
                .wrapping_add(i64::from(sz).wrapping_mul(salt_z))
                ^ seed as i64;
            let mut random = JavaRandom::new(source_seed as u64);
            let bound = random.next_int(10) + 1;
            let bound = random.next_int(bound) + 1;
            let count = random.next_int(bound);
            let count = if random.next_int(5) == 0 { count } else { 0 };
            for _ in 0..count {
                let x = f64::from(sx * 16 + random.next_int(16) as i32);
                let y = f64::from(random.next_int(128));
                let z = f64::from(sz * 16 + random.next_int(16) as i32);
                let mut tunnels = 1;
                if random.next_int(4) == 0 {
                    let radius = 1.0 + random.next_float() * 6.0;
                    node(
                        blocks,
                        target,
                        &mut random,
                        [x, y, z],
                        radius,
                        0.0,
                        0.0,
                        -1,
                        -1,
                        0.5,
                    );
                    tunnels += random.next_int(4);
                }
                for _ in 0..tunnels {
                    let yaw = random.next_float() * PI * 2.0;
                    let pitch = (random.next_float() - 0.5) * 2.0 / 8.0;
                    let radius = random.next_float() * 2.0 + random.next_float();
                    node(
                        blocks,
                        target,
                        &mut random,
                        [x, y, z],
                        radius * 2.0,
                        yaw,
                        pitch,
                        0,
                        0,
                        0.5,
                    );
                }
            }
        }
    }
}

/// `generateCaveNode`. `source` is `MapGenBase.rand`; the node draws its own
/// random from it, and so does each branch it forks into.
#[allow(clippy::too_many_arguments)]
fn node(
    blocks: &mut RawBlocks,
    target: ChunkPosition,
    source: &mut JavaRandom,
    mut p: [f64; 3],
    radius: f32,
    mut yaw: f32,
    mut pitch: f32,
    mut step: i32,
    mut length: i32,
    vertical: f64,
) {
    let center_x = f64::from(target.x * 16 + 8);
    let center_z = f64::from(target.z * 16 + 8);
    let mut yaw_velocity = 0.0f32;
    let mut pitch_velocity = 0.0f32;
    let mut random = JavaRandom::new(source.next_long() as u64);
    if length <= 0 {
        let span = RANGE * 16 - 16;
        length = span - random.next_int((span / 4) as u32) as i32;
    }
    let large = step == -1;
    if large {
        step = length / 2;
    }
    let branch_step = random.next_int((length / 2) as u32) as i32 + length / 4;
    let gentle = random.next_int(6) == 0;
    while step < length {
        let width = 1.5 + f64::from(math::sin(step as f32 * PI / length as f32) * radius * 1.0);
        let height = width * vertical;
        let cos_pitch = math::cos(pitch);
        let sin_pitch = math::sin(pitch);
        p[0] += f64::from(math::cos(yaw) * cos_pitch);
        p[1] += f64::from(sin_pitch);
        p[2] += f64::from(math::sin(yaw) * cos_pitch);
        pitch *= if gentle { 0.92 } else { 0.7 };
        pitch += pitch_velocity * 0.1;
        yaw += yaw_velocity * 0.1;
        pitch_velocity *= 0.9;
        yaw_velocity *= 0.75;
        pitch_velocity += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
        yaw_velocity += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
        if !large && step == branch_step && radius > 1.0 {
            // Branches are round; the tunnels they leave are flattened.
            let left_radius = random.next_float() * 0.5 + 0.5;
            node(
                blocks,
                target,
                source,
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
                blocks,
                target,
                source,
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
            let remaining = f64::from(length - step);
            let reach = f64::from(radius + 2.0 + 16.0);
            if dx * dx + dz * dz - remaining * remaining > reach * reach {
                return;
            }
            if p[0] >= center_x - 16.0 - width * 2.0
                && p[2] >= center_z - 16.0 - width * 2.0
                && p[0] <= center_x + 16.0 + width * 2.0
                && p[2] <= center_z + 16.0 + width * 2.0
            {
                // A large cave stops after its first carved step; lava near
                // the step skips the carve and keeps the cave walking.
                if carve_ellipsoid(blocks, target, p, width, height) && large {
                    break;
                }
            }
        }
        step += 1;
    }
}

/// The body of one cave step. Returns false without carving when lava is near.
fn carve_ellipsoid(
    blocks: &mut RawBlocks,
    target: ChunkPosition,
    p: [f64; 3],
    width: f64,
    height: f64,
) -> bool {
    let x0 = (math::floor_double(p[0] - width) - target.x * 16 - 1).max(0);
    let x1 = (math::floor_double(p[0] + width) - target.x * 16 + 1).min(16);
    let y0 = (math::floor_double(p[1] - height) - 1).max(1);
    let y1 = (math::floor_double(p[1] + height) + 1).min(120);
    let z0 = (math::floor_double(p[2] - width) - target.z * 16 - 1).max(0);
    let z1 = (math::floor_double(p[2] + width) - target.z * 16 + 1).min(16);

    // Beta scans only the shell of the box: interior columns check their top
    // cell and then skip straight to the bottom one.
    let lava = [Block::Lava.as_u8(), Block::FlowingLava.as_u8()];
    for x in x0..x1 {
        for z in z0..z1 {
            let mut y = y1 + 1;
            while y >= y0 - 1 {
                if (0..CHUNK_HEIGHT as i32).contains(&y) {
                    if lava.contains(&blocks[raw_index(x as usize, y as usize, z as usize)]) {
                        return false;
                    }
                    if y != y0 - 1 && x != x0 && x != x1 - 1 && z != z0 && z != z1 - 1 {
                        y = y0;
                    }
                }
                y -= 1;
            }
        }
    }

    let carved = [
        Block::Netherrack.as_u8(),
        Block::Dirt.as_u8(),
        Block::Grass.as_u8(),
    ];
    for x in x0..x1 {
        let nx = (f64::from(x + target.x * 16) + 0.5 - p[0]) / width;
        for z in z0..z1 {
            let nz = (f64::from(z + target.z * 16) + 0.5 - p[2]) / width;
            // Beta tests the ellipsoid at `y` but writes the cell above it:
            // its index starts at `y1` while the test starts at `y1 - 1`.
            for y in (y0..y1).rev() {
                let ny = (f64::from(y) + 0.5 - p[1]) / height;
                if ny > -0.7 && nx * nx + ny * ny + nz * nz < 1.0 {
                    let index = raw_index(x as usize, (y + 1) as usize, z as usize);
                    if carved.contains(&blocks[index]) {
                        blocks[index] = Block::Air.as_u8();
                    }
                }
            }
        }
    }
    true
}

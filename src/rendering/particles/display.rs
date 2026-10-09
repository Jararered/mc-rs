//! `World.randomDisplayUpdates`: each client tick Beta samples 1000 cells
//! within 16 blocks of the player and asks each block for its
//! `randomDisplayTick`, which is where torches, furnaces, fire, lava,
//! redstone and portals make their smoke, flames and sparks. The sounds Beta
//! plays alongside are left for audio.

use bevy::prelude::*;

use super::effects::EffectParticles;
use super::effects::FxKind;
use crate::block::blocks::Block;
use crate::block::properties::can_catch_fire;
use crate::random::JavaRandom;
use crate::world::chunk::WorldChunks;

const RADIUS: u32 = 16;
const SAMPLES: usize = 1000;

/// Beta's `BlockRedstoneRepeater.repeaterTorchOffset`.
const REPEATER_TORCH_OFFSET: [f32; 4] = [-0.0625, 0.0625, 0.1875, 0.3125];

/// One tick of `randomDisplayUpdates` around the block cell `center`.
pub fn random_display_updates(
    particles: &mut EffectParticles,
    chunks: &WorldChunks,
    center: IVec3,
) {
    let mut random = std::mem::replace(&mut particles.display_random, JavaRandom::new(0));
    for _ in 0..SAMPLES {
        let x = center.x + random.next_int(RADIUS) as i32 - random.next_int(RADIUS) as i32;
        let y = center.y + random.next_int(RADIUS) as i32 - random.next_int(RADIUS) as i32;
        let z = center.z + random.next_int(RADIUS) as i32 - random.next_int(RADIUS) as i32;
        let Some(block) = chunks.block_at(x, y, z) else {
            continue;
        };
        if block == Block::Air {
            continue;
        }
        display_tick(particles, &mut random, chunks, IVec3::new(x, y, z), block);
    }
    particles.display_random = random;
}

fn display_tick(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    chunks: &WorldChunks,
    cell: IVec3,
    block: Block,
) {
    let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
    let base = cell.as_vec3();
    match block {
        Block::Torch => torch(particles, base, metadata),
        Block::RedstoneTorch => redstone_torch(particles, random, base, metadata),
        Block::LitFurnace => furnace(particles, random, base, metadata),
        Block::Fire => fire(particles, random, chunks, cell),
        Block::LitRedstoneOre => sparkle(particles, random, chunks, cell),
        Block::RedstoneWire => redstone_wire(particles, random, base, metadata),
        Block::PoweredRepeater => repeater(particles, random, base, metadata),
        Block::NetherPortal => portal(particles, random, chunks, cell),
        Block::Lava | Block::FlowingLava => lava(particles, random, chunks, cell),
        _ => {}
    }
}

/// `BlockTorch.randomDisplayTick`.
fn torch(particles: &mut EffectParticles, base: Vec3, metadata: u8) {
    let x = base.x + 0.5;
    let y = base.y + 0.7;
    let z = base.z + 0.5;
    let rise = 0.22;
    let lean = 0.27;
    let position = match metadata {
        1 => Vec3::new(x - lean, y + rise, z),
        2 => Vec3::new(x + lean, y + rise, z),
        3 => Vec3::new(x, y + rise, z - lean),
        4 => Vec3::new(x, y + rise, z + lean),
        _ => Vec3::new(x, y, z),
    };
    particles.spawn(FxKind::Smoke, position, Vec3::ZERO);
    particles.spawn(FxKind::Flame, position, Vec3::ZERO);
}

/// `BlockRedstoneTorch.randomDisplayTick` for a lit torch.
fn redstone_torch(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    base: Vec3,
    metadata: u8,
) {
    let mut jitter = || (random.next_float() - 0.5) * 0.2;
    let x = base.x + 0.5 + jitter();
    let y = base.y + 0.7 + jitter();
    let z = base.z + 0.5 + jitter();
    let rise = 0.22;
    let lean = 0.27;
    let position = match metadata {
        1 => Vec3::new(x - lean, y + rise, z),
        2 => Vec3::new(x + lean, y + rise, z),
        3 => Vec3::new(x, y + rise, z - lean),
        4 => Vec3::new(x, y + rise, z + lean),
        _ => Vec3::new(x, y, z),
    };
    particles.spawn(FxKind::Reddust, position, Vec3::ZERO);
}

/// `BlockFurnace.randomDisplayTick` for a burning furnace.
fn furnace(particles: &mut EffectParticles, random: &mut JavaRandom, base: Vec3, metadata: u8) {
    let x = base.x + 0.5;
    let y = base.y + random.next_float() * 6.0 / 16.0;
    let z = base.z + 0.5;
    let front = 0.52;
    let along = random.next_float() * 0.6 - 0.3;
    let position = match metadata {
        4 => Vec3::new(x - front, y, z + along),
        5 => Vec3::new(x + front, y, z + along),
        2 => Vec3::new(x + along, y, z - front),
        3 => Vec3::new(x + along, y, z + front),
        _ => return,
    };
    particles.spawn(FxKind::Smoke, position, Vec3::ZERO);
    particles.spawn(FxKind::Flame, position, Vec3::ZERO);
}

/// `BlockFire.randomDisplayTick`: smoke off whatever is burning.
fn fire(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    chunks: &WorldChunks,
    cell: IVec3,
) {
    let burns = |offset: IVec3| {
        let at = cell + offset;
        chunks
            .block_at(at.x, at.y, at.z)
            .is_some_and(can_catch_fire)
    };
    let below = cell - IVec3::Y;
    let supported = chunks
        .block_at(below.x, below.y, below.z)
        .is_some_and(|block| block.is_normal_cube() || can_catch_fire(block));
    let base = cell.as_vec3();
    if supported {
        for _ in 0..3 {
            let x = base.x + random.next_float();
            let y = base.y + random.next_float() * 0.5 + 0.5;
            let z = base.z + random.next_float();
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
        return;
    }
    if burns(IVec3::NEG_X) {
        for _ in 0..2 {
            let x = base.x + random.next_float() * 0.1;
            let y = base.y + random.next_float();
            let z = base.z + random.next_float();
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
    if burns(IVec3::X) {
        for _ in 0..2 {
            let x = base.x + 1.0 - random.next_float() * 0.1;
            let y = base.y + random.next_float();
            let z = base.z + random.next_float();
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
    if burns(IVec3::NEG_Z) {
        for _ in 0..2 {
            let x = base.x + random.next_float();
            let y = base.y + random.next_float();
            let z = base.z + random.next_float() * 0.1;
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
    if burns(IVec3::Z) {
        for _ in 0..2 {
            let x = base.x + random.next_float();
            let y = base.y + random.next_float();
            let z = base.z + 1.0 - random.next_float() * 0.1;
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
    if burns(IVec3::Y) {
        for _ in 0..2 {
            let x = base.x + random.next_float();
            let y = base.y + 1.0 - random.next_float() * 0.1;
            let z = base.z + random.next_float();
            particles.spawn(FxKind::LargeSmoke, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
}

/// `BlockRedstoneOre.sparkle`: dust on the faces that are not buried.
pub fn sparkle(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    chunks: &WorldChunks,
    cell: IVec3,
) {
    let opaque = |offset: IVec3| {
        let at = cell + offset;
        chunks
            .block_at(at.x, at.y, at.z)
            .is_some_and(Block::is_opaque_cube)
    };
    let inset = 0.0625;
    let base = cell.as_vec3();
    for face in 0..6 {
        let mut x = base.x + random.next_float();
        let mut y = base.y + random.next_float();
        let mut z = base.z + random.next_float();
        match face {
            0 if !opaque(IVec3::Y) => y = base.y + 1.0 + inset,
            1 if !opaque(IVec3::NEG_Y) => y = base.y - inset,
            2 if !opaque(IVec3::Z) => z = base.z + 1.0 + inset,
            3 if !opaque(IVec3::NEG_Z) => z = base.z - inset,
            4 if !opaque(IVec3::X) => x = base.x + 1.0 + inset,
            5 if !opaque(IVec3::NEG_X) => x = base.x - inset,
            _ => {}
        }
        if x < base.x
            || x > base.x + 1.0
            || y < 0.0
            || y > base.y + 1.0
            || z < base.z
            || z > base.z + 1.0
        {
            particles.spawn(FxKind::Reddust, Vec3::new(x, y, z), Vec3::ZERO);
        }
    }
}

/// `BlockRedstoneWire.randomDisplayTick`: dust tinted by the signal.
fn redstone_wire(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    base: Vec3,
    metadata: u8,
) {
    if metadata == 0 {
        return;
    }
    let x = base.x + 0.5 + (random.next_float() - 0.5) * 0.2;
    let y = base.y + 0.0625;
    let z = base.z + 0.5 + (random.next_float() - 0.5) * 0.2;
    let power = f32::from(metadata) / 15.0;
    let red = power * 0.6 + 0.4;
    let green = (power * power * 0.7 - 0.5).max(0.0);
    let blue = (power * power * 0.6 - 0.7).max(0.0);
    particles.spawn(
        FxKind::Reddust,
        Vec3::new(x, y, z),
        Vec3::new(red, green, blue),
    );
}

/// `BlockRedstoneRepeater.randomDisplayTick` for a powered repeater.
fn repeater(particles: &mut EffectParticles, random: &mut JavaRandom, base: Vec3, metadata: u8) {
    let x = base.x + 0.5 + (random.next_float() - 0.5) * 0.2;
    let y = base.y + 0.4 + (random.next_float() - 0.5) * 0.2;
    let z = base.z + 0.5 + (random.next_float() - 0.5) * 0.2;
    let mut dx = 0.0;
    let mut dz = 0.0;
    if random.next_int(2) == 0 {
        match metadata & 3 {
            0 => dz = -0.3125,
            1 => dx = 0.3125,
            2 => dz = 0.3125,
            _ => dx = -0.3125,
        }
    } else {
        let delay = REPEATER_TORCH_OFFSET[usize::from((metadata & 12) >> 2)];
        match metadata & 3 {
            0 => dz = delay,
            1 => dx = -delay,
            2 => dz = -delay,
            _ => dx = delay,
        }
    }
    particles.spawn(FxKind::Reddust, Vec3::new(x + dx, y, z + dz), Vec3::ZERO);
}

/// `BlockPortal.randomDisplayTick`: four swirls drifting off the plane.
fn portal(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    chunks: &WorldChunks,
    cell: IVec3,
) {
    let base = cell.as_vec3();
    let is_portal =
        |dx: i32| chunks.block_at(cell.x + dx, cell.y, cell.z) == Some(Block::NetherPortal);
    for _ in 0..4 {
        let mut x = base.x + random.next_float();
        let y = base.y + random.next_float();
        let mut z = base.z + random.next_float();
        let side = random.next_int(2) as f32 * 2.0 - 1.0;
        let mut velocity = Vec3::new(
            (random.next_float() - 0.5) * 0.5,
            (random.next_float() - 0.5) * 0.5,
            (random.next_float() - 0.5) * 0.5,
        );
        if !is_portal(-1) && !is_portal(1) {
            x = base.x + 0.5 + 0.25 * side;
            velocity.x = random.next_float() * 2.0 * side;
        } else {
            z = base.z + 0.5 + 0.25 * side;
            velocity.z = random.next_float() * 2.0 * side;
        }
        particles.spawn(FxKind::Portal, Vec3::new(x, y, z), velocity);
    }
}

/// `BlockFluid.randomDisplayTick` for lava: now and then a pop at the surface.
fn lava(
    particles: &mut EffectParticles,
    random: &mut JavaRandom,
    chunks: &WorldChunks,
    cell: IVec3,
) {
    let above = chunks.block_at(cell.x, cell.y + 1, cell.z);
    if above == Some(Block::Air) && random.next_int(100) == 0 {
        let base = cell.as_vec3();
        let x = base.x + random.next_float();
        let z = base.z + random.next_float();
        particles.spawn(FxKind::Lava, Vec3::new(x, base.y + 1.0, z), Vec3::ZERO);
    }
}

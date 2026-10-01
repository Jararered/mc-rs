//! Beta `WorldGenFlowers`, `WorldGenTallGrass`, and `WorldGenDeadBush`, with
//! the `canBlockStay` rules of the plants they place.

use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::plant_grows_on;
use crate::random::JavaRandom;
use crate::world::chunk::CHUNK_HEIGHT;

use super::world::PopulationWorld;
use super::world::is_air_or_leaves;

/// One scatter offset: `nextInt(8) - nextInt(8)` horizontally and
/// `nextInt(4) - nextInt(4)` vertically, drawn x, y, z.
fn scatter(rand: &mut JavaRandom, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
    let x = x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
    let y = y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
    let z = z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
    (x, y, z)
}

/// `BlockFlower.canBlockStay` and the `BlockMushroom` and `BlockDeadBush`
/// overrides.
fn can_stay(world: &PopulationWorld, x: i32, y: i32, z: i32, block: Id) -> bool {
    let below = world.get(x, y - 1, z);
    match block {
        Id::BrownMushroom | Id::RedMushroom => {
            (0..CHUNK_HEIGHT as i32).contains(&y)
                && world.light(x, y, z) < 13
                && is_opaque_cube(below)
        }
        Id::DeadBush => lit_or_open(world, x, y, z) && below == Id::Sand,
        _ => lit_or_open(world, x, y, z) && plant_grows_on(below),
    }
}

fn lit_or_open(world: &PopulationWorld, x: i32, y: i32, z: i32) -> bool {
    world.light(x, y, z) >= 8 || world.sees_sky(x, y, z)
}

/// `WorldGenFlowers`: 64 attempts. Beta also places mushrooms with it.
pub(super) fn flower_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    block: Id,
) {
    for _ in 0..64 {
        let (x, y, z) = scatter(rand, x, y, z);
        if world.is_air(x, y, z) && can_stay(world, x, y, z, block) {
            world.set(x, y, z, block);
        }
    }
}

/// Lower a patch origin through air and leaves, as the tall grass and dead
/// bush generators do before scattering.
fn descend(world: &PopulationWorld, x: i32, mut y: i32, z: i32) -> i32 {
    while is_air_or_leaves(world.get(x, y, z)) && y > 0 {
        y -= 1;
    }
    y
}

/// `WorldGenTallGrass`: 128 attempts around the ground under the origin.
pub(super) fn tall_grass_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
    block: Id,
) {
    let y = descend(world, x, y, z);
    for _ in 0..128 {
        let (x, y, z) = scatter(rand, x, y, z);
        if world.is_air(x, y, z) && can_stay(world, x, y, z, block) {
            world.set(x, y, z, block);
        }
    }
}

/// `WorldGenDeadBush`: four attempts around the ground under the origin.
pub(super) fn dead_bush_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) {
    let y = descend(world, x, y, z);
    for _ in 0..4 {
        let (x, y, z) = scatter(rand, x, y, z);
        if world.is_air(x, y, z) && can_stay(world, x, y, z, Id::DeadBush) {
            world.set(x, y, z, Id::DeadBush);
        }
    }
}

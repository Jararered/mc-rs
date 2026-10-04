//! Beta reed (sugar cane) patches from `WorldGenReed`.

use crate::block::blocks::Block;
use crate::random::JavaRandom;

use super::world::PopulationWorld;
use super::world::is_water;

/// Water beside `(x, y, z)`, in `BlockReed.canPlaceBlockAt`'s order.
fn adjacent_water(world: &PopulationWorld, x: i32, y: i32, z: i32) -> bool {
    [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
        .into_iter()
        .any(|(x, z)| is_water(world.get(x, y, z)))
}

/// Beta 1.7.3 `BlockReed.canBlockStay`: on another reed, or on grass or dirt
/// beside water. Unlike the game's placement rule, sand never holds a reed.
fn can_stay(world: &PopulationWorld, x: i32, y: i32, z: i32) -> bool {
    match world.get(x, y - 1, z) {
        Block::SugarCane => true,
        Block::Grass | Block::Dirt => adjacent_water(world, x, y - 1, z),
        _ => false,
    }
}

/// Twenty attempts at the origin's height, each needing water beside the
/// block below, stacking two to four segments.
pub(super) fn reed_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) {
    for _ in 0..20 {
        let x = x + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = z + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        if !world.is_air(x, y, z) || !adjacent_water(world, x, y - 1, z) {
            continue;
        }
        let bound = rand.next_int(3) + 1;
        let height = 2 + rand.next_int(bound) as i32;
        for offset in 0..height {
            if can_stay(world, x, y + offset, z) {
                world.set(x, y + offset, z, Block::SugarCane);
            }
        }
    }
}

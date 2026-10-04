//! Beta pumpkin patches from `WorldGenPumpkin`.

use crate::block::blocks::Block;
use crate::random::JavaRandom;

use super::world::PopulationWorld;

/// 64 attempts on air above grass. Each placed pumpkin draws its facing.
pub(super) fn pumpkin_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) {
    for _ in 0..64 {
        let x = x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        if world.is_air(x, y, z) && world.get(x, y - 1, z) == Block::Grass {
            world.set_with_metadata(x, y, z, Block::Pumpkin, rand.next_int(4) as u8);
        }
    }
}

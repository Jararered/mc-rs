//! Beta desert cactus patches from `WorldGenCactus`.

use crate::block::blocks::Block;
use crate::block::properties::cactus_can_stay;
use crate::random::JavaRandom;

use super::world::PopulationWorld;

/// Ten attempts, each stacking one to three segments that `BlockCactus`
/// allows to stay.
pub(super) fn cactus_patch(
    world: &mut PopulationWorld,
    rand: &mut JavaRandom,
    x: i32,
    y: i32,
    z: i32,
) {
    for _ in 0..10 {
        let x = x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        if !world.is_air(x, y, z) {
            continue;
        }
        let bound = rand.next_int(3) + 1;
        let height = 1 + rand.next_int(bound) as i32;
        for offset in 0..height {
            let cactus_y = y + offset;
            let neighbors = [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)]
                .map(|(x, z)| world.get(x, cactus_y, z));
            if cactus_can_stay(world.get(x, cactus_y - 1, z), neighbors) {
                world.set(x, cactus_y, z, Block::Cactus);
            }
        }
    }
}

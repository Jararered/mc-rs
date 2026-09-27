//! Beta `BlockTNT`: power and primed player breaks spawn an 80-tick fuse.
use bevy::math::IVec3;

use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Tnt;
pub static TNT: Tnt = Tnt;

fn check_power(world: &mut TickWorld, position: IVec3) {
    if world.block(position) == Id::Tnt && world.block_indirectly_getting_powered(position) {
        world.set_metadata(position, 1);
        world.set_block_notify(position, Id::Air);
    }
}

impl BlockBehavior for Tnt {
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        check_power(world, position);
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Id) {
        if super::super::behavior::behavior(neighbor).can_provide_power() {
            check_power(world, position);
        }
    }

    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Id, metadata: u8) {
        if metadata & 1 != 0 {
            world.prime_tnt(position, 80);
        }
    }
}

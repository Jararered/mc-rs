//! Redstone ore: Beta's `BlockRedstoneOre`. Touching it lights it up, and a
//! random tick puts it out again.

use bevy::math::IVec3;

use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct RedstoneOre;
pub static REDSTONE_ORE: RedstoneOre = RedstoneOre;

/// `BlockRedstoneOre.func_320_h`: light unlit ore.
fn glow(world: &mut TickWorld, position: IVec3) {
    if world.block(position) == Id::RedstoneOre {
        world.set_block_notify(position, Id::LitRedstoneOre);
    }
}

impl BlockBehavior for RedstoneOre {
    /// Only the lit ore ticks on load, which is what dims it.
    fn ticks_randomly(&self, block: Id) -> bool {
        block == Id::LitRedstoneOre
    }

    fn tick_rate(&self, _block: Id) -> u32 {
        30
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block(position) == Id::LitRedstoneOre {
            world.set_block_notify(position, Id::RedstoneOre);
        }
    }

    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        glow(world, position);
    }

    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        glow(world, position);
    }

    fn entity_walked(&self, world: &mut TickWorld, position: IVec3) {
        glow(world, position);
    }
}

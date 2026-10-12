//! Redstone ore: Beta's `BlockRedstoneOre`. Touching it lights it up, and a
//! random tick puts it out again.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct RedstoneOre;
pub static REDSTONE_ORE: RedstoneOre = RedstoneOre;

/// `BlockRedstoneOre.func_320_h`: light unlit ore.
fn glow(world: &mut TickWorld, position: IVec3) {
    if world.block(position) == Block::RedstoneOreInactive {
        world.set_block_notify(position, Block::RedstoneOreActive);
    }
}

impl BlockBehavior for RedstoneOre {
    /// Only the lit ore ticks on load, which is what dims it.
    fn ticks_randomly(&self, block: Block) -> bool {
        block == Block::RedstoneOreActive
    }

    fn tick_rate(&self, _block: Block) -> u32 {
        30
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block(position) == Block::RedstoneOreActive {
            world.set_block_notify(position, Block::RedstoneOreInactive);
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

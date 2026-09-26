//! Sand and gravel: Beta's `BlockSand` and `BlockGravel`.
//!
//! Any change around the block schedules a tick three ticks later. If
//! nothing holds it up by then it becomes a
//! [`FallingBlock`](crate::entity::falling_block::FallingBlock) entity, or
//! drops straight to the ground when the world around it is not loaded.

use bevy::math::IVec3;

use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

use super::fluid::is_liquid;

/// `BlockSand.tickRate`.
pub const FALL_DELAY: u32 = 3;
/// `BlockSand.tryToFall` spawns an entity only when chunks this far around
/// the block exist.
const ENTITY_REACH: i32 = 32;

/// `BlockSand.canFallBelow`: air, fire, water, and lava.
pub fn can_fall_below(block: Id) -> bool {
    block == Id::Air || block == Id::Fire || is_liquid(block)
}

/// `World.canBlockBePlacedAt` for a landing falling block: the cell holds
/// air, a fluid, fire, or a snow layer, which the block replaces.
pub fn can_land_in(block: Id) -> bool {
    can_fall_below(block) || block == Id::SnowLayer
}

pub struct Falling;
pub static FALLING: Falling = Falling;

impl BlockBehavior for Falling {
    fn tick_rate(&self, _block: Id) -> u32 {
        FALL_DELAY
    }

    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        world.schedule(position, block, FALL_DELAY);
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Id) {
        let block = world.block(position);
        world.schedule(position, block, FALL_DELAY);
    }

    /// `BlockSand.tryToFall`.
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        if position.y < 0 || !can_fall_below(world.block(position - IVec3::Y)) {
            return;
        }
        if world.area_loaded(position, ENTITY_REACH) {
            world.spawn_falling_block(position, block);
            return;
        }
        world.set_block_notify(position, Id::Air);
        let mut landing = position;
        while landing.y > 0 && can_fall_below(world.block(landing - IVec3::Y)) {
            landing.y -= 1;
        }
        if landing.y > 0 {
            world.set_block_notify(landing, block);
        }
    }
}

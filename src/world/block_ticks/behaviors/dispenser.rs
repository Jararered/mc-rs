//! Beta `BlockDispenser`: four-tick level-triggered dispensing.
use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Dispenser;
pub static DISPENSER: Dispenser = Dispenser;

fn powered(world: &mut TickWorld, pos: IVec3) -> bool {
    world.block_indirectly_getting_powered(pos)
        || world.block_indirectly_getting_powered(pos + IVec3::Y)
}

impl BlockBehavior for Dispenser {
    fn tick_rate(&self, _: Block) -> u32 {
        4
    }

    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        if world.metadata(pos) == 0 {
            let north = world.is_normal_cube(pos + IVec3::NEG_Z);
            let south = world.is_normal_cube(pos + IVec3::Z);
            let west = world.is_normal_cube(pos + IVec3::NEG_X);
            let east = world.is_normal_cube(pos + IVec3::X);
            let facing = if east && !west {
                4
            } else if west && !east {
                5
            } else if south && !north {
                2
            } else {
                3
            };
            world.set_metadata(pos, facing);
        }
        if powered(world, pos) {
            world.schedule(pos, Block::Dispenser, 4);
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, neighbor: Block) {
        if super::super::behavior::behavior(neighbor).can_provide_power() && powered(world, pos) {
            world.schedule(pos, Block::Dispenser, 4);
        }
    }

    fn update_tick(&self, world: &mut TickWorld, pos: IVec3) {
        if !powered(world, pos) {
            return;
        }
        let mut selected = None;
        let mut seen = 0;
        for slot in 0..9 {
            if world
                .dispenser(pos)
                .is_some_and(|dispenser| dispenser.slots[slot].is_some())
            {
                seen += 1;
                if world.random().next_int(seen) == 0 {
                    selected = Some(slot);
                }
            }
        }
        if let Some(slot) = selected {
            world.dispense(pos, world.metadata(pos), slot);
        }
    }
}

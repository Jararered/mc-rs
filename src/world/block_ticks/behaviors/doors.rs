//! Beta two-cell doors and powered/hand-activated wooden trapdoors.
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Door;
pub static DOOR: Door = Door;

fn toggle(world: &mut TickWorld, lower: IVec3, open: bool) {
    let block = world.block(lower);
    let meta = world.metadata(lower);
    if (meta & 4 != 0) == open {
        return;
    }
    let next = meta ^ 4;
    world.set_metadata_notify(lower, next);
    if world.block(lower + IVec3::Y) == block {
        world.set_metadata_notify(lower + IVec3::Y, next | 8);
    }
}
impl BlockBehavior for Door {
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        if world.block(position) != Id::WoodenDoor {
            return;
        }
        let lower = if world.metadata(position) & 8 != 0 {
            position - IVec3::Y
        } else {
            position
        };
        toggle(world, lower, world.metadata(lower) & 4 == 0);
    }
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        self.activated(world, position);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Id) {
        let block = world.block(position);
        if world.metadata(position) & 8 != 0 {
            if world.block(position - IVec3::Y) != block {
                world.set_block_notify(position, Id::Air);
            } else if super::super::behavior::behavior(neighbor).can_provide_power() {
                self.neighbor_changed(world, position - IVec3::Y, neighbor);
            }
        } else {
            let upper = position + IVec3::Y;
            if world.block(upper) != block || !world.is_normal_cube(position - IVec3::Y) {
                world.set_block_notify(position, Id::Air);
                if world.block(upper) == block {
                    world.set_block_notify(upper, Id::Air);
                }
                world.drop_block_as_item(position, block, 0);
            } else if super::super::behavior::behavior(neighbor).can_provide_power() {
                let powered = world.block_indirectly_getting_powered(position)
                    || world.block_indirectly_getting_powered(upper);
                toggle(world, position, powered);
            }
        }
    }
}

pub struct Trapdoor;
pub static TRAPDOOR: Trapdoor = Trapdoor;
fn support(meta: u8) -> IVec3 {
    match meta & 3 {
        0 => IVec3::Z,
        1 => IVec3::NEG_Z,
        2 => IVec3::X,
        _ => IVec3::NEG_X,
    }
}
impl BlockBehavior for Trapdoor {
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        world.set_metadata_notify(position, world.metadata(position) ^ 4);
    }
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        self.activated(world, position);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Id) {
        if !world.is_normal_cube(position + support(world.metadata(position))) {
            world.drop_block_as_item(position, Id::Trapdoor, world.metadata(position));
            world.set_block_notify(position, Id::Air);
        } else if super::super::behavior::behavior(neighbor).can_provide_power() {
            let powered = world.block_indirectly_getting_powered(position);
            if (world.metadata(position) & 4 != 0) != powered {
                world.set_metadata_notify(position, world.metadata(position) ^ 4);
            }
        }
    }
}

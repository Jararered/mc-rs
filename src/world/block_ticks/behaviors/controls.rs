//! Beta's power sources that attach to a supporting block.
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

fn attached(meta: u8) -> IVec3 {
    match meta & 7 {
        1 => IVec3::NEG_X,
        2 => IVec3::X,
        3 => IVec3::NEG_Z,
        4 => IVec3::Z,
        _ => IVec3::NEG_Y,
    }
}
fn support_lost(world: &mut TickWorld, position: IVec3) -> bool {
    if !world.is_normal_cube(position + attached(world.metadata(position))) {
        let block = world.block(position);
        world.drop_block_as_item(position, block, world.metadata(position));
        world.set_block_notify(position, Id::Air);
        true
    } else {
        false
    }
}
fn notify_switch(world: &mut TickWorld, position: IVec3, block: Id) {
    world.notify_neighbors(position, block);
    world.notify_neighbors(position + attached(world.metadata(position)), block);
}

pub struct Lever;
pub static LEVER: Lever = Lever;
impl BlockBehavior for Lever {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, _: u8) -> bool {
        world.metadata(position) & 8 != 0
    }
    fn strong_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        self.weak_power(world, position, side)
            && side
                == match world.metadata(position) & 7 {
                    1 => 5,
                    2 => 4,
                    3 => 3,
                    4 => 2,
                    _ => 1,
                }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Id) {
        support_lost(world, position);
    }
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        world.set_metadata_notify(position, world.metadata(position) ^ 8);
        notify_switch(world, position, Id::Lever);
    }
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        self.activated(world, position);
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Id, metadata: u8) {
        if metadata & 8 != 0 {
            world.notify_neighbors(position, Id::Lever);
            world.notify_neighbors(position + attached(metadata), Id::Lever);
        }
    }
}

pub struct Button;
pub static BUTTON: Button = Button;
impl BlockBehavior for Button {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn ticks_randomly(&self, _: Id) -> bool {
        true
    }
    fn tick_rate(&self, _: Id) -> u32 {
        20
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, _: u8) -> bool {
        world.metadata(position) & 8 != 0
    }
    fn strong_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        self.weak_power(world, position, side)
            && side
                == match world.metadata(position) & 7 {
                    1 => 5,
                    2 => 4,
                    3 => 3,
                    4 => 2,
                    _ => 1,
                }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Id) {
        support_lost(world, position);
    }
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 != 0 {
            return;
        }
        world.set_metadata_notify(position, world.metadata(position) | 8);
        notify_switch(world, position, Id::StoneButton);
        world.schedule(position, Id::StoneButton, 20);
    }
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        self.activated(world, position);
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 != 0 {
            world.set_metadata_notify(position, world.metadata(position) & 7);
            notify_switch(world, position, Id::StoneButton);
        }
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Id, metadata: u8) {
        if metadata & 8 != 0 {
            world.notify_neighbors(position, Id::StoneButton);
            world.notify_neighbors(position + attached(metadata), Id::StoneButton);
        }
    }
}

pub struct Plate;
pub static PLATE: Plate = Plate;
impl BlockBehavior for Plate {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn ticks_randomly(&self, _: Id) -> bool {
        true
    }
    fn tick_rate(&self, _: Id) -> u32 {
        20
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, _: u8) -> bool {
        world.metadata(position) != 0
    }
    fn strong_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        side == 1 && self.weak_power(world, position, side)
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Id) {
        if !world.is_normal_cube(position - IVec3::Y) {
            let block = world.block(position);
            world.drop_block_as_item(position, block, world.metadata(position));
            world.set_block_notify(position, Id::Air);
        }
    }
    fn entity_collided(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) == 0 {
            update_plate(world, position);
        }
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) != 0 {
            update_plate(world, position);
        }
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, block: Id, metadata: u8) {
        if metadata > 0 {
            world.notify_neighbors(position, block);
            world.notify_neighbors(position - IVec3::Y, block);
        }
    }
}

fn update_plate(world: &mut TickWorld, position: IVec3) {
    let block = world.block(position);
    let occupied = world.occupant_on(position, block);
    if occupied != (world.metadata(position) != 0) {
        world.set_metadata_notify(position, u8::from(occupied));
        world.notify_neighbors(position, block);
        world.notify_neighbors(position - IVec3::Y, block);
    }
    if occupied {
        world.schedule(position, block, 20);
    }
}

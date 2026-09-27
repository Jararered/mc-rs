//! Beta's normal, powered and detector rails.
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Rail;
pub static RAIL: Rail = Rail;
pub struct Detector;
pub static DETECTOR: Detector = Detector;

fn is_rail(block: Id) -> bool {
    matches!(block, Id::Rail | Id::PoweredRail | Id::DetectorRail)
}
fn connected(world: &TickWorld, cell: IVec3) -> bool {
    [0, -1, 1]
        .into_iter()
        .any(|dy| is_rail(world.block(cell + IVec3::Y * dy)))
}
fn shape(world: &TickWorld, pos: IVec3, powered: bool) -> u8 {
    let north = connected(world, pos + IVec3::NEG_Z);
    let south = connected(world, pos + IVec3::Z);
    let east = connected(world, pos + IVec3::X);
    let west = connected(world, pos + IVec3::NEG_X);
    let curved = world.block(pos) == Id::Rail;
    let mut shape = if (north || south) && !(east || west) {
        0
    } else if (east || west) && !(north || south) {
        1
    } else if curved && south && east && !north && !west {
        6
    } else if curved && south && west && !north && !east {
        7
    } else if curved && north && west && !south && !east {
        8
    } else if curved && north && east && !south && !west {
        9
    } else if east || west {
        1
    } else {
        0
    };
    if curved && (north || south) && (east || west) {
        let corners = if powered {
            [
                (south && east, 6),
                (south && west, 7),
                (north && east, 9),
                (north && west, 8),
            ]
        } else {
            [
                (north && west, 8),
                (north && east, 9),
                (south && west, 7),
                (south && east, 6),
            ]
        };
        for (exists, corner) in corners {
            if exists {
                shape = corner;
            }
        }
    }
    if shape == 0 {
        if is_rail(world.block(pos + IVec3::NEG_Z + IVec3::Y)) {
            shape = 4;
        }
        if is_rail(world.block(pos + IVec3::Z + IVec3::Y)) {
            shape = 5;
        }
    } else if shape == 1 {
        if is_rail(world.block(pos + IVec3::X + IVec3::Y)) {
            shape = 2;
        }
        if is_rail(world.block(pos + IVec3::NEG_X + IVec3::Y)) {
            shape = 3;
        }
    }
    shape
}

fn update_shape(world: &mut TickWorld, position: IVec3) {
    let block = world.block(position);
    let old = world.metadata(position);
    let powered = world.block_indirectly_getting_powered(position);
    let new = shape(world, position, powered) | (old & 8);
    if new != old {
        world.set_metadata_notify(position, new);
    }
    if block == Id::PoweredRail {
        update_power(world, position);
    }
}
fn propagation(world: &mut TickWorld, pos: IVec3, forward: bool, depth: u8) -> bool {
    if depth >= 8 {
        return false;
    }
    let shape = world.metadata(pos) & 7;
    let (offset, may_descend) = match (shape, forward) {
        (0, true) => (IVec3::Z, true),
        (0, false) => (IVec3::NEG_Z, true),
        (1, true) => (IVec3::NEG_X, true),
        (1, false) => (IVec3::X, true),
        (2, true) => (IVec3::NEG_X, true),
        (2, false) => (IVec3::X + IVec3::Y, false),
        (3, true) => (IVec3::NEG_X + IVec3::Y, false),
        (3, false) => (IVec3::X, true),
        (4, true) => (IVec3::Z, true),
        (4, false) => (IVec3::NEG_Z + IVec3::Y, false),
        (5, true) => (IVec3::Z + IVec3::Y, false),
        (5, false) => (IVec3::NEG_Z, true),
        _ => return false,
    };
    let axis_z = matches!(shape, 0 | 4 | 5);
    for next in [
        Some(pos + offset),
        may_descend.then_some(pos + offset - IVec3::Y),
    ]
    .into_iter()
    .flatten()
    {
        if world.block(next) != Id::PoweredRail {
            continue;
        }
        let next_meta = world.metadata(next);
        if next_meta & 8 != 0 && matches!(next_meta & 7, 0 | 4 | 5) == axis_z {
            if world.block_indirectly_getting_powered(next)
                || world.block_indirectly_getting_powered(next + IVec3::Y)
                || propagation(world, next, forward, depth + 1)
            {
                return true;
            }
        }
    }
    false
}
fn update_power(world: &mut TickWorld, position: IVec3) {
    let old = world.metadata(position);
    let powered = world.block_indirectly_getting_powered(position)
        || world.block_indirectly_getting_powered(position + IVec3::Y)
        || propagation(world, position, true, 0)
        || propagation(world, position, false, 0);
    if powered != (old & 8 != 0) {
        world.set_metadata_notify(position, (old & 7) | if powered { 8 } else { 0 });
        world.notify_neighbors(position - IVec3::Y, Id::PoweredRail);
    }
}
impl BlockBehavior for Rail {
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        if !world.is_normal_cube(position - IVec3::Y) {
            return;
        }
        update_shape(world, position);
        for dir in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            for dy in [-1, 0, 1] {
                let neighbor = position + dir + IVec3::Y * dy;
                if is_rail(world.block(neighbor)) {
                    world.notify_neighbor(neighbor, world.block(position));
                }
            }
        }
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Id, _: u8) {
        for dir in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            for dy in [-1, 0, 1] {
                let neighbor = position + dir + IVec3::Y * dy;
                if is_rail(world.block(neighbor)) {
                    world.notify_neighbor(neighbor, Id::Rail);
                }
            }
        }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Id) {
        let block = world.block(position);
        let meta = world.metadata(position);
        let slope_support = match meta & 7 {
            2 => Some(IVec3::X),
            3 => Some(IVec3::NEG_X),
            4 => Some(IVec3::NEG_Z),
            5 => Some(IVec3::Z),
            _ => None,
        };
        if !world.is_normal_cube(position - IVec3::Y)
            || slope_support.is_some_and(|dir| !world.is_normal_cube(position + dir))
        {
            world.drop_block_as_item(position, block, meta);
            world.set_block_notify(position, Id::Air);
        } else {
            update_shape(world, position);
        }
    }
}
impl BlockBehavior for Detector {
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
        side == 1 && self.weak_power(world, position, side)
    }
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        RAIL.on_added(world, position);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Id) {
        RAIL.neighbor_changed(world, position, neighbor);
    }
    fn entity_collided(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 == 0 {
            update_detector(world, position);
        }
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) & 8 != 0 {
            update_detector(world, position);
        }
    }
}
fn update_detector(world: &mut TickWorld, pos: IVec3) {
    let meta = world.metadata(pos);
    let occupied = world.occupant_on(pos, Id::DetectorRail);
    if occupied != (meta & 8 != 0) {
        world.set_metadata_notify(pos, (meta & 7) | if occupied { 8 } else { 0 });
        world.notify_neighbors(pos, Id::DetectorRail);
        world.notify_neighbors(pos - IVec3::Y, Id::DetectorRail);
    }
    if occupied {
        world.schedule(pos, Id::DetectorRail, 20);
    }
}

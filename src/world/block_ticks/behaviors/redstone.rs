//! Beta's dust, two-state repeater, and redstone torch power propagation.
use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Wire;
pub static WIRE: Wire = Wire;

const HORIZONTAL: [IVec3; 4] = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z];

fn wire_at(world: &TickWorld, position: IVec3) -> u8 {
    if world.block(position) == Block::RedstoneWire {
        world.metadata(position)
    } else {
        0
    }
}

fn adjacent_wire(world: &TickWorld, position: IVec3, offset: IVec3) -> u8 {
    let neighbor = position + offset;
    let mut strength = wire_at(world, neighbor);
    if world.is_normal_cube(neighbor) && !world.is_normal_cube(position + IVec3::Y) {
        strength = strength.max(wire_at(world, neighbor + IVec3::Y));
    } else if !world.is_normal_cube(neighbor) {
        strength = strength.max(wire_at(world, neighbor - IVec3::Y));
    }
    strength
}

fn wire_neighbors(world: &TickWorld, position: IVec3) -> [IVec3; 8] {
    let mut neighbors = [IVec3::ZERO; 8];
    for (index, offset) in HORIZONTAL.into_iter().enumerate() {
        let adjacent = position + offset;
        neighbors[index * 2] = adjacent;
        neighbors[index * 2 + 1] = adjacent
            + if world.is_normal_cube(adjacent) {
                IVec3::Y
            } else {
                IVec3::NEG_Y
            };
    }
    neighbors
}

fn propagate(world: &mut TickWorld, position: IVec3) {
    // Beta recursively visits adjacent dust. Drain the same work iteratively
    // so a large circuit cannot exhaust the Rust call stack. Writes inside
    // the loop notify nobody (`editingBlocks`), so the loop itself never
    // nests; the notifications after it may run another update, as Beta's
    // `updateAndPropagateCurrentStrength` does.
    let mut pending = vec![position];
    // `blocksNeedingUpdate`, in the order cells were added.
    let mut notifications = Vec::new();
    let mut queued = std::collections::HashSet::new();
    let mut work = 0usize;
    while let Some(position) = pending.pop() {
        if world.block(position) != Block::RedstoneWire {
            continue;
        }
        work += 1;
        if work > 262_144 {
            world.schedule(position, Block::RedstoneWire, 1);
            break;
        }
        let old = world.metadata(position);
        let powered =
            world.without_wire_power(|world| world.block_indirectly_getting_powered(position));
        let strength = if powered {
            15
        } else {
            HORIZONTAL
                .into_iter()
                .map(|offset| adjacent_wire(world, position, offset))
                .max()
                .unwrap_or(0)
                .saturating_sub(1)
        };
        if old == strength {
            continue;
        }
        world.set_editing(true);
        world.set_metadata_notify(position, strength);
        world.set_editing(false);
        pending.extend(wire_neighbors(world, position));
        // Beta tests the new strength after decrementing it, so a wire that
        // settles at 1 notifies as well as one that goes dark.
        if old == 0 || strength <= 1 {
            for cell in std::iter::once(position)
                .chain(crate::world::block_ticks::NEIGHBORS.map(|offset| position + offset))
            {
                if queued.insert(cell) {
                    notifications.push(cell);
                }
            }
        }
    }
    for cell in notifications {
        world.notify_neighbors(cell, Block::RedstoneWire);
    }
}

/// `notifyWireNeighborsOfNeighborChange`: a wire whose connections changed
/// may now point somewhere else, so everything around it and around its six
/// neighbors looks again.
fn notify_wire_neighbors(world: &mut TickWorld, position: IVec3) {
    if world.block(position) != Block::RedstoneWire {
        return;
    }
    world.notify_neighbors(position, Block::RedstoneWire);
    for offset in [
        IVec3::NEG_X,
        IVec3::X,
        IVec3::NEG_Z,
        IVec3::Z,
        IVec3::NEG_Y,
        IVec3::Y,
    ] {
        world.notify_neighbors(position + offset, Block::RedstoneWire);
    }
}

/// The tail `onBlockAdded` and `onBlockRemoval` share: the four wires beside
/// the cell, then the one a step up or down on each side.
fn notify_connected_wires(world: &mut TickWorld, position: IVec3) {
    for offset in HORIZONTAL {
        notify_wire_neighbors(world, position + offset);
    }
    for offset in HORIZONTAL {
        let adjacent = position + offset;
        let step = if world.is_normal_cube(adjacent) {
            IVec3::Y
        } else {
            IVec3::NEG_Y
        };
        notify_wire_neighbors(world, adjacent + step);
    }
}

fn is_power_provider_or_wire(world: &TickWorld, position: IVec3, side: i8) -> bool {
    let block = world.block(position);
    // A source on the step above or below (`side` -1) still connects; only
    // a repeater has to face the dust.
    match block {
        Block::RedstoneWire => true,
        Block::Repeater | Block::PoweredRepeater => {
            i16::from(side) == [2, 3, 0, 1][usize::from(world.metadata(position) & 3)]
        }
        _ => super::super::behavior::behavior(block).can_provide_power(),
    }
}

impl BlockBehavior for Wire {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        propagate(world, position);
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        if world.metadata(position) == 0 {
            return false;
        }
        if side == 1 {
            return true;
        }
        let x = IVec3::X;
        let z = IVec3::Z;
        let connected = HORIZONTAL.map(|offset| {
            let adjacent = position + offset;
            is_power_provider_or_wire(
                world,
                adjacent,
                match offset {
                    o if o == -x => 1,
                    o if o == x => 3,
                    o if o == -z => 2,
                    _ => 0,
                },
            ) || (!world.is_normal_cube(adjacent)
                && is_power_provider_or_wire(world, adjacent - IVec3::Y, -1))
                || (!world.is_normal_cube(position + IVec3::Y)
                    && world.is_normal_cube(adjacent)
                    && is_power_provider_or_wire(world, adjacent + IVec3::Y, -1))
        });
        let [west, east, north, south] = connected;
        if side >= 2 && side <= 5 && !connected.into_iter().any(|v| v) {
            return true;
        }
        match side {
            2 => north && !west && !east,
            3 => south && !west && !east,
            4 => west && !north && !south,
            5 => east && !north && !south,
            _ => false,
        }
    }
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        propagate(world, position);
        world.notify_neighbors(position + IVec3::Y, Block::RedstoneWire);
        world.notify_neighbors(position - IVec3::Y, Block::RedstoneWire);
        notify_connected_wires(world, position);
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Block, metadata: u8) {
        world.notify_neighbors(position + IVec3::Y, Block::RedstoneWire);
        world.notify_neighbors(position - IVec3::Y, Block::RedstoneWire);
        // Beta recomputes the emptied cell itself, which reaches the wires
        // it fed and, because the cell read as unpowered, notifies around it
        // and its six neighbors.
        for neighbor in wire_neighbors(world, position) {
            if world.block(neighbor) == Block::RedstoneWire {
                propagate(world, neighbor);
            }
        }
        if metadata > 0 {
            world.notify_neighbors(position, Block::RedstoneWire);
            for offset in crate::world::block_ticks::NEIGHBORS {
                world.notify_neighbors(position + offset, Block::RedstoneWire);
            }
        }
        notify_connected_wires(world, position);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Block) {
        if !world.is_normal_cube(position - IVec3::Y) {
            world.drop_block_as_item(position, Block::RedstoneWire, world.metadata(position));
            world.set_block_notify(position, Block::Air);
        } else {
            propagate(world, position);
        }
    }
}

pub struct Repeater;
pub static REPEATER: Repeater = Repeater;

fn repeater_input(world: &mut TickWorld, position: IVec3) -> bool {
    let direction = match world.metadata(position) & 3 {
        0 => IVec3::Z,
        1 => IVec3::NEG_X,
        2 => IVec3::NEG_Z,
        _ => IVec3::X,
    };
    let side = match world.metadata(position) & 3 {
        0 => 3,
        1 => 4,
        2 => 2,
        _ => 5,
    };
    let input = position + direction;
    world.block_indirectly_providing_power_to(input, side) || wire_at(world, input) > 0
}

impl BlockBehavior for Repeater {
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        if world.block(position) != Block::PoweredRepeater {
            return false;
        }
        let front = [3, 4, 2, 5][(world.metadata(position) & 3) as usize];
        side == front
    }
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        for offset in crate::world::block_ticks::NEIGHBORS {
            world.notify_neighbors(position + offset, world.block(position));
        }
        if world.block(position) == Block::Repeater && repeater_input(world, position) {
            world.schedule(position, Block::Repeater, 1);
        }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Block) {
        let block = world.block(position);
        if !world.is_normal_cube(position - IVec3::Y) {
            world.drop_block_as_item(position, block, world.metadata(position));
            world.set_block_notify(position, Block::Air);
            return;
        }
        if (block == Block::PoweredRepeater) != repeater_input(world, position) {
            let delay = 2 * (u32::from(world.metadata(position) >> 2) + 1);
            world.schedule(position, block, delay);
        }
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        let powered = repeater_input(world, position);
        let metadata = world.metadata(position);
        if block == Block::PoweredRepeater && !powered {
            world.set_block_and_metadata_notify(position, Block::Repeater, metadata);
        } else if block == Block::Repeater {
            world.set_block_and_metadata_notify(position, Block::PoweredRepeater, metadata);
            if !powered {
                world.schedule(
                    position,
                    Block::PoweredRepeater,
                    2 * (u32::from(metadata >> 2) + 1),
                );
            }
        }
    }
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        let meta = world.metadata(position);
        world.set_metadata_notify(position, (meta & 3) | ((meta + 4) & 12));
    }
}

pub struct RedstoneTorch;
pub static REDSTONE_TORCH: RedstoneTorch = RedstoneTorch;

/// `BlockTorch.canPlaceTorchOn` for a floor torch; side torches need a cube.
fn torch_supported(world: &TickWorld, position: IVec3) -> bool {
    let offset = torch_support(world.metadata(position));
    let support = position + offset;
    world.is_normal_cube(support)
        || (offset == IVec3::NEG_Y && world.block(support) == Block::Fence)
}

fn torch_support(metadata: u8) -> IVec3 {
    match metadata {
        1 => IVec3::NEG_X,
        2 => IVec3::X,
        3 => IVec3::NEG_Z,
        4 => IVec3::Z,
        _ => IVec3::NEG_Y,
    }
}
fn torch_powered(world: &mut TickWorld, position: IVec3) -> bool {
    let support = torch_support(world.metadata(position));
    let side = match support {
        IVec3::NEG_X => 4,
        IVec3::X => 5,
        IVec3::NEG_Z => 2,
        IVec3::Z => 3,
        _ => 0,
    };
    world.block_indirectly_providing_power_to(position + support, side)
}
/// BlockRedstoneTorch notifies around all six adjacent cells when it turns
/// on or off. This reaches consumers beside a solid block powered by the
/// torch (one cell diagonally from the torch itself).
fn notify_torch_neighbors(world: &mut TickWorld, position: IVec3) {
    for offset in [
        IVec3::NEG_Y,
        IVec3::Y,
        IVec3::NEG_X,
        IVec3::X,
        IVec3::NEG_Z,
        IVec3::Z,
    ] {
        world.notify_neighbors(position + offset, Block::RedstoneTorch);
    }
}

impl BlockBehavior for RedstoneTorch {
    fn can_provide_power(&self) -> bool {
        true
    }
    fn ticks_randomly(&self, _: Block) -> bool {
        true
    }
    fn tick_rate(&self, _: Block) -> u32 {
        2
    }
    fn weak_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        world.block(position) == Block::RedstoneTorch
            && side
                != match world.metadata(position) {
                    1 => 5,
                    2 => 4,
                    3 => 3,
                    4 => 2,
                    _ => 1,
                }
    }
    fn strong_power(&self, world: &mut TickWorld, position: IVec3, side: u8) -> bool {
        side == 0 && self.weak_power(world, position, side)
    }
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        if world.block(position) == Block::RedstoneTorch {
            notify_torch_neighbors(world, position);
        }
    }
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, previous: Block, _: u8) {
        if previous == Block::RedstoneTorch {
            notify_torch_neighbors(world, position);
        }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _: Block) {
        if !torch_supported(world, position) {
            world.drop_block_as_item(position, Block::RedstoneTorch, 0);
            world.set_block_notify(position, Block::Air);
        } else {
            world.schedule(position, world.block(position), 2);
        }
    }
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let powered = torch_powered(world, position);
        let block = world.block(position);
        let meta = world.metadata(position);
        if powered && block == Block::RedstoneTorch {
            world.set_block_and_metadata_notify(position, Block::UnlitRedstoneTorch, meta);
            world.torch_burned_out(position, true);
        } else if !powered
            && block == Block::UnlitRedstoneTorch
            && !world.torch_burned_out(position, false)
        {
            world.set_block_and_metadata_notify(position, Block::RedstoneTorch, meta);
        }
    }
}

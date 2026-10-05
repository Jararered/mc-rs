//! Water and lava: Beta's `BlockFluid`, `BlockFlowing`, and `BlockStationary`.
//!
//! Metadata is the flow decay. `0` is a source, `1..=7` is how far the fluid
//! has spread from one, and `8` and up marks fluid falling from the block
//! above. Flowing blocks run a scheduled tick every [`Fluid::tick_rate`] to
//! spread or dry up, and settle into the still block once they stop
//! changing. A still block turns back into a flowing one when a neighbor
//! changes, so breaking a lake's wall lets it flow again.

use bevy::math::IVec3;

use crate::block::blocks::Block;
pub use crate::block::fluids::Fluid;
pub use crate::block::fluids::is_lava;
pub use crate::block::fluids::is_liquid;
pub use crate::block::fluids::is_water;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

/// `BlockFluid.getFlowDecay`: the cell's metadata if it holds `fluid`,
/// otherwise `-1`.
fn flow_decay(world: &TickWorld, position: IVec3, fluid: Fluid) -> i32 {
    if Fluid::of(world.block(position)) == Some(fluid) {
        i32::from(world.metadata(position))
    } else {
        -1
    }
}

/// `BlockFluid.checkForHarden`: lava touching water on a side or from above
/// becomes obsidian if it is a source, or cobblestone if it spread four or
/// fewer blocks.
fn check_for_harden(world: &mut TickWorld, position: IVec3, block: Block) {
    if world.block(position) != block || Fluid::of(block) != Some(Fluid::Lava) {
        return;
    }
    let touches_water = [IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X, IVec3::Y]
        .into_iter()
        .any(|offset| is_water(world.block(position + offset)));
    if !touches_water {
        return;
    }
    let metadata = world.metadata(position);
    if metadata == 0 {
        world.set_block_notify(position, Block::Obsidian);
    } else if metadata <= 4 {
        world.set_block_notify(position, Block::Cobblestone);
    }
}

/// `BlockFlowing.blockBlocksFlow`: doors, signs, ladders, and reeds hold
/// fluid back despite their non-solid material.
fn blocks_flow(world: &TickWorld, position: IVec3) -> bool {
    let block = world.block(position);
    if matches!(
        block,
        Block::WoodenDoor | Block::IronDoor | Block::StandingSign | Block::SugarCane
    ) || block.is_ladder()
    {
        return true;
    }
    block != Block::Air && block.is_solid_material()
}

/// `BlockFlowing.liquidCanDisplaceBlock`: neither this fluid, nor lava, nor a
/// block that holds fluid back.
fn can_displace(world: &TickWorld, position: IVec3, fluid: Fluid) -> bool {
    match Fluid::of(world.block(position)) {
        Some(other) if other == fluid => false,
        Some(Fluid::Lava) => false,
        _ => !blocks_flow(world, position),
    }
}

/// Horizontal flow directions in `BlockFlowing`'s order: `-x`, `+x`, `-z`,
/// `+z`.
const DIRECTIONS: [IVec3; 4] = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z];

/// The direction that leads back where a search came from.
const fn opposite(direction: usize) -> usize {
    direction ^ 1
}

/// Whether flow may enter `position` sideways: not blocked, and not already
/// a source of the same fluid.
fn open_for_flow(world: &TickWorld, position: IVec3, fluid: Fluid) -> bool {
    !blocks_flow(world, position)
        && (Fluid::of(world.block(position)) != Some(fluid) || world.metadata(position) != 0)
}

/// `BlockFlowing.calculateFlowCost`: the fewest steps from `position` to a
/// cell the fluid could fall from, searching up to four steps out and never
/// doubling back.
fn flow_cost(world: &TickWorld, position: IVec3, fluid: Fluid, steps: i32, from: usize) -> i32 {
    let mut cost = 1000;
    for (direction, offset) in DIRECTIONS.into_iter().enumerate() {
        if direction == opposite(from) {
            continue;
        }
        let next = position + offset;
        if !open_for_flow(world, next, fluid) {
            continue;
        }
        if !blocks_flow(world, next - IVec3::Y) {
            return steps;
        }
        if steps < 4 {
            cost = cost.min(flow_cost(world, next, fluid, steps + 1, direction));
        }
    }
    cost
}

/// `BlockFlowing.getOptimalFlowDirections`: the directions with the
/// cheapest path to a drop. With no drop in reach every open side ties.
fn optimal_flow_directions(world: &TickWorld, position: IVec3, fluid: Fluid) -> [bool; 4] {
    let costs: [i32; 4] = std::array::from_fn(|direction| {
        let next = position + DIRECTIONS[direction];
        if !open_for_flow(world, next, fluid) {
            1000
        } else if !blocks_flow(world, next - IVec3::Y) {
            0
        } else {
            flow_cost(world, next, fluid, 1, direction)
        }
    });
    let cheapest = costs.into_iter().min().unwrap_or(1000);
    costs.map(|cost| cost == cheapest)
}

/// `BlockFlowing.getSmallestFlowDecay`: fold one neighbor into the smallest
/// decay seen so far, counting adjacent sources.
fn smallest_flow_decay(
    world: &TickWorld,
    position: IVec3,
    fluid: Fluid,
    smallest: i32,
    sources: &mut u32,
) -> i32 {
    let mut decay = flow_decay(world, position, fluid);
    if decay < 0 {
        return smallest;
    }
    if decay == 0 {
        *sources += 1;
    }
    if decay >= 8 {
        decay = 0;
    }
    if smallest >= 0 && decay >= smallest {
        smallest
    } else {
        decay
    }
}

/// `BlockFlowing.flowIntoBlock`: wash out whatever is in the way, or fizz
/// against it for lava, then place flowing fluid at `decay`.
fn flow_into(world: &mut TickWorld, position: IVec3, fluid: Fluid, decay: i32) {
    if !can_displace(world, position, fluid) {
        return;
    }
    let block = world.block(position);
    if block != Block::Air && fluid == Fluid::Water {
        let metadata = world.metadata(position);
        world.drop_block_as_item(position, block, metadata);
    }
    world.set_block_and_metadata_notify(position, fluid.flowing(), decay as u8);
}

/// `BlockFlowing.updateFlow`: settle into the still block, keeping the decay.
fn settle(world: &mut TickWorld, position: IVec3, fluid: Fluid) {
    let metadata = world.metadata(position);
    world.set_block_and_metadata(position, fluid.still(), metadata);
}

/// Beta `BlockFlowing`: water and lava that are spreading.
pub struct Flowing;
pub static FLOWING: Flowing = Flowing;

impl BlockBehavior for Flowing {
    /// `BlockFluid` ticks on load, so a flowing block left without a
    /// scheduled tick still settles or spreads eventually.
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn tick_rate(&self, block: Block) -> u32 {
        Fluid::of(block).map_or(10, Fluid::tick_rate)
    }

    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        check_for_harden(world, position, block);
        if world.block(position) == block {
            world.schedule(position, block, self.tick_rate(block));
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let block = world.block(position);
        check_for_harden(world, position, block);
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        let Some(fluid) = Fluid::of(block) else {
            return;
        };
        let step = fluid.decay_step();
        let mut decay = flow_decay(world, position, fluid);
        let mut settles = true;
        if decay > 0 {
            let mut sources = 0;
            let mut smallest = -100;
            for offset in DIRECTIONS {
                smallest =
                    smallest_flow_decay(world, position + offset, fluid, smallest, &mut sources);
            }
            let mut next = smallest + step;
            if next >= 8 || smallest < 0 {
                next = -1;
            }
            let above = flow_decay(world, position + IVec3::Y, fluid);
            if above >= 0 {
                next = if above >= 8 { above } else { above + 8 };
            }
            // Two sources beside water on a floor, or over more water, make
            // a new source.
            if sources >= 2 && fluid == Fluid::Water {
                let below = position - IVec3::Y;
                if world.is_solid(below)
                    || (Fluid::of(world.block(below)) == Some(fluid)
                        && world.metadata(position) == 0)
                {
                    next = 0;
                }
            }
            if fluid == Fluid::Lava
                && decay < 8
                && next < 8
                && next > decay
                && world.random().next_int(4) != 0
            {
                next = decay;
                settles = false;
            }
            if next != decay {
                decay = next;
                if next < 0 {
                    world.set_block_notify(position, Block::Air);
                } else {
                    world.set_metadata_notify(position, next as u8);
                    world.schedule(position, block, fluid.tick_rate());
                    world.notify_neighbors(position, block);
                }
            } else if settles {
                settle(world, position, fluid);
            }
        } else {
            settle(world, position, fluid);
        }

        let below = position - IVec3::Y;
        if can_displace(world, below, fluid) {
            let falling = if decay >= 8 { decay } else { decay + 8 };
            world.set_block_and_metadata_notify(below, block, falling as u8);
        } else if decay >= 0 && (decay == 0 || blocks_flow(world, below)) {
            let directions = optimal_flow_directions(world, position, fluid);
            let spread = if decay >= 8 { 1 } else { decay + step };
            if spread >= 8 {
                return;
            }
            for (direction, flows) in directions.into_iter().enumerate() {
                if flows {
                    flow_into(world, position + DIRECTIONS[direction], fluid, spread);
                }
            }
        }
    }
}

/// Beta `BlockStationary`: settled water and lava.
pub struct Stationary;
pub static STATIONARY: Stationary = Stationary;

impl BlockBehavior for Stationary {
    /// Only still lava ticks on load, to set fire to its surroundings.
    fn ticks_randomly(&self, block: Block) -> bool {
        block == Block::Lava
    }

    fn tick_rate(&self, block: Block) -> u32 {
        Fluid::of(block).map_or(10, Fluid::tick_rate)
    }

    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        let block = world.block(position);
        check_for_harden(world, position, block);
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let block = world.block(position);
        check_for_harden(world, position, block);
        if world.block(position) != block {
            return;
        }
        let Some(fluid) = Fluid::of(block) else {
            return;
        };
        // `setNotStationary`: become the flowing block without waking the
        // neighbors, then let the scheduled tick decide what to do.
        let metadata = world.metadata(position);
        world.set_editing(true);
        world.set_block_and_metadata(position, fluid.flowing(), metadata);
        world.schedule(position, fluid.flowing(), fluid.tick_rate());
        world.set_editing(false);
    }

    /// Still lava's random tick walks up to three cells upward and sets fire
    /// to air beside anything flammable. Fire is not in the world yet, so the
    /// walk runs (and draws its random numbers) but places nothing.
    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block(position) != Block::Lava {
            return;
        }
        let steps = world.random().next_int(3);
        let mut cell = position;
        for _ in 0..steps {
            cell.x += world.random().next_int(3) as i32 - 1;
            cell.y += 1;
            cell.z += world.random().next_int(3) as i32 - 1;
            let block = world.block(cell);
            if block == Block::Air {
                let flammable = crate::world::block_ticks::NEIGHBORS
                    .into_iter()
                    .any(|offset| is_flammable(world.block(cell + offset)));
                if flammable {
                    world.set_block_notify(cell, Block::Fire);
                    return;
                }
            } else if block.is_solid_material() {
                return;
            }
        }
    }
}

/// Beta `Material.getBurning`: wood, leaves, wool, and TNT.
pub fn is_flammable(block: Block) -> bool {
    crate::world::furnace::is_wood_material(block)
        || block.is_chest()
        || matches!(block, Block::Leaves | Block::Wool | Block::Tnt)
}

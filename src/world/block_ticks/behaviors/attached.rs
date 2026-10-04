//! Blocks that hang on a neighbor: Beta's `BlockTorch` and `BlockLadder`.
//! Each breaks off as an item when the block it rests against goes.
//!
//! The attachment is block metadata, as in Beta; see
//! [`Block::facing`](crate::block::blocks::Block::facing).

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

/// `BlockTorch.canPlaceTorchOn`: a normal cube or a fence.
fn torch_rests_on(world: &TickWorld, position: IVec3) -> bool {
    world.is_normal_cube(position) || world.block(position) == Block::Fence
}

/// The cell a torch with `metadata` leans against: a wall, or the floor.
pub fn torch_support(metadata: u8) -> IVec3 {
    Block::Torch
        .support_offset(metadata)
        .map_or(IVec3::NEG_Y, |[x, y, z]| IVec3::new(x, y, z))
}

/// `BlockTorch.canPlaceBlockAt`: any wall or the floor would hold a torch.
fn torch_can_stay_anywhere(world: &TickWorld, position: IVec3) -> bool {
    [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
        .into_iter()
        .any(|offset| world.is_normal_cube(position + offset))
        || torch_rests_on(world, position - IVec3::Y)
}

fn break_off(world: &mut TickWorld, position: IVec3) {
    let block = world.block(position);
    let metadata = world.metadata(position);
    world.drop_block_as_item(position, block, metadata);
    world.set_block_notify(position, Block::Air);
}

pub struct Torch;
pub static TORCH: Torch = Torch;

impl Torch {
    /// `dropTorchIfCantStay`, then the check for the torch's own support.
    fn check_support(world: &mut TickWorld, position: IVec3) {
        if !torch_can_stay_anywhere(world, position) {
            break_off(world, position);
            return;
        }
        let offset = torch_support(world.metadata(position));
        let support = position + offset;
        let holds = if offset == IVec3::NEG_Y {
            torch_rests_on(world, support)
        } else {
            world.is_normal_cube(support)
        };
        if !holds {
            break_off(world, position);
        }
    }
}

impl BlockBehavior for Torch {
    /// `BlockTorch.onBlockAdded` picks a facing and drops a torch with no
    /// support. Placement already chose the facing.
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        if !torch_can_stay_anywhere(world, position) {
            break_off(world, position);
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        Self::check_support(world, position);
    }
}

pub struct Ladder;
pub static LADDER: Ladder = Ladder;

impl BlockBehavior for Ladder {
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let holds = match Block::Ladder.support_offset(world.metadata(position)) {
            Some([x, y, z]) => world.is_normal_cube(position + IVec3::new(x, y, z)),
            // An unoriented ladder from an old save rests on any wall.
            None => [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
                .into_iter()
                .any(|offset| world.is_normal_cube(position + offset)),
        };
        if !holds {
            break_off(world, position);
        }
    }
}

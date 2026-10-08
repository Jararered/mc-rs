//! Beta's `BlockBed`: a half that loses its partner goes with it. Sleeping
//! and the Nether explosion need the player and live in `player::sleep`.

use bevy::math::IVec3;

use crate::block::bed;
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Bed;
pub static BED: Bed = Bed;

impl BlockBehavior for Bed {
    /// `BlockBed.onNeighborBlockChange`. Only the half without the foot bit
    /// drops the item, so a broken bed leaves one.
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let metadata = world.metadata(position);
        let step = bed::head_to_foot(metadata);
        if bed::is_foot(metadata) {
            if world.block(position - step) != Block::Bed {
                world.set_block_notify(position, Block::Air);
            }
        } else if world.block(position + step) != Block::Bed {
            world.set_block_notify(position, Block::Air);
            world.drop_block_as_item(position, Block::Bed, metadata);
        }
    }
}

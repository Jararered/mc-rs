//! Beta `BlockCake`: six slices on a solid block. Eating is a player action
//! (`eat_cake` in `player::interaction::editing`); this is the support check.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Cake;
pub static CAKE: Cake = Cake;

impl BlockBehavior for Cake {
    /// `BlockCake.onNeighborBlockChange`. `quantityDropped` is 0, so the
    /// requested drop is empty.
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        if !world.is_solid(position - IVec3::Y) {
            let metadata = world.metadata(position);
            world.drop_block_as_item(position, Block::Cake, metadata);
            world.set_block_notify(position, Block::Air);
        }
    }
}

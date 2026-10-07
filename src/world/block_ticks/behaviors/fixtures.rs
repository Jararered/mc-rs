//! Wooden fixtures with state: Beta's `BlockDoor`, `BlockTrapDoor`, and the
//! stacking half of `BlockStep`. Redstone is not simulated, so nothing here
//! reacts to power and an iron door never opens.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

/// Metadata bit of an open door or trapdoor.
pub const OPEN: u8 = 4;
/// Metadata bit of a door's upper half.
pub const UPPER: u8 = 8;

pub struct Door;
pub static DOOR: Door = Door;

impl Door {
    /// `BlockDoor.blockActivated`: swing both halves. The lower half holds
    /// the state and the upper half copies it.
    fn toggle(world: &mut TickWorld, position: IVec3) {
        let door = world.block(position);
        if door != Block::WoodenDoor {
            return;
        }
        let metadata = world.metadata(position);
        if metadata & UPPER != 0 {
            if world.block(position - IVec3::Y) == door {
                Self::toggle(world, position - IVec3::Y);
            }
            return;
        }
        if world.block(position + IVec3::Y) == door {
            world.set_metadata_notify(position + IVec3::Y, (metadata ^ OPEN) + UPPER);
        }
        world.set_metadata_notify(position, metadata ^ OPEN);
    }
}

impl BlockBehavior for Door {
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        Self::toggle(world, position);
    }

    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        Self::toggle(world, position);
    }

    /// `BlockDoor.onNeighborBlockChange`: a half without its other half
    /// goes, and so does a door whose floor went. Only the lower half drops
    /// the item.
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let door = world.block(position);
        let metadata = world.metadata(position);
        let (below, above) = (position - IVec3::Y, position + IVec3::Y);
        if metadata & UPPER != 0 {
            if world.block(below) != door {
                world.set_block_notify(position, Block::Air);
            }
            return;
        }
        let mut removed = false;
        if world.block(above) != door {
            world.set_block_notify(position, Block::Air);
            removed = true;
        }
        if !world.is_normal_cube(below) {
            world.set_block_notify(position, Block::Air);
            removed = true;
            if world.block(above) == door {
                world.set_block_notify(above, Block::Air);
            }
        }
        if removed {
            world.drop_block_as_item(position, door, metadata);
        }
    }
}

pub struct Trapdoor;
pub static TRAPDOOR: Trapdoor = Trapdoor;

impl Trapdoor {
    fn toggle(world: &mut TickWorld, position: IVec3) {
        let metadata = world.metadata(position);
        world.set_metadata_notify(position, metadata ^ OPEN);
    }

    /// The offset to the cube the trapdoor is hinged on.
    pub fn support(metadata: u8) -> IVec3 {
        match metadata & 3 {
            0 => IVec3::Z,
            1 => IVec3::NEG_Z,
            2 => IVec3::X,
            _ => IVec3::NEG_X,
        }
    }
}

impl BlockBehavior for Trapdoor {
    fn clicked(&self, world: &mut TickWorld, position: IVec3) {
        Self::toggle(world, position);
    }

    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        Self::toggle(world, position);
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let metadata = world.metadata(position);
        if !world.is_normal_cube(position + Self::support(metadata)) {
            world.set_block_notify(position, Block::Air);
            world.drop_block_as_item(position, Block::Trapdoor, metadata);
        }
    }
}

/// `BlockStep.onBlockAdded`: a slab set on a slab of the same material
/// becomes one double slab.
pub struct Slab;
pub static SLAB: Slab = Slab;

impl BlockBehavior for Slab {
    fn on_added(&self, world: &mut TickWorld, position: IVec3) {
        let below = position - IVec3::Y;
        let metadata = world.metadata(position);
        if world.block(below) == Block::StoneSlab && world.metadata(below) == metadata {
            world.set_block_notify(position, Block::Air);
            world.set_block_and_metadata_notify(below, Block::DoubleStoneSlab, metadata);
        }
    }
}

//! Fixtures with state: Beta's `BlockDoor`, `BlockTrapDoor`, and the stacking
//! half of `BlockStep`. Doors and trapdoors open and close with power, and
//! an iron door opens only that way.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use crate::world::block_ticks::behavior;

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

impl Door {
    /// `BlockDoor.onPoweredBlockChange`: swing the pair to match `open`.
    fn set_open(world: &mut TickWorld, position: IVec3, open: bool) {
        let door = world.block(position);
        let metadata = world.metadata(position);
        if (metadata & OPEN != 0) == open {
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
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Block) {
        let door = world.block(position);
        let metadata = world.metadata(position);
        let (below, above) = (position - IVec3::Y, position + IVec3::Y);
        if metadata & UPPER != 0 {
            if world.block(below) != door {
                world.set_block_notify(position, Block::Air);
            } else if neighbor != door {
                // The upper half passes the change down, as Beta does.
                self.neighbor_changed(world, below, neighbor);
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
        } else if behavior(neighbor).can_provide_power() {
            let powered = world.block_indirectly_getting_powered(position)
                || world.block_indirectly_getting_powered(above);
            Door::set_open(world, position, powered);
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

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, neighbor: Block) {
        let metadata = world.metadata(position);
        if !world.is_normal_cube(position + Self::support(metadata)) {
            world.set_block_notify(position, Block::Air);
            world.drop_block_as_item(position, Block::Trapdoor, metadata);
        } else if behavior(neighbor).can_provide_power() {
            // `BlockTrapDoor.onNeighborBlockChange`: open while powered.
            let powered = world.block_indirectly_getting_powered(position);
            if (metadata & OPEN != 0) != powered {
                world.set_metadata_notify(position, metadata ^ OPEN);
            }
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

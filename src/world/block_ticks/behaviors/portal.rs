//! Nether portals: Beta's `BlockPortal`.
//!
//! A portal is two columns of three portal blocks inside an obsidian frame
//! four wide and five tall, whose corners may be missing. Fire lit on the
//! frame's bottom edge fills it ([`try_to_create_portal`], called from the
//! fire block's `onBlockAdded`), and each portal block checks its own part of
//! the frame whenever a neighbor changes, so breaking any obsidian that
//! matters takes the whole portal with it, block by block.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

/// `BlockPortal.tryToCreatePortal`, for a fire at `position`. Returns whether
/// a portal was made.
pub fn try_to_create_portal(world: &mut TickWorld, mut position: IVec3) -> bool {
    let obsidian =
        |world: &TickWorld, offset: IVec3| world.block(position + offset) == Block::Obsidian;
    let along_x = i32::from(obsidian(world, IVec3::NEG_X) || obsidian(world, IVec3::X));
    let along_z = i32::from(obsidian(world, IVec3::NEG_Z) || obsidian(world, IVec3::Z));
    // Obsidian on both axes, or on neither, says nothing about which way the
    // frame faces.
    if along_x == along_z {
        return false;
    }
    let step = IVec3::new(along_x, 0, along_z);
    // The fire may be in either column; start from the lower one.
    if world.is_air(position - step) {
        position -= step;
    }
    for width in -1..=2 {
        for height in -1..=3 {
            let edge = width == -1 || width == 2 || height == -1 || height == 3;
            let corner = (width == -1 || width == 2) && (height == -1 || height == 3);
            if corner {
                continue;
            }
            let block = world.block(position + step * width + IVec3::Y * height);
            let fits = if edge {
                block == Block::Obsidian
            } else {
                matches!(block, Block::Air | Block::Fire)
            };
            if !fits {
                return false;
            }
        }
    }
    world.set_editing(true);
    for width in 0..2 {
        for height in 0..3 {
            world.set_block_notify(
                position + step * width + IVec3::Y * height,
                Block::NetherPortal,
            );
        }
    }
    world.set_editing(false);
    true
}

pub struct Portal;
pub static PORTAL: Portal = Portal;

impl BlockBehavior for Portal {
    /// `onNeighborBlockChange`: the column must stand on obsidian, be three
    /// high under obsidian, and have obsidian on one side and its twin
    /// column on the other.
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let portal = |world: &TickWorld, cell: IVec3| world.block(cell) == Block::NetherPortal;
        let across = if portal(world, position - IVec3::X) || portal(world, position + IVec3::X) {
            IVec3::X
        } else {
            IVec3::Z
        };
        let mut base = position;
        while portal(world, base - IVec3::Y) {
            base.y -= 1;
        }
        let mut whole = false;
        if world.block(base - IVec3::Y) == Block::Obsidian {
            let mut height = 1;
            while height < 4 && portal(world, base + IVec3::Y * height) {
                height += 1;
            }
            if height == 3 && world.block(base + IVec3::Y * height) == Block::Obsidian {
                let on_x = portal(world, position - IVec3::X) || portal(world, position + IVec3::X);
                let on_z = portal(world, position - IVec3::Z) || portal(world, position + IVec3::Z);
                let framed = |world: &TickWorld, frame: IVec3, twin: IVec3| {
                    world.block(frame) == Block::Obsidian && portal(world, twin)
                };
                whole = !(on_x && on_z)
                    && (framed(world, position + across, position - across)
                        || framed(world, position - across, position + across));
            }
        }
        if !whole {
            world.set_block_notify(position, Block::Air);
        }
    }
}

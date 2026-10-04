//! Leaf decay: Beta's `BlockLeaves` and the log removal that starts it in
//! `BlockLog`.
//!
//! Leaf metadata bit [`CHECK_DECAY`] asks the next random tick to look for a
//! log. Removing a log flags the leaves within four blocks, and removing a
//! leaf flags the leaves touching it. A flagged leaf that cannot reach a log
//! through at most four leaves decays. Generated leaves start unflagged;
//! player-placed ones start flagged, so they decay unless a log is near.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

/// Leaf metadata bit: check for a supporting log on the next random tick.
pub const CHECK_DECAY: u8 = 8;
/// How far through leaves a log supports them.
const SUPPORT_REACH: i32 = 4;
const SPAN: usize = (SUPPORT_REACH * 2 + 1) as usize;

/// Set [`CHECK_DECAY`] on every leaf within `radius` of `position`, without
/// notifying anything, as `onBlockRemoval` does.
fn flag_leaves(world: &mut TickWorld, position: IVec3, radius: i32) {
    if !world.area_loaded(position, radius + 1) {
        return;
    }
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            for dz in -radius..=radius {
                let cell = position + IVec3::new(dx, dy, dz);
                if world.block(cell).is_leaves() {
                    let metadata = world.metadata(cell);
                    if metadata & CHECK_DECAY == 0 {
                        world.set_metadata(cell, metadata | CHECK_DECAY);
                    }
                }
            }
        }
    }
}

/// `BlockLeaves.updateTick`'s search: whether a log reaches `position`
/// through at most four steps of leaves.
pub fn supported(world: &TickWorld, position: IVec3) -> bool {
    let index = |x: i32, y: i32, z: i32| {
        ((x + SUPPORT_REACH) as usize * SPAN + (y + SUPPORT_REACH) as usize) * SPAN
            + (z + SUPPORT_REACH) as usize
    };
    // Distance from a log: 0 for logs, -2 for unreached leaves, -1 otherwise.
    let mut distance = [-1i8; SPAN * SPAN * SPAN];
    for x in -SUPPORT_REACH..=SUPPORT_REACH {
        for y in -SUPPORT_REACH..=SUPPORT_REACH {
            for z in -SUPPORT_REACH..=SUPPORT_REACH {
                let block = world.block(position + IVec3::new(x, y, z));
                distance[index(x, y, z)] = if block == Block::Wood {
                    0
                } else if block.is_leaves() {
                    -2
                } else {
                    -1
                };
            }
        }
    }
    for step in 1..=SUPPORT_REACH as i8 {
        for x in -SUPPORT_REACH..=SUPPORT_REACH {
            for y in -SUPPORT_REACH..=SUPPORT_REACH {
                for z in -SUPPORT_REACH..=SUPPORT_REACH {
                    if distance[index(x, y, z)] != step - 1 {
                        continue;
                    }
                    for offset in crate::world::block_ticks::NEIGHBORS {
                        let (nx, ny, nz) = (x + offset.x, y + offset.y, z + offset.z);
                        let inside = [nx, ny, nz]
                            .iter()
                            .all(|axis| (-SUPPORT_REACH..=SUPPORT_REACH).contains(axis));
                        if inside && distance[index(nx, ny, nz)] == -2 {
                            distance[index(nx, ny, nz)] = step;
                        }
                    }
                }
            }
        }
    }
    distance[index(0, 0, 0)] >= 0
}

pub struct Leaves;
pub static LEAVES: Leaves = Leaves;

impl BlockBehavior for Leaves {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _previous: Block, _metadata: u8) {
        flag_leaves(world, position, 1);
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let metadata = world.metadata(position);
        if metadata & CHECK_DECAY == 0 || !world.area_loaded(position, SUPPORT_REACH + 1) {
            return;
        }
        if supported(world, position) {
            world.set_metadata(position, metadata & !CHECK_DECAY);
        } else {
            let block = world.block(position);
            world.drop_block_as_item(position, block, metadata);
            world.set_block_notify(position, Block::Air);
        }
    }
}

/// Beta `BlockLog`: removing a log asks nearby leaves to check their support.
pub struct Log;
pub static LOG: Log = Log;

impl BlockBehavior for Log {
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _previous: Block, _metadata: u8) {
        flag_leaves(world, position, SUPPORT_REACH);
    }
}

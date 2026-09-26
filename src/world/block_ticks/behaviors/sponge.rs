//! Beta 1.7.3's `BlockSponge` no longer absorbs water. Removing one still
//! notifies every block within two cells, which wakes the water it once held
//! back.

use bevy::math::IVec3;

use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

pub struct Sponge;
pub static SPONGE: Sponge = Sponge;

impl BlockBehavior for Sponge {
    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _previous: Id, _metadata: u8) {
        for dx in -2..=2 {
            for dy in -2..=2 {
                for dz in -2..=2 {
                    let cell = position + IVec3::new(dx, dy, dz);
                    let block = world.block(cell);
                    world.notify_neighbors(cell, block);
                }
            }
        }
    }
}

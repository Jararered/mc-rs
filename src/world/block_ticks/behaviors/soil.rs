//! Grass and farmland: Beta's `BlockGrass` and `BlockFarmland`.

use bevy::math::IVec3;

use crate::block::definition::light_opacity;
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

use super::fluid::is_water;

/// Beta `BlockGrass`. Random ticks turn grass under a dark, light-blocking
/// block back into dirt, and let well-lit grass spread onto nearby dirt.
pub struct Grass;
pub static GRASS: Grass = Grass;

impl BlockBehavior for Grass {
    fn ticks_randomly(&self, _block: Id) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        let above = position + IVec3::Y;
        let light = world.light(above);
        if light < 4 && light_opacity(world.block(above)) > 2 {
            if world.random().next_int(4) != 0 {
                return;
            }
            world.set_block_notify(position, Id::Dirt);
        } else if light >= 9 {
            let target = IVec3::new(
                position.x + world.random().next_int(3) as i32 - 1,
                position.y + world.random().next_int(5) as i32 - 3,
                position.z + world.random().next_int(3) as i32 - 1,
            );
            let over = target + IVec3::Y;
            if world.block(target) == Id::Dirt
                && world.light(over) >= 4
                && light_opacity(world.block(over)) <= 2
            {
                world.set_block_notify(target, Id::Grass);
            }
        }
    }
}

/// Farmland metadata is its moisture: `7` beside water, counting down to `0`
/// once the water is gone.
pub const MAX_MOISTURE: u8 = 7;

/// Beta `BlockFarmland`. Random ticks keep it wet within four blocks of
/// water, dry it out otherwise, and turn dry farmland with no crop back into
/// dirt. Trampling or covering it with a solid block also reverts it.
pub struct Farmland;
pub static FARMLAND: Farmland = Farmland;

impl Farmland {
    /// `BlockFarmland.isWaterNearby`: water within four blocks across, on the
    /// farmland's level or one above.
    pub fn water_nearby(world: &TickWorld, position: IVec3) -> bool {
        (-4..=4).any(|dx| {
            (0..=1).any(|dy| {
                (-4..=4).any(|dz| is_water(world.block(position + IVec3::new(dx, dy, dz))))
            })
        })
    }
}

impl BlockBehavior for Farmland {
    fn ticks_randomly(&self, _block: Id) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.random().next_int(5) != 0 {
            return;
        }
        if Self::water_nearby(world, position) || world.rained_on(position + IVec3::Y) {
            world.set_metadata_notify(position, MAX_MOISTURE);
            return;
        }
        let moisture = world.metadata(position);
        if moisture > 0 {
            world.set_metadata_notify(position, moisture - 1);
        } else if world.block(position + IVec3::Y) != Id::Crops {
            world.set_block_notify(position, Id::Dirt);
        }
    }

    fn entity_walked(&self, world: &mut TickWorld, position: IVec3) {
        if world.random().next_int(4) == 0 {
            world.set_block_notify(position, Id::Dirt);
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Id) {
        if world.is_solid(position + IVec3::Y) {
            world.set_block_notify(position, Id::Dirt);
        }
    }
}

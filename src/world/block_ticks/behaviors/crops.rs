//! Wheat: Beta's `BlockCrops`.
//!
//! Metadata is the growth stage, `0` for fresh seeds through `7` for ripe
//! wheat. Random ticks advance it while the crop is lit, faster on wet
//! farmland and slower when crops crowd each other.

use bevy::math::IVec3;

use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

use super::plants::check_flower_change;

/// The ripe growth stage.
pub const RIPE: u8 = 7;

pub struct Crops;
pub static CROPS: Crops = Crops;

impl Crops {
    /// `BlockCrops.getGrowthRate`: `1` plus the farmland under and around the
    /// crop (wet farmland counts triple, neighbors a quarter), halved when
    /// crops sit diagonally or line both axes.
    pub fn growth_rate(world: &TickWorld, position: IVec3) -> f32 {
        let crop = |dx: i32, dz: i32| world.block(position + IVec3::new(dx, 0, dz)) == Id::Crops;
        let along_x = crop(-1, 0) || crop(1, 0);
        let along_z = crop(0, -1) || crop(0, 1);
        let diagonal = crop(-1, -1) || crop(1, -1) || crop(1, 1) || crop(-1, 1);
        let mut rate = 1.0;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let soil = position + IVec3::new(dx, -1, dz);
                let mut value = 0.0;
                if world.block(soil) == Id::Farmland {
                    value = if world.metadata(soil) > 0 { 3.0 } else { 1.0 };
                }
                if dx != 0 || dz != 0 {
                    value /= 4.0;
                }
                rate += value;
            }
        }
        if diagonal || (along_x && along_z) {
            rate /= 2.0;
        }
        rate
    }
}

impl BlockBehavior for Crops {
    fn ticks_randomly(&self, _block: Id) -> bool {
        true
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Id) {
        check_flower_change(world, position);
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        check_flower_change(world, position);
        if world.block(position) != Id::Crops || world.light(position + IVec3::Y) < 9 {
            return;
        }
        let stage = world.metadata(position);
        if stage >= RIPE {
            return;
        }
        let rate = Self::growth_rate(world, position);
        let bound = (100.0 / rate) as u32;
        if world.random().next_int(bound.max(1)) == 0 {
            world.set_metadata_notify(position, stage + 1);
        }
    }
}

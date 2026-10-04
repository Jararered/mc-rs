//! Ice and snow: Beta's `BlockIce`, `BlockSnow` (the thin layer), and
//! `BlockSnowBlock`. All three melt in strong block light; sunlight never
//! melts them.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::block::definition::light_opacity;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::is_solid_material;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

use super::fluid::is_liquid;

/// Snow and snow layers melt above this block light.
const MELT_LIGHT: u8 = 11;

/// Pop the block's natural drops and replace it with `with`.
fn melt(world: &mut TickWorld, position: IVec3, with: Block) {
    let block = world.block(position);
    let metadata = world.metadata(position);
    world.drop_block_as_item(position, block, metadata);
    world.set_block_notify(position, with);
}

/// Beta `BlockIce`. Melts into still water in block light above 8, and a
/// harvested block leaves flowing water when it rested on something solid
/// or liquid.
pub struct Ice;
pub static ICE: Ice = Ice;

impl BlockBehavior for Ice {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block_light(position) > MELT_LIGHT - light_opacity(Block::Ice) {
            melt(world, position, Block::Water);
        }
    }

    fn harvested(&self, world: &mut TickWorld, position: IVec3, _block: Block, _metadata: u8) {
        let below = world.block(position - IVec3::Y);
        if is_solid_material(below) || is_liquid(below) {
            world.set_block_notify(position, Block::FlowingWater);
        }
    }
}

/// Beta `BlockSnow`: the thin snow layer. Melts in block light above 11, and
/// breaks off when the block under it is not a solid opaque cube.
pub struct SnowLayer;
pub static SNOW_LAYER: SnowLayer = SnowLayer;

impl SnowLayer {
    /// `BlockSnow.canPlaceBlockAt`.
    pub fn can_stay(world: &TickWorld, position: IVec3) -> bool {
        let below = world.block(position - IVec3::Y);
        below != Block::Air && is_opaque_cube(below) && is_solid_material(below)
    }
}

impl BlockBehavior for SnowLayer {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block_light(position) > MELT_LIGHT {
            melt(world, position, Block::Air);
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        if !Self::can_stay(world, position) {
            melt(world, position, Block::Air);
        }
    }
}

/// Beta `BlockSnowBlock`: melts in block light above 11, dropping its four
/// snowballs.
pub struct SnowBlock;
pub static SNOW_BLOCK: SnowBlock = SnowBlock;

impl BlockBehavior for SnowBlock {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.block_light(position) > MELT_LIGHT {
            melt(world, position, Block::Air);
        }
    }
}

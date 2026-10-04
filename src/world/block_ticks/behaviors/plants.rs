//! Plants that need the right ground and light: Beta's `BlockFlower` family
//! (flowers, tall grass, dead bushes, mushrooms), `BlockCactus`, and
//! `BlockReed`.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::block::properties::cactus_can_stay;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::plant_grows_on;
use crate::block::properties::sugar_cane_can_stay;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;

use super::fluid::is_water;

/// `canThisPlantGrowOnThisBlockID` for each `BlockFlower` subclass.
fn grows_on(plant: Block, ground: Block) -> bool {
    match plant {
        Block::DeadBush => ground == Block::Sand,
        Block::Crops => ground == Block::Farmland,
        Block::BrownMushroom | Block::RedMushroom => is_opaque_cube(ground),
        _ => plant_grows_on(ground),
    }
}

/// `BlockFlower.canBlockStay`, and `BlockMushroom`'s override: mushrooms need
/// shade, everything else light or open sky.
pub fn plant_can_stay(world: &mut TickWorld, position: IVec3, plant: Block) -> bool {
    let ground = world.block(position - IVec3::Y);
    if matches!(plant, Block::BrownMushroom | Block::RedMushroom) {
        return world.is_loaded(position)
            && world.full_light(position) < 13
            && grows_on(plant, ground);
    }
    (world.full_light(position) >= 8 || world.sees_sky(position)) && grows_on(plant, ground)
}

/// `BlockFlower.checkFlowerChange`: pop the plant off if it can no longer
/// stay.
pub fn check_flower_change(world: &mut TickWorld, position: IVec3) {
    let plant = world.block(position);
    if !plant_can_stay(world, position, plant) {
        let metadata = world.metadata(position);
        world.drop_block_as_item(position, plant, metadata);
        world.set_block_notify(position, Block::Air);
    }
}

/// Beta `BlockFlower`: dandelions, roses, tall grass, ferns, and dead bushes.
pub struct Flower;
pub static FLOWER: Flower = Flower;

impl BlockBehavior for Flower {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        check_flower_change(world, position);
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        check_flower_change(world, position);
    }
}

/// Beta `BlockMushroom`. A random tick has a 1 in 100 chance to spread a
/// mushroom into a nearby shaded cell on solid ground.
pub struct Mushroom;
pub static MUSHROOM: Mushroom = Mushroom;

impl BlockBehavior for Mushroom {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        check_flower_change(world, position);
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        if world.random().next_int(100) != 0 {
            return;
        }
        let mushroom = world.block(position);
        let random = world.random();
        let x = position.x + random.next_int(3) as i32 - 1;
        let y = position.y + random.next_int(2) as i32 - random.next_int(2) as i32;
        let z = position.z + random.next_int(3) as i32 - 1;
        let target = IVec3::new(x, y, z);
        if !world.is_air(target) || !plant_can_stay(world, target, mushroom) {
            return;
        }
        // Beta rolls a second offset and then ignores it.
        let random = world.random();
        random.next_int(3);
        random.next_int(3);
        if world.is_air(target) && plant_can_stay(world, target, mushroom) {
            world.set_block_notify(target, mushroom);
        }
    }
}

/// Stacks of cactus or sugar cane grow to three blocks. Metadata counts the
/// top block's random ticks; at 15 it grows a new block above.
fn grow_stack(world: &mut TickWorld, position: IVec3) {
    let block = world.block(position);
    let above = position + IVec3::Y;
    if !world.is_air(above) {
        return;
    }
    let mut height = 1;
    while world.block(position - IVec3::Y * height) == block {
        height += 1;
    }
    if height >= 3 {
        return;
    }
    let age = world.metadata(position);
    if age == 15 {
        world.set_block_notify(above, block);
        world.set_metadata_notify(position, 0);
    } else {
        world.set_metadata_notify(position, age + 1);
    }
}

/// Pop `position`'s block off as an item and clear the cell.
fn break_off(world: &mut TickWorld, position: IVec3) {
    let block = world.block(position);
    let metadata = world.metadata(position);
    world.drop_block_as_item(position, block, metadata);
    world.set_block_notify(position, Block::Air);
}

/// Beta `BlockCactus`: grows, and breaks when a solid block touches its side
/// or its sand or cactus support goes.
pub struct Cactus;
pub static CACTUS: Cactus = Cactus;

impl BlockBehavior for Cactus {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        grow_stack(world, position);
    }

    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let sides = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
            .map(|offset| world.block(position + offset));
        if !cactus_can_stay(world.block(position - IVec3::Y), sides) {
            break_off(world, position);
        }
    }
}

/// Beta `BlockReed` (sugar cane): grows, and breaks when its support or the
/// water beside its ground goes.
pub struct Reed;
pub static REED: Reed = Reed;

impl BlockBehavior for Reed {
    fn ticks_randomly(&self, _block: Block) -> bool {
        true
    }

    fn update_tick(&self, world: &mut TickWorld, position: IVec3) {
        grow_stack(world, position);
    }

    /// `BlockReed.checkBlockCoordValid`, with the game's placement rule,
    /// which also accepts sand.
    fn neighbor_changed(&self, world: &mut TickWorld, position: IVec3, _neighbor: Block) {
        let ground = position - IVec3::Y;
        let water = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
            .map(|offset| is_water(world.block(ground + offset)));
        if !sugar_cane_can_stay(world.block(ground), water) {
            break_off(world, position);
        }
    }
}

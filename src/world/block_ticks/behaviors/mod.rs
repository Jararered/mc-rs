//! Block update behavior, one module per Beta block family, and the table
//! that assigns each compact block value its implementation.
//!
//! To give a block update behavior, implement
//! [`BlockBehavior`](super::BlockBehavior) for a unit struct in the module for
//! its family (or a new one), and list the block's values with a reference to
//! the struct in [`table`]. `&Foo` is a constant, so it is already `'static`.
//! See `docs/BLOCK_TICKS.md`.

use crate::block::blocks::Block;

use super::behavior::BlockBehavior;
use super::behavior::inert_table;

pub mod attached;
pub mod bed;
pub mod cake;
pub mod controls;
pub mod crops;
pub mod dispenser;
pub mod falling;
pub mod fire;
pub mod fixtures;
pub mod fluid;
pub mod jukebox;
pub mod leaves;
pub mod note;
pub mod ore;
pub mod piston;
pub mod plants;
pub mod portal;
pub mod rail;
pub mod redstone;
pub mod snow;
pub mod soil;
pub mod sponge;
pub mod tnt;

/// Register `behavior` for every block in `blocks`.
fn register(
    table: &mut [&'static dyn BlockBehavior; 256],
    blocks: &[Block],
    behavior: &'static dyn BlockBehavior,
) {
    for block in blocks {
        table[usize::from(block.as_u8())] = behavior;
    }
}

/// Every block's update behavior. Values not listed here are inert.
pub(super) fn table() -> [&'static dyn BlockBehavior; 256] {
    let mut table = inert_table();
    register(
        &mut table,
        &[Block::WaterFlowing, Block::LavaFlowing],
        &fluid::Flowing,
    );
    register(&mut table, &[Block::Water, Block::Lava], &fluid::Stationary);
    register(&mut table, &[Block::Sand, Block::Gravel], &falling::Falling);
    register(&mut table, &[Block::Grass], &soil::Grass);
    register(&mut table, &[Block::Fire], &fire::Fire);
    register(&mut table, &[Block::Farmland], &soil::Farmland);
    register(&mut table, &[Block::Crops], &crops::Crops);
    register(
        &mut table,
        &[
            Block::Dandelion,
            Block::Rose,
            Block::TallGrass,
            Block::DeadBush,
        ],
        &plants::Flower,
    );
    register(
        &mut table,
        &[Block::BrownMushroom, Block::RedMushroom],
        &plants::Mushroom,
    );
    register(&mut table, &[Block::Cactus], &plants::Cactus);
    register(&mut table, &[Block::SugarCane], &plants::Reed);
    register(&mut table, &[Block::Leaves], &leaves::Leaves);
    register(&mut table, &[Block::Wood], &leaves::Log);
    register(
        &mut table,
        &[Block::RedstoneOreInactive, Block::RedstoneOreActive],
        &ore::RedstoneOre,
    );
    register(&mut table, &[Block::Ice], &snow::Ice);
    register(&mut table, &[Block::SnowLayer], &snow::SnowLayer);
    register(&mut table, &[Block::Snow], &snow::SnowBlock);
    register(&mut table, &[Block::Torch], &attached::Torch);
    register(&mut table, &[Block::Ladder], &attached::Ladder);
    register(&mut table, &[Block::Sign, Block::WallSign], &attached::Sign);
    register(&mut table, &[Block::Sponge], &sponge::Sponge);
    register(&mut table, &[Block::Sapling], &plants::Sapling);
    register(
        &mut table,
        &[Block::WoodenDoor, Block::IronDoor],
        &fixtures::Door,
    );
    register(&mut table, &[Block::Trapdoor], &fixtures::Trapdoor);
    register(
        &mut table,
        &[Block::StoneSlab, Block::DoubleStoneSlab],
        &fixtures::Slab,
    );
    register(&mut table, &[Block::Bed], &bed::Bed);
    register(&mut table, &[Block::Cake], &cake::Cake);
    register(&mut table, &[Block::NetherPortal], &portal::Portal);
    register(&mut table, &[Block::Redstone], &redstone::Wire);
    register(
        &mut table,
        &[Block::RepeaterInactive, Block::RepeaterActive],
        &redstone::Repeater,
    );
    register(
        &mut table,
        &[Block::RedstoneTorchActive, Block::RedstoneTorchInactive],
        &redstone::RedstoneTorch,
    );
    register(&mut table, &[Block::Lever], &controls::Lever);
    register(&mut table, &[Block::StoneButton], &controls::Button);
    register(
        &mut table,
        &[Block::StonePressurePlate, Block::WoodenPressurePlate],
        &controls::Plate,
    );
    register(
        &mut table,
        &[Block::Piston, Block::StickyPiston],
        &piston::Piston,
    );
    register(&mut table, &[Block::PistonHead], &piston::PistonHead);
    register(&mut table, &[Block::Rail, Block::PoweredRail], &rail::Rail);
    register(&mut table, &[Block::DetectorRail], &rail::Detector);
    register(&mut table, &[Block::NoteBlock], &note::Note);
    register(&mut table, &[Block::Jukebox], &jukebox::Jukebox);
    register(&mut table, &[Block::Tnt], &tnt::Tnt);
    register(&mut table, &[Block::Dispenser], &dispenser::Dispenser);
    table
}

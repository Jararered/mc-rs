//! Block update behavior, one module per Beta block family, and the table
//! that assigns each compact block value its implementation.
//!
//! To give a block update behavior, implement
//! [`BlockBehavior`](super::BlockBehavior) for a unit struct in the module for
//! its family (or a new one), declare a `static` of it, and list the block's
//! values in [`table`]. See `docs/BLOCK_TICKS.md`.

use crate::block::blocks::Block;

use super::behavior::BlockBehavior;
use super::behavior::inert_table;

pub mod attached;
pub mod bed;
pub mod controls;
pub mod crops;
pub mod dispenser;
pub mod falling;
pub mod fire;
pub mod fixtures;
pub mod fluid;
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
        &[Block::FlowingWater, Block::FlowingLava],
        &fluid::FLOWING,
    );
    register(&mut table, &[Block::Water, Block::Lava], &fluid::STATIONARY);
    register(&mut table, &[Block::Sand, Block::Gravel], &falling::FALLING);
    register(&mut table, &[Block::Grass], &soil::GRASS);
    register(&mut table, &[Block::Fire], &fire::FIRE);
    register(&mut table, &[Block::Farmland], &soil::FARMLAND);
    register(&mut table, &[Block::Crops], &crops::CROPS);
    register(
        &mut table,
        &[
            Block::Dandelion,
            Block::Rose,
            Block::TallGrass,
            Block::DeadBush,
        ],
        &plants::FLOWER,
    );
    register(
        &mut table,
        &[Block::BrownMushroom, Block::RedMushroom],
        &plants::MUSHROOM,
    );
    register(&mut table, &[Block::Cactus], &plants::CACTUS);
    register(&mut table, &[Block::SugarCane], &plants::REED);
    register(&mut table, &[Block::Leaves], &leaves::LEAVES);
    register(&mut table, &[Block::Wood], &leaves::LOG);
    register(
        &mut table,
        &[Block::RedstoneOre, Block::LitRedstoneOre],
        &ore::REDSTONE_ORE,
    );
    register(&mut table, &[Block::Ice], &snow::ICE);
    register(&mut table, &[Block::SnowLayer], &snow::SNOW_LAYER);
    register(&mut table, &[Block::Snow], &snow::SNOW_BLOCK);
    register(&mut table, &[Block::Torch], &attached::TORCH);
    register(&mut table, &[Block::Ladder], &attached::LADDER);
    register(&mut table, &[Block::Sponge], &sponge::SPONGE);
    register(&mut table, &[Block::Sapling], &plants::SAPLING);
    register(
        &mut table,
        &[Block::WoodenDoor, Block::IronDoor],
        &fixtures::DOOR,
    );
    register(&mut table, &[Block::Trapdoor], &fixtures::TRAPDOOR);
    register(&mut table, &[Block::StoneSlab], &fixtures::SLAB);
    register(&mut table, &[Block::Bed], &bed::BED);
    register(&mut table, &[Block::NetherPortal], &portal::PORTAL);
    register(&mut table, &[Block::RedstoneWire], &redstone::WIRE);
    register(
        &mut table,
        &[Block::Repeater, Block::PoweredRepeater],
        &redstone::REPEATER,
    );
    register(
        &mut table,
        &[Block::RedstoneTorch, Block::UnlitRedstoneTorch],
        &redstone::REDSTONE_TORCH,
    );
    register(&mut table, &[Block::Lever], &controls::LEVER);
    register(&mut table, &[Block::StoneButton], &controls::BUTTON);
    register(
        &mut table,
        &[Block::StonePressurePlate, Block::WoodenPressurePlate],
        &controls::PLATE,
    );
    register(
        &mut table,
        &[Block::Piston, Block::StickyPiston],
        &piston::PISTON,
    );
    register(&mut table, &[Block::PistonHead], &piston::HEAD);
    register(&mut table, &[Block::Rail, Block::PoweredRail], &rail::RAIL);
    register(&mut table, &[Block::DetectorRail], &rail::DETECTOR);
    register(&mut table, &[Block::NoteBlock], &note::NOTE);
    register(&mut table, &[Block::Tnt], &tnt::TNT);
    register(&mut table, &[Block::Dispenser], &dispenser::DISPENSER);
    table
}

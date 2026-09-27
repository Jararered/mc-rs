//! Block update behavior, one module per Beta block family, and the table
//! that assigns each compact block value its implementation.
//!
//! To give a block update behavior, implement
//! [`BlockBehavior`](super::BlockBehavior) for a unit struct in the module for
//! its family (or a new one), declare a `static` of it, and list the block's
//! values in [`table`]. See `docs/BLOCK_TICKS.md`.

use crate::block::id::Id;

use super::behavior::BlockBehavior;
use super::behavior::inert_table;

pub mod attached;
pub mod controls;
pub mod crops;
pub mod dispenser;
pub mod doors;
pub mod falling;
pub mod fluid;
pub mod leaves;
pub mod note;
pub mod ore;
pub mod piston;
pub mod plants;
pub mod rail;
pub mod redstone;
pub mod snow;
pub mod soil;
pub mod sponge;
pub mod tnt;

/// Register `behavior` for every block in `blocks`.
fn register(
    table: &mut [&'static dyn BlockBehavior; 256],
    blocks: &[Id],
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
        &[Id::FlowingWater, Id::FlowingLava],
        &fluid::FLOWING,
    );
    register(&mut table, &[Id::Water, Id::Lava], &fluid::STATIONARY);
    register(&mut table, &[Id::Sand, Id::Gravel], &falling::FALLING);
    register(&mut table, &[Id::Grass], &soil::GRASS);
    register(&mut table, &[Id::Farmland], &soil::FARMLAND);
    register(&mut table, &[Id::Crops], &crops::CROPS);
    register(
        &mut table,
        &[
            Id::Dandelion,
            Id::Rose,
            Id::TallGrass,
            Id::Fern,
            Id::DeadBush,
        ],
        &plants::FLOWER,
    );
    register(
        &mut table,
        &[Id::BrownMushroom, Id::RedMushroom],
        &plants::MUSHROOM,
    );
    register(&mut table, &[Id::Cactus], &plants::CACTUS);
    register(&mut table, &[Id::SugarCane], &plants::REED);
    register(
        &mut table,
        &[Id::Leaves, Id::SpruceLeaves, Id::BirchLeaves],
        &leaves::LEAVES,
    );
    register(
        &mut table,
        &[Id::Wood, Id::SpruceWood, Id::BirchWood],
        &leaves::LOG,
    );
    register(
        &mut table,
        &[Id::RedstoneOre, Id::LitRedstoneOre],
        &ore::REDSTONE_ORE,
    );
    register(&mut table, &[Id::Ice], &snow::ICE);
    register(&mut table, &[Id::SnowLayer], &snow::SNOW_LAYER);
    register(&mut table, &[Id::Snow], &snow::SNOW_BLOCK);
    register(
        &mut table,
        &[
            Id::Torch,
            Id::TorchWest,
            Id::TorchEast,
            Id::TorchNorth,
            Id::TorchSouth,
        ],
        &attached::TORCH,
    );
    register(
        &mut table,
        &[
            Id::Ladder,
            Id::LadderNorth,
            Id::LadderEast,
            Id::LadderSouth,
            Id::LadderWest,
        ],
        &attached::LADDER,
    );
    register(&mut table, &[Id::Sponge], &sponge::SPONGE);
    register(&mut table, &[Id::RedstoneWire], &redstone::WIRE);
    register(
        &mut table,
        &[Id::Repeater, Id::PoweredRepeater],
        &redstone::REPEATER,
    );
    register(
        &mut table,
        &[Id::RedstoneTorch, Id::UnlitRedstoneTorch],
        &redstone::REDSTONE_TORCH,
    );
    register(&mut table, &[Id::Lever], &controls::LEVER);
    register(&mut table, &[Id::StoneButton], &controls::BUTTON);
    register(
        &mut table,
        &[Id::StonePressurePlate, Id::WoodenPressurePlate],
        &controls::PLATE,
    );
    register(&mut table, &[Id::WoodenDoor, Id::IronDoor], &doors::DOOR);
    register(&mut table, &[Id::Piston, Id::StickyPiston], &piston::PISTON);
    register(&mut table, &[Id::PistonHead], &piston::HEAD);
    register(&mut table, &[Id::Rail, Id::PoweredRail], &rail::RAIL);
    register(&mut table, &[Id::DetectorRail], &rail::DETECTOR);
    register(&mut table, &[Id::NoteBlock], &note::NOTE);
    register(&mut table, &[Id::Trapdoor], &doors::TRAPDOOR);
    register(&mut table, &[Id::Tnt], &tnt::TNT);
    register(&mut table, &[Id::Dispenser], &dispenser::DISPENSER);
    table
}

use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: Id) -> &'static str {
        match state {
            Id::Chest => "chest",
            Id::ChestNorth => "chest_north",
            Id::ChestEast => "chest_east",
            Id::ChestSouth => "chest_south",
            Id::ChestWest => "chest_west",
            Id::Ladder => "ladder",
            Id::LadderNorth => "ladder_north",
            Id::LadderEast => "ladder_east",
            Id::LadderSouth => "ladder_south",
            Id::LadderWest => "ladder_west",
            Id::Furnace => "furnace",
            Id::LitFurnace => "lit_furnace",
            Id::FurnaceNorth => "furnace_north",
            Id::FurnaceEast => "furnace_east",
            Id::FurnaceSouth => "furnace_south",
            Id::FurnaceWest => "furnace_west",
            Id::LitFurnaceNorth => "lit_furnace_north",
            Id::LitFurnaceEast => "lit_furnace_east",
            Id::LitFurnaceSouth => "lit_furnace_south",
            Id::LitFurnaceWest => "lit_furnace_west",
            Id::Pumpkin => "pumpkin",
            Id::PumpkinNorth => "pumpkin_north",
            Id::PumpkinEast => "pumpkin_east",
            Id::PumpkinSouth => "pumpkin_south",
            Id::PumpkinWest => "pumpkin_west",
            Id::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: Id) -> bool {
        matches!(
            state,
            Id::Chest
                | Id::ChestNorth
                | Id::ChestEast
                | Id::ChestSouth
                | Id::ChestWest
                | Id::Ladder
                | Id::LadderNorth
                | Id::LadderEast
                | Id::LadderSouth
                | Id::LadderWest
                | Id::Furnace
                | Id::LitFurnace
                | Id::FurnaceNorth
                | Id::FurnaceEast
                | Id::FurnaceSouth
                | Id::FurnaceWest
                | Id::LitFurnaceNorth
                | Id::LitFurnaceEast
                | Id::LitFurnaceSouth
                | Id::LitFurnaceWest
                | Id::Pumpkin
                | Id::PumpkinNorth
                | Id::PumpkinEast
                | Id::PumpkinSouth
                | Id::PumpkinWest
        )
    }

    fn properties(&self, state: Id) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: Id) -> bool {
        !matches!(
            state,
            Id::Chest
                | Id::ChestNorth
                | Id::ChestEast
                | Id::ChestSouth
                | Id::ChestWest
                | Id::Ladder
                | Id::LadderNorth
                | Id::LadderEast
                | Id::LadderSouth
                | Id::LadderWest
        )
    }

    fn light_opacity(&self, state: Id) -> u8 {
        if state.is_ladder() { 0 } else { 15 }
    }

    fn light_emission(&self, state: Id) -> u8 {
        if state.is_lit_furnace() { 13 } else { 0 }
    }
}

fn properties(state: Id) -> BlockProperties {
    match state {
        Id::Chest | Id::ChestNorth | Id::ChestEast | Id::ChestSouth | Id::ChestWest => {
            BlockProperties {
                opaque_cube: false,
                ..BlockProperties::solid(2.5)
            }
        }
        Id::Ladder | Id::LadderNorth | Id::LadderEast | Id::LadderSouth | Id::LadderWest => {
            let bounds = match state.ladder_support_offset() {
                Some([0, 0, -1]) => ([0.0, 0.0, 0.0], [1.0, 1.0, 0.125]),
                Some([0, 0, 1]) => ([0.0, 0.0, 0.875], [1.0, 1.0, 1.0]),
                Some([1, 0, 0]) => ([0.875, 0.0, 0.0], [1.0, 1.0, 1.0]),
                Some([-1, 0, 0]) => ([0.0, 0.0, 0.0], [0.125, 1.0, 1.0]),
                _ => BlockProperties::FULL_BOUNDS,
            };
            BlockProperties {
                opaque_cube: false,
                light_opacity: 0,
                collision_bounds: Some(bounds),
                selection_bounds: bounds,
                ..BlockProperties::solid(0.4)
            }
        }
        Id::Furnace
        | Id::LitFurnace
        | Id::FurnaceNorth
        | Id::FurnaceEast
        | Id::FurnaceSouth
        | Id::FurnaceWest
        | Id::LitFurnaceNorth
        | Id::LitFurnaceEast
        | Id::LitFurnaceSouth
        | Id::LitFurnaceWest => BlockProperties {
            harvestable_by_hand: false,
            light_emission: if state.is_lit_furnace() { 13 } else { 0 },
            ..BlockProperties::solid(3.5)
        },
        Id::Pumpkin | Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest => {
            BlockProperties::solid(1.0)
        }
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

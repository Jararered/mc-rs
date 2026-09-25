use crate::block::block::BlockId;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::Chest => "chest",
            BlockId::ChestNorth => "chest_north",
            BlockId::ChestEast => "chest_east",
            BlockId::ChestSouth => "chest_south",
            BlockId::ChestWest => "chest_west",
            BlockId::Ladder => "ladder",
            BlockId::LadderNorth => "ladder_north",
            BlockId::LadderEast => "ladder_east",
            BlockId::LadderSouth => "ladder_south",
            BlockId::LadderWest => "ladder_west",
            BlockId::Furnace => "furnace",
            BlockId::LitFurnace => "lit_furnace",
            BlockId::FurnaceNorth => "furnace_north",
            BlockId::FurnaceEast => "furnace_east",
            BlockId::FurnaceSouth => "furnace_south",
            BlockId::FurnaceWest => "furnace_west",
            BlockId::LitFurnaceNorth => "lit_furnace_north",
            BlockId::LitFurnaceEast => "lit_furnace_east",
            BlockId::LitFurnaceSouth => "lit_furnace_south",
            BlockId::LitFurnaceWest => "lit_furnace_west",
            BlockId::Pumpkin => "pumpkin",
            BlockId::PumpkinNorth => "pumpkin_north",
            BlockId::PumpkinEast => "pumpkin_east",
            BlockId::PumpkinSouth => "pumpkin_south",
            BlockId::PumpkinWest => "pumpkin_west",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::Chest
                | BlockId::ChestNorth
                | BlockId::ChestEast
                | BlockId::ChestSouth
                | BlockId::ChestWest
                | BlockId::Ladder
                | BlockId::LadderNorth
                | BlockId::LadderEast
                | BlockId::LadderSouth
                | BlockId::LadderWest
                | BlockId::Furnace
                | BlockId::LitFurnace
                | BlockId::FurnaceNorth
                | BlockId::FurnaceEast
                | BlockId::FurnaceSouth
                | BlockId::FurnaceWest
                | BlockId::LitFurnaceNorth
                | BlockId::LitFurnaceEast
                | BlockId::LitFurnaceSouth
                | BlockId::LitFurnaceWest
                | BlockId::Pumpkin
                | BlockId::PumpkinNorth
                | BlockId::PumpkinEast
                | BlockId::PumpkinSouth
                | BlockId::PumpkinWest
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: BlockId) -> bool {
        !matches!(
            state,
            BlockId::Chest
                | BlockId::ChestNorth
                | BlockId::ChestEast
                | BlockId::ChestSouth
                | BlockId::ChestWest
                | BlockId::Ladder
                | BlockId::LadderNorth
                | BlockId::LadderEast
                | BlockId::LadderSouth
                | BlockId::LadderWest
        )
    }

    fn light_opacity(&self, state: BlockId) -> u8 {
        if state.is_ladder() { 0 } else { 15 }
    }

    fn light_emission(&self, state: BlockId) -> u8 {
        if state.is_lit_furnace() { 13 } else { 0 }
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Chest
        | BlockId::ChestNorth
        | BlockId::ChestEast
        | BlockId::ChestSouth
        | BlockId::ChestWest => BlockProperties {
            opaque_cube: false,
            ..BlockProperties::solid(2.5)
        },
        BlockId::Ladder
        | BlockId::LadderNorth
        | BlockId::LadderEast
        | BlockId::LadderSouth
        | BlockId::LadderWest => {
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
        BlockId::Furnace
        | BlockId::LitFurnace
        | BlockId::FurnaceNorth
        | BlockId::FurnaceEast
        | BlockId::FurnaceSouth
        | BlockId::FurnaceWest
        | BlockId::LitFurnaceNorth
        | BlockId::LitFurnaceEast
        | BlockId::LitFurnaceSouth
        | BlockId::LitFurnaceWest => BlockProperties {
            harvestable_by_hand: false,
            light_emission: if state.is_lit_furnace() { 13 } else { 0 },
            ..BlockProperties::solid(3.5)
        },
        BlockId::Pumpkin
        | BlockId::PumpkinNorth
        | BlockId::PumpkinEast
        | BlockId::PumpkinSouth
        | BlockId::PumpkinWest => BlockProperties::solid(1.0),
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

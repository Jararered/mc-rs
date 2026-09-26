use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
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

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        !matches!(
            id,
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

    fn light_opacity(&self, id: Id) -> u8 {
        if id.is_ladder() { 0 } else { 15 }
    }

    fn light_emission(&self, id: Id) -> u8 {
        if id.is_lit_furnace() { 13 } else { 0 }
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::Chest | Id::ChestNorth | Id::ChestEast | Id::ChestSouth | Id::ChestWest => {
            BlockProperties {
                opaque_cube: false,
                ..BlockProperties::solid(2.5)
            }
        }
        Id::Ladder | Id::LadderNorth | Id::LadderEast | Id::LadderSouth | Id::LadderWest => {
            let bounds = match id.ladder_support_offset() {
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
            light_emission: if id.is_lit_furnace() { 13 } else { 0 },
            ..BlockProperties::solid(3.5)
        },
        Id::Pumpkin | Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest => {
            BlockProperties::solid(1.0)
        }
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

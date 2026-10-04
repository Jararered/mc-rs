use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(
            block,
            Block::Chest
                | Block::ChestNorth
                | Block::ChestEast
                | Block::ChestSouth
                | Block::ChestWest
                | Block::Ladder
                | Block::LadderNorth
                | Block::LadderEast
                | Block::LadderSouth
                | Block::LadderWest
                | Block::Furnace
                | Block::LitFurnace
                | Block::FurnaceNorth
                | Block::FurnaceEast
                | Block::FurnaceSouth
                | Block::FurnaceWest
                | Block::LitFurnaceNorth
                | Block::LitFurnaceEast
                | Block::LitFurnaceSouth
                | Block::LitFurnaceWest
                | Block::Pumpkin
                | Block::PumpkinNorth
                | Block::PumpkinEast
                | Block::PumpkinSouth
                | Block::PumpkinWest
        )
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }

    fn opaque_cube(&self, block: Block) -> bool {
        !matches!(
            block,
            Block::Chest
                | Block::ChestNorth
                | Block::ChestEast
                | Block::ChestSouth
                | Block::ChestWest
                | Block::Ladder
                | Block::LadderNorth
                | Block::LadderEast
                | Block::LadderSouth
                | Block::LadderWest
        )
    }

    fn light_opacity(&self, block: Block) -> u8 {
        if block.is_ladder() { 0 } else { 15 }
    }

    fn light_emission(&self, block: Block) -> u8 {
        if block.is_lit_furnace() { 13 } else { 0 }
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Chest
        | Block::ChestNorth
        | Block::ChestEast
        | Block::ChestSouth
        | Block::ChestWest => BlockProperties {
            opaque_cube: false,
            ..BlockProperties::solid(2.5)
        },
        Block::Ladder
        | Block::LadderNorth
        | Block::LadderEast
        | Block::LadderSouth
        | Block::LadderWest => {
            let bounds = match block.ladder_support_offset() {
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
        Block::Furnace
        | Block::LitFurnace
        | Block::FurnaceNorth
        | Block::FurnaceEast
        | Block::FurnaceSouth
        | Block::FurnaceWest
        | Block::LitFurnaceNorth
        | Block::LitFurnaceEast
        | Block::LitFurnaceSouth
        | Block::LitFurnaceWest => BlockProperties {
            harvestable_by_hand: false,
            light_emission: if block.is_lit_furnace() { 13 } else { 0 },
            ..BlockProperties::solid(3.5)
        },
        Block::Pumpkin
        | Block::PumpkinNorth
        | Block::PumpkinEast
        | Block::PumpkinSouth
        | Block::PumpkinWest => BlockProperties::solid(1.0),
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

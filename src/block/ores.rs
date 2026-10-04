use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(
            block,
            Block::GoldOre
                | Block::IronOre
                | Block::CoalOre
                | Block::LapisOre
                | Block::LapisBlock
                | Block::DiamondOre
                | Block::RedstoneOre
                | Block::LitRedstoneOre
        )
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }

    fn light_emission(&self, block: Block) -> u8 {
        if block == Block::LitRedstoneOre { 9 } else { 0 }
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::GoldOre
        | Block::IronOre
        | Block::CoalOre
        | Block::LapisOre
        | Block::LapisBlock
        | Block::DiamondOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Block::RedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Block::LitRedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 9,
            ..BlockProperties::solid(3.0)
        },
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

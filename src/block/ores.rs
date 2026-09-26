use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::BlockId;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::GoldOre => "gold_ore",
            BlockId::IronOre => "iron_ore",
            BlockId::CoalOre => "coal_ore",
            BlockId::LapisOre => "lapis_ore",
            BlockId::LapisBlock => "lapis_block",
            BlockId::DiamondOre => "diamond_ore",
            BlockId::RedstoneOre => "redstone_ore",
            BlockId::LitRedstoneOre => "lit_redstone_ore",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::GoldOre
                | BlockId::IronOre
                | BlockId::CoalOre
                | BlockId::LapisOre
                | BlockId::LapisBlock
                | BlockId::DiamondOre
                | BlockId::RedstoneOre
                | BlockId::LitRedstoneOre
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn light_emission(&self, state: BlockId) -> u8 {
        if state == BlockId::LitRedstoneOre {
            9
        } else {
            0
        }
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::GoldOre
        | BlockId::IronOre
        | BlockId::CoalOre
        | BlockId::LapisOre
        | BlockId::LapisBlock
        | BlockId::DiamondOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        BlockId::RedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        BlockId::LitRedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 9,
            ..BlockProperties::solid(3.0)
        },
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

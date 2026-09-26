use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::GoldOre
                | Id::IronOre
                | Id::CoalOre
                | Id::LapisOre
                | Id::LapisBlock
                | Id::DiamondOre
                | Id::RedstoneOre
                | Id::LitRedstoneOre
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn light_emission(&self, id: Id) -> u8 {
        if id == Id::LitRedstoneOre { 9 } else { 0 }
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::GoldOre
        | Id::IronOre
        | Id::CoalOre
        | Id::LapisOre
        | Id::LapisBlock
        | Id::DiamondOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Id::RedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Id::LitRedstoneOre => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 9,
            ..BlockProperties::solid(3.0)
        },
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

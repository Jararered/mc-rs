use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::BlockId;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::FlowingWater => "flowing_water",
            BlockId::Water => "water",
            BlockId::FlowingLava => "flowing_lava",
            BlockId::Lava => "lava",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::FlowingWater | BlockId::Water | BlockId::FlowingLava | BlockId::Lava
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, _state: BlockId) -> bool {
        false
    }

    fn light_opacity(&self, state: BlockId) -> u8 {
        if state == BlockId::Water { 3 } else { 15 }
    }

    fn light_emission(&self, state: BlockId) -> u8 {
        if matches!(state, BlockId::FlowingLava | BlockId::Lava) {
            15
        } else {
            0
        }
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::FlowingWater => BlockProperties {
            light_opacity: 15,
            ..BlockProperties::fluid(0.0)
        },
        BlockId::Water => BlockProperties {
            light_opacity: 3,
            ..BlockProperties::fluid(100.0)
        },
        BlockId::FlowingLava | BlockId::Lava => BlockProperties {
            light_opacity: 15,
            light_emission: 15,
            ..BlockProperties::fluid(0.0)
        },
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

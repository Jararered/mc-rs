use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, state: Id) -> bool {
        matches!(
            state,
            Id::FlowingWater | Id::Water | Id::FlowingLava | Id::Lava
        )
    }

    fn properties(&self, state: Id) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, _state: Id) -> bool {
        false
    }

    fn light_opacity(&self, state: Id) -> u8 {
        if state == Id::Water { 3 } else { 15 }
    }

    fn light_emission(&self, state: Id) -> u8 {
        if matches!(state, Id::FlowingLava | Id::Lava) {
            15
        } else {
            0
        }
    }
}

fn properties(state: Id) -> BlockProperties {
    match state {
        Id::FlowingWater => BlockProperties {
            light_opacity: 15,
            ..BlockProperties::fluid(0.0)
        },
        Id::Water => BlockProperties {
            light_opacity: 3,
            ..BlockProperties::fluid(100.0)
        },
        Id::FlowingLava | Id::Lava => BlockProperties {
            light_opacity: 15,
            light_emission: 15,
            ..BlockProperties::fluid(0.0)
        },
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

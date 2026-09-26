use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::FlowingWater | Id::Water | Id::FlowingLava | Id::Lava
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, _id: Id) -> bool {
        false
    }

    fn light_opacity(&self, id: Id) -> u8 {
        if id == Id::Water { 3 } else { 15 }
    }

    fn light_emission(&self, id: Id) -> u8 {
        if matches!(id, Id::FlowingLava | Id::Lava) {
            15
        } else {
            0
        }
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
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

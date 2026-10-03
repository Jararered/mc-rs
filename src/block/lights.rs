use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::Torch
                | Id::Fire
                | Id::Glowstone
                | Id::JackOLantern
                | Id::TorchWest
                | Id::TorchEast
                | Id::TorchNorth
                | Id::TorchSouth
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        !is_torch(id) && id != Id::Fire
    }

    fn light_opacity(&self, id: Id) -> u8 {
        if is_torch(id) || id == Id::Fire {
            0
        } else {
            15
        }
    }

    fn light_emission(&self, id: Id) -> u8 {
        if matches!(id, Id::Glowstone | Id::JackOLantern | Id::Fire) || is_torch(id) {
            15
        } else {
            0
        }
    }

    fn crossed_plant(&self, id: Id) -> bool {
        id == Id::Fire
    }

    fn torch(&self, id: Id) -> bool {
        is_torch(id)
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::Torch | Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth => {
            BlockProperties {
                torch: true,
                light_emission: 15,
                selection_bounds: crate::block::properties::torch_selection_bounds(id),
                ..BlockProperties::non_colliding(0.0)
            }
        }
        Id::Fire => BlockProperties {
            light_emission: 15,
            targetable: false,
            replaceable: true,
            crossed_plant: true,
            ..BlockProperties::non_colliding(0.0)
        },
        Id::UnlitRedstoneTorch => BlockProperties::solid(0.0),
        Id::RedstoneTorch => BlockProperties::solid(0.0),
        Id::Glowstone => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 15,
            ..BlockProperties::solid(0.3)
        },
        Id::JackOLantern => BlockProperties {
            light_emission: 15,
            ..BlockProperties::solid(1.0)
        },
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

fn is_torch(id: Id) -> bool {
    matches!(
        id,
        Id::Torch | Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth
    )
}

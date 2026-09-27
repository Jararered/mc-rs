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
                | Id::Glowstone
                | Id::JackOLantern
                | Id::TorchWest
                | Id::TorchEast
                | Id::TorchNorth
                | Id::TorchSouth
                | Id::RedstoneTorch
                | Id::UnlitRedstoneTorch
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        !is_torch(id)
    }

    fn light_opacity(&self, id: Id) -> u8 {
        if is_torch(id) { 0 } else { 15 }
    }

    fn light_emission(&self, id: Id) -> u8 {
        if id == Id::RedstoneTorch {
            7
        } else if id == Id::UnlitRedstoneTorch {
            0
        } else if matches!(id, Id::Glowstone | Id::JackOLantern) || is_torch(id) {
            15
        } else {
            0
        }
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
        Id::Fire => BlockProperties::solid(0.0),
        Id::UnlitRedstoneTorch | Id::RedstoneTorch => BlockProperties {
            torch: true,
            light_emission: if id == Id::RedstoneTorch { 7 } else { 0 },
            selection_bounds: ([0.375, 0.0, 0.375], [0.625, 0.625, 0.625]),
            ..BlockProperties::non_colliding(0.0)
        },
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
        Id::Torch
            | Id::TorchWest
            | Id::TorchEast
            | Id::TorchNorth
            | Id::TorchSouth
            | Id::RedstoneTorch
            | Id::UnlitRedstoneTorch
    )
}

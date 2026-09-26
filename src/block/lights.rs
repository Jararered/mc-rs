use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: Id) -> &'static str {
        match state {
            Id::Torch => "torch",
            Id::Fire => "fire",
            Id::UnlitRedstoneTorch => "unlit_redstone_torch",
            Id::RedstoneTorch => "redstone_torch",
            Id::Glowstone => "glowstone",
            Id::JackOLantern => "jack_olantern",
            Id::TorchWest => "torch_west",
            Id::TorchEast => "torch_east",
            Id::TorchNorth => "torch_north",
            Id::TorchSouth => "torch_south",
            Id::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: Id) -> bool {
        matches!(
            state,
            Id::Torch
                | Id::Glowstone
                | Id::JackOLantern
                | Id::TorchWest
                | Id::TorchEast
                | Id::TorchNorth
                | Id::TorchSouth
        )
    }

    fn properties(&self, state: Id) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: Id) -> bool {
        !is_torch(state)
    }

    fn light_opacity(&self, state: Id) -> u8 {
        if is_torch(state) { 0 } else { 15 }
    }

    fn light_emission(&self, state: Id) -> u8 {
        if matches!(state, Id::Glowstone | Id::JackOLantern) || is_torch(state) {
            15
        } else {
            0
        }
    }

    fn torch(&self, state: Id) -> bool {
        is_torch(state)
    }
}

fn properties(state: Id) -> BlockProperties {
    match state {
        Id::Torch | Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth => {
            BlockProperties {
                torch: true,
                light_emission: 15,
                selection_bounds: crate::block::properties::torch_selection_bounds(state),
                ..BlockProperties::non_colliding(0.0)
            }
        }
        Id::Fire => BlockProperties::solid(0.0),
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

fn is_torch(state: Id) -> bool {
    matches!(
        state,
        Id::Torch | Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth
    )
}

use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::BlockId;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::Torch => "torch",
            BlockId::Fire => "fire",
            BlockId::UnlitRedstoneTorch => "unlit_redstone_torch",
            BlockId::RedstoneTorch => "redstone_torch",
            BlockId::Glowstone => "glowstone",
            BlockId::JackOLantern => "jack_olantern",
            BlockId::TorchWest => "torch_west",
            BlockId::TorchEast => "torch_east",
            BlockId::TorchNorth => "torch_north",
            BlockId::TorchSouth => "torch_south",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::Torch
                | BlockId::Glowstone
                | BlockId::JackOLantern
                | BlockId::TorchWest
                | BlockId::TorchEast
                | BlockId::TorchNorth
                | BlockId::TorchSouth
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: BlockId) -> bool {
        !is_torch(state)
    }

    fn light_opacity(&self, state: BlockId) -> u8 {
        if is_torch(state) { 0 } else { 15 }
    }

    fn light_emission(&self, state: BlockId) -> u8 {
        if matches!(state, BlockId::Glowstone | BlockId::JackOLantern) || is_torch(state) {
            15
        } else {
            0
        }
    }

    fn torch(&self, state: BlockId) -> bool {
        is_torch(state)
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Torch
        | BlockId::TorchWest
        | BlockId::TorchEast
        | BlockId::TorchNorth
        | BlockId::TorchSouth => BlockProperties {
            torch: true,
            light_emission: 15,
            selection_bounds: crate::block::properties::torch_selection_bounds(state),
            ..BlockProperties::non_colliding(0.0)
        },
        BlockId::Fire => BlockProperties::solid(0.0),
        BlockId::UnlitRedstoneTorch => BlockProperties::solid(0.0),
        BlockId::RedstoneTorch => BlockProperties::solid(0.0),
        BlockId::Glowstone => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 15,
            ..BlockProperties::solid(0.3)
        },
        BlockId::JackOLantern => BlockProperties {
            light_emission: 15,
            ..BlockProperties::solid(1.0)
        },
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

fn is_torch(state: BlockId) -> bool {
    matches!(
        state,
        BlockId::Torch
            | BlockId::TorchWest
            | BlockId::TorchEast
            | BlockId::TorchNorth
            | BlockId::TorchSouth
    )
}

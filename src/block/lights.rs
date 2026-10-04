use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(
            block,
            Block::Torch
                | Block::Fire
                | Block::Glowstone
                | Block::JackOLantern
                | Block::TorchWest
                | Block::TorchEast
                | Block::TorchNorth
                | Block::TorchSouth
        )
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }

    fn opaque_cube(&self, block: Block) -> bool {
        !block.is_torch() && block != Block::Fire
    }

    fn light_opacity(&self, block: Block) -> u8 {
        if block.is_torch() || block == Block::Fire {
            0
        } else {
            15
        }
    }

    fn light_emission(&self, block: Block) -> u8 {
        if matches!(block, Block::Glowstone | Block::JackOLantern | Block::Fire) || block.is_torch()
        {
            15
        } else {
            0
        }
    }

    fn crossed_plant(&self, block: Block) -> bool {
        block == Block::Fire
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Torch
        | Block::TorchWest
        | Block::TorchEast
        | Block::TorchNorth
        | Block::TorchSouth => BlockProperties {
            light_emission: 15,
            selection_bounds: crate::block::properties::torch_selection_bounds(block),
            ..BlockProperties::non_colliding(0.0)
        },
        Block::Fire => BlockProperties {
            light_emission: 15,
            targetable: false,
            replaceable: true,
            crossed_plant: true,
            ..BlockProperties::non_colliding(0.0)
        },
        Block::UnlitRedstoneTorch => BlockProperties::solid(0.0),
        Block::RedstoneTorch => BlockProperties::solid(0.0),
        Block::Glowstone => BlockProperties {
            harvestable_by_hand: false,
            light_emission: 15,
            ..BlockProperties::solid(0.3)
        },
        Block::JackOLantern => BlockProperties {
            light_emission: 15,
            ..BlockProperties::solid(1.0)
        },
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

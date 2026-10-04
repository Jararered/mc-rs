use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(block, Block::Dispenser)
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Bed => BlockProperties::solid(0.0),
        Block::Cobweb => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Block::DetectorRail => BlockProperties::solid(0.0),
        Block::Dispenser => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.5)
        },
        Block::IronDoor => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Block::Lever => BlockProperties::solid(0.0),
        Block::MovingPiston => BlockProperties::solid(0.0),
        Block::NetherPortal => BlockProperties::solid(0.0),
        Block::Piston => BlockProperties::solid(0.0),
        Block::PistonHead => BlockProperties::solid(0.0),
        Block::PoweredRail => BlockProperties::solid(0.0),
        Block::PoweredRepeater => BlockProperties::solid(0.0),
        Block::Rail => BlockProperties::solid(0.0),
        Block::RedstoneWire => BlockProperties::solid(0.0),
        Block::Repeater => BlockProperties::solid(0.0),
        Block::StandingSign => BlockProperties::solid(0.0),
        Block::StickyPiston => BlockProperties::solid(0.0),
        Block::StoneButton => BlockProperties::solid(0.0),
        Block::StonePressurePlate => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Block::WallSign => BlockProperties::solid(0.0),
        Block::WoodenDoor => BlockProperties::solid(0.0),
        Block::WoodenPressurePlate => BlockProperties::solid(0.0),
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

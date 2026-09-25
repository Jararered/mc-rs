use crate::block::block::BlockId;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::Bed => "bed",
            BlockId::Cobweb => "cobweb",
            BlockId::DetectorRail => "detector_rail",
            BlockId::Dispenser => "dispenser",
            BlockId::IronDoor => "iron_door",
            BlockId::Lever => "lever",
            BlockId::MovingPiston => "moving_piston",
            BlockId::NetherPortal => "nether_portal",
            BlockId::Piston => "piston",
            BlockId::PistonHead => "piston_head",
            BlockId::PoweredRail => "powered_rail",
            BlockId::PoweredRepeater => "powered_repeater",
            BlockId::Rail => "rail",
            BlockId::RedstoneWire => "redstone_wire",
            BlockId::Repeater => "repeater",
            BlockId::StandingSign => "standing_sign",
            BlockId::StickyPiston => "sticky_piston",
            BlockId::StoneButton => "stone_button",
            BlockId::StonePressurePlate => "stone_pressure_plate",
            BlockId::WallSign => "wall_sign",
            BlockId::WoodenDoor => "wooden_door",
            BlockId::WoodenPressurePlate => "wooden_pressure_plate",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(state, BlockId::Dispenser)
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Bed => BlockProperties::solid(0.0),
        BlockId::Cobweb => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::DetectorRail => BlockProperties::solid(0.0),
        BlockId::Dispenser => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.5)
        },
        BlockId::IronDoor => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::Lever => BlockProperties::solid(0.0),
        BlockId::MovingPiston => BlockProperties::solid(0.0),
        BlockId::NetherPortal => BlockProperties::solid(0.0),
        BlockId::Piston => BlockProperties::solid(0.0),
        BlockId::PistonHead => BlockProperties::solid(0.0),
        BlockId::PoweredRail => BlockProperties::solid(0.0),
        BlockId::PoweredRepeater => BlockProperties::solid(0.0),
        BlockId::Rail => BlockProperties::solid(0.0),
        BlockId::RedstoneWire => BlockProperties::solid(0.0),
        BlockId::Repeater => BlockProperties::solid(0.0),
        BlockId::StandingSign => BlockProperties::solid(0.0),
        BlockId::StickyPiston => BlockProperties::solid(0.0),
        BlockId::StoneButton => BlockProperties::solid(0.0),
        BlockId::StonePressurePlate => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::WallSign => BlockProperties::solid(0.0),
        BlockId::WoodenDoor => BlockProperties::solid(0.0),
        BlockId::WoodenPressurePlate => BlockProperties::solid(0.0),
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

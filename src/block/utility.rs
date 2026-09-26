use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: Id) -> &'static str {
        match state {
            Id::Bed => "bed",
            Id::Cobweb => "cobweb",
            Id::DetectorRail => "detector_rail",
            Id::Dispenser => "dispenser",
            Id::IronDoor => "iron_door",
            Id::Lever => "lever",
            Id::MovingPiston => "moving_piston",
            Id::NetherPortal => "nether_portal",
            Id::Piston => "piston",
            Id::PistonHead => "piston_head",
            Id::PoweredRail => "powered_rail",
            Id::PoweredRepeater => "powered_repeater",
            Id::Rail => "rail",
            Id::RedstoneWire => "redstone_wire",
            Id::Repeater => "repeater",
            Id::StandingSign => "standing_sign",
            Id::StickyPiston => "sticky_piston",
            Id::StoneButton => "stone_button",
            Id::StonePressurePlate => "stone_pressure_plate",
            Id::WallSign => "wall_sign",
            Id::WoodenDoor => "wooden_door",
            Id::WoodenPressurePlate => "wooden_pressure_plate",
            Id::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: Id) -> bool {
        matches!(state, Id::Dispenser)
    }

    fn properties(&self, state: Id) -> BlockProperties {
        properties(state)
    }
}

fn properties(state: Id) -> BlockProperties {
    match state {
        Id::Bed => BlockProperties::solid(0.0),
        Id::Cobweb => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::DetectorRail => BlockProperties::solid(0.0),
        Id::Dispenser => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.5)
        },
        Id::IronDoor => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::Lever => BlockProperties::solid(0.0),
        Id::MovingPiston => BlockProperties::solid(0.0),
        Id::NetherPortal => BlockProperties::solid(0.0),
        Id::Piston => BlockProperties::solid(0.0),
        Id::PistonHead => BlockProperties::solid(0.0),
        Id::PoweredRail => BlockProperties::solid(0.0),
        Id::PoweredRepeater => BlockProperties::solid(0.0),
        Id::Rail => BlockProperties::solid(0.0),
        Id::RedstoneWire => BlockProperties::solid(0.0),
        Id::Repeater => BlockProperties::solid(0.0),
        Id::StandingSign => BlockProperties::solid(0.0),
        Id::StickyPiston => BlockProperties::solid(0.0),
        Id::StoneButton => BlockProperties::solid(0.0),
        Id::StonePressurePlate => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::WallSign => BlockProperties::solid(0.0),
        Id::WoodenDoor => BlockProperties::solid(0.0),
        Id::WoodenPressurePlate => BlockProperties::solid(0.0),
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

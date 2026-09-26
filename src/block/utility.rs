use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(id, Id::Dispenser)
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
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

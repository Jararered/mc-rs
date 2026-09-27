use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::Dispenser
                | Id::RedstoneWire
                | Id::Repeater
                | Id::PoweredRepeater
                | Id::Lever
                | Id::StoneButton
                | Id::StonePressurePlate
                | Id::WoodenPressurePlate
                | Id::WoodenDoor
                | Id::IronDoor
                | Id::Trapdoor
                | Id::Piston
                | Id::StickyPiston
                | Id::PistonHead
                | Id::MovingPiston
                | Id::Rail
                | Id::PoweredRail
                | Id::DetectorRail
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        properties(id).opaque_cube
    }
    fn light_opacity(&self, id: Id) -> u8 {
        properties(id).light_opacity
    }
    fn light_emission(&self, id: Id) -> u8 {
        properties(id).light_emission
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::Bed => BlockProperties::solid(0.0),
        Id::Cobweb => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::DetectorRail => BlockProperties::non_colliding(0.7),
        Id::Dispenser => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.5)
        },
        Id::Trapdoor => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            ..BlockProperties::solid(3.0)
        },
        Id::IronDoor => BlockProperties {
            harvestable_by_hand: false,
            opaque_cube: false,
            light_opacity: 0,
            ..BlockProperties::solid(5.0)
        },
        Id::Lever => BlockProperties {
            selection_bounds: ([0.25, 0.0, 0.25], [0.75, 0.625, 0.75]),
            ..BlockProperties::non_colliding(0.5)
        },
        Id::MovingPiston => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            ..BlockProperties::solid(0.5)
        },
        Id::NetherPortal => BlockProperties::solid(0.0),
        Id::Piston => BlockProperties::solid(0.5),
        Id::PistonHead => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            ..BlockProperties::solid(0.5)
        },
        Id::PoweredRail => BlockProperties::non_colliding(0.7),
        Id::PoweredRepeater => BlockProperties {
            light_emission: 9,
            selection_bounds: ([0.0, 0.0, 0.0], [1.0, 0.125, 1.0]),
            ..BlockProperties::non_colliding(0.0)
        },
        Id::Rail => BlockProperties::non_colliding(0.7),
        Id::RedstoneWire => BlockProperties {
            selection_bounds: ([0.0, 0.0, 0.0], [1.0, 0.0625, 1.0]),
            ..BlockProperties::non_colliding(0.0)
        },
        Id::Repeater => BlockProperties {
            selection_bounds: ([0.0, 0.0, 0.0], [1.0, 0.125, 1.0]),
            ..BlockProperties::non_colliding(0.0)
        },
        Id::StandingSign => BlockProperties::solid(0.0),
        Id::StickyPiston => BlockProperties::solid(0.5),
        Id::StoneButton => BlockProperties {
            selection_bounds: ([0.375, 0.375, 0.0], [0.625, 0.625, 0.125]),
            ..BlockProperties::non_colliding(0.5)
        },
        Id::StonePressurePlate => BlockProperties {
            harvestable_by_hand: false,
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 0.0625, 0.9375]),
            ..BlockProperties::non_colliding(0.5)
        },
        Id::WallSign => BlockProperties::solid(0.0),
        Id::WoodenDoor => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            ..BlockProperties::solid(3.0)
        },
        Id::WoodenPressurePlate => BlockProperties {
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 0.0625, 0.9375]),
            ..BlockProperties::non_colliding(0.5)
        },
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

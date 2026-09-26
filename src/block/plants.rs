use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::Wood
                | Id::Leaves
                | Id::TallGrass
                | Id::DeadBush
                | Id::Dandelion
                | Id::Rose
                | Id::BrownMushroom
                | Id::RedMushroom
                | Id::Cactus
                | Id::SugarCane
                | Id::SpruceLeaves
                | Id::BirchLeaves
                | Id::SpruceWood
                | Id::BirchWood
                | Id::SprucePlanks
                | Id::BirchPlanks
                | Id::Fern
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        !matches!(
            id,
            Id::Leaves
                | Id::SpruceLeaves
                | Id::BirchLeaves
                | Id::Cactus
                | Id::DeadBush
                | Id::Dandelion
                | Id::Rose
                | Id::BrownMushroom
                | Id::RedMushroom
                | Id::TallGrass
                | Id::Fern
                | Id::SugarCane
        )
    }

    fn light_opacity(&self, id: Id) -> u8 {
        match id {
            Id::Leaves | Id::SpruceLeaves | Id::BirchLeaves => 1,
            Id::Cactus
            | Id::DeadBush
            | Id::Dandelion
            | Id::Rose
            | Id::BrownMushroom
            | Id::RedMushroom
            | Id::TallGrass
            | Id::Fern
            | Id::SugarCane => 0,
            _ => 15,
        }
    }

    fn crossed_plant(&self, id: Id) -> bool {
        matches!(
            id,
            Id::DeadBush
                | Id::Dandelion
                | Id::Rose
                | Id::BrownMushroom
                | Id::RedMushroom
                | Id::TallGrass
                | Id::Fern
                | Id::SugarCane
        )
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::Sapling => BlockProperties::solid(0.0),
        Id::Wood | Id::SpruceWood | Id::BirchWood => BlockProperties::solid(2.0),
        Id::SprucePlanks | Id::BirchPlanks => BlockProperties::solid(2.0),
        Id::Leaves | Id::SpruceLeaves | Id::BirchLeaves => BlockProperties {
            opaque_cube: false,
            light_opacity: 1,
            ..BlockProperties::solid(0.2)
        },
        Id::TallGrass | Id::Fern | Id::DeadBush => {
            crossed_plant(0.0, ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9]))
        }
        Id::Dandelion | Id::Rose => crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.6, 0.7])),
        Id::BrownMushroom | Id::RedMushroom => {
            crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.4, 0.7]))
        }
        Id::Crops => BlockProperties::solid(0.0),
        Id::Cactus => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375])),
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 1.0, 0.9375]),
            ..BlockProperties::solid(0.4)
        },
        Id::SugarCane => crossed_plant(0.0, ([0.125, 0.0, 0.125], [0.875, 1.0, 0.875])),
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

fn crossed_plant(
    hardness: f32,
    selection_bounds: crate::block::definition::BlockBounds,
) -> BlockProperties {
    BlockProperties {
        crossed_plant: true,
        selection_bounds,
        ..BlockProperties::non_colliding(hardness)
    }
}

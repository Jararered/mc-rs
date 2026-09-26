use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: Id) -> &'static str {
        match state {
            Id::Sapling => "sapling",
            Id::Wood => "wood",
            Id::Leaves => "leaves",
            Id::TallGrass => "tall_grass",
            Id::DeadBush => "dead_bush",
            Id::Dandelion => "dandelion",
            Id::Rose => "rose",
            Id::BrownMushroom => "brown_mushroom",
            Id::RedMushroom => "red_mushroom",
            Id::Crops => "crops",
            Id::Cactus => "cactus",
            Id::SugarCane => "sugar_cane",
            Id::SpruceLeaves => "spruce_leaves",
            Id::BirchLeaves => "birch_leaves",
            Id::SpruceWood => "spruce_wood",
            Id::BirchWood => "birch_wood",
            Id::SprucePlanks => "spruce_planks",
            Id::BirchPlanks => "birch_planks",
            Id::Fern => "fern",
            Id::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: Id) -> bool {
        matches!(
            state,
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

    fn properties(&self, state: Id) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: Id) -> bool {
        !matches!(
            state,
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

    fn light_opacity(&self, state: Id) -> u8 {
        match state {
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

    fn crossed_plant(&self, state: Id) -> bool {
        matches!(
            state,
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

fn properties(state: Id) -> BlockProperties {
    match state {
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

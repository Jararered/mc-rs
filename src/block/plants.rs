use crate::block::block::BlockId;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::Sapling => "sapling",
            BlockId::Wood => "wood",
            BlockId::Leaves => "leaves",
            BlockId::TallGrass => "tall_grass",
            BlockId::DeadBush => "dead_bush",
            BlockId::Dandelion => "dandelion",
            BlockId::Rose => "rose",
            BlockId::BrownMushroom => "brown_mushroom",
            BlockId::RedMushroom => "red_mushroom",
            BlockId::Crops => "crops",
            BlockId::Cactus => "cactus",
            BlockId::SugarCane => "sugar_cane",
            BlockId::SpruceLeaves => "spruce_leaves",
            BlockId::BirchLeaves => "birch_leaves",
            BlockId::SpruceWood => "spruce_wood",
            BlockId::BirchWood => "birch_wood",
            BlockId::SprucePlanks => "spruce_planks",
            BlockId::BirchPlanks => "birch_planks",
            BlockId::Fern => "fern",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::Wood
                | BlockId::Leaves
                | BlockId::TallGrass
                | BlockId::DeadBush
                | BlockId::Dandelion
                | BlockId::Rose
                | BlockId::BrownMushroom
                | BlockId::RedMushroom
                | BlockId::Cactus
                | BlockId::SugarCane
                | BlockId::SpruceLeaves
                | BlockId::BirchLeaves
                | BlockId::SpruceWood
                | BlockId::BirchWood
                | BlockId::SprucePlanks
                | BlockId::BirchPlanks
                | BlockId::Fern
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: BlockId) -> bool {
        !matches!(
            state,
            BlockId::Leaves
                | BlockId::SpruceLeaves
                | BlockId::BirchLeaves
                | BlockId::Cactus
                | BlockId::DeadBush
                | BlockId::Dandelion
                | BlockId::Rose
                | BlockId::BrownMushroom
                | BlockId::RedMushroom
                | BlockId::TallGrass
                | BlockId::Fern
                | BlockId::SugarCane
        )
    }

    fn light_opacity(&self, state: BlockId) -> u8 {
        match state {
            BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => 1,
            BlockId::Cactus
            | BlockId::DeadBush
            | BlockId::Dandelion
            | BlockId::Rose
            | BlockId::BrownMushroom
            | BlockId::RedMushroom
            | BlockId::TallGrass
            | BlockId::Fern
            | BlockId::SugarCane => 0,
            _ => 15,
        }
    }

    fn crossed_plant(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::DeadBush
                | BlockId::Dandelion
                | BlockId::Rose
                | BlockId::BrownMushroom
                | BlockId::RedMushroom
                | BlockId::TallGrass
                | BlockId::Fern
                | BlockId::SugarCane
        )
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Sapling => BlockProperties::solid(0.0),
        BlockId::Wood | BlockId::SpruceWood | BlockId::BirchWood => BlockProperties::solid(2.0),
        BlockId::SprucePlanks | BlockId::BirchPlanks => BlockProperties::solid(2.0),
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => BlockProperties {
            opaque_cube: false,
            light_opacity: 1,
            ..BlockProperties::solid(0.2)
        },
        BlockId::TallGrass | BlockId::Fern | BlockId::DeadBush => {
            crossed_plant(0.0, ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9]))
        }
        BlockId::Dandelion | BlockId::Rose => {
            crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.6, 0.7]))
        }
        BlockId::BrownMushroom | BlockId::RedMushroom => {
            crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.4, 0.7]))
        }
        BlockId::Crops => BlockProperties::solid(0.0),
        BlockId::Cactus => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375])),
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 1.0, 0.9375]),
            ..BlockProperties::solid(0.4)
        },
        BlockId::SugarCane => crossed_plant(0.0, ([0.125, 0.0, 0.125], [0.875, 1.0, 0.875])),
        BlockId::Unknown(_) => BlockProperties::unknown(),
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

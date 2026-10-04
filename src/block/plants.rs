use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(
            block,
            Block::Wood
                | Block::Leaves
                | Block::TallGrass
                | Block::DeadBush
                | Block::Dandelion
                | Block::Rose
                | Block::BrownMushroom
                | Block::RedMushroom
                | Block::Cactus
                | Block::SugarCane
                | Block::Crops
                | Block::SpruceLeaves
                | Block::BirchLeaves
                | Block::SpruceWood
                | Block::BirchWood
                | Block::SprucePlanks
                | Block::BirchPlanks
                | Block::Fern
        )
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }

    fn opaque_cube(&self, block: Block) -> bool {
        !matches!(
            block,
            Block::Leaves
                | Block::SpruceLeaves
                | Block::BirchLeaves
                | Block::Cactus
                | Block::DeadBush
                | Block::Dandelion
                | Block::Rose
                | Block::BrownMushroom
                | Block::RedMushroom
                | Block::TallGrass
                | Block::Fern
                | Block::SugarCane
                | Block::Crops
        )
    }

    fn light_opacity(&self, block: Block) -> u8 {
        match block {
            Block::Leaves | Block::SpruceLeaves | Block::BirchLeaves => 1,
            Block::Cactus
            | Block::DeadBush
            | Block::Dandelion
            | Block::Rose
            | Block::BrownMushroom
            | Block::RedMushroom
            | Block::TallGrass
            | Block::Fern
            | Block::SugarCane
            | Block::Crops => 0,
            _ => 15,
        }
    }

    fn crossed_plant(&self, block: Block) -> bool {
        matches!(
            block,
            Block::DeadBush
                | Block::Dandelion
                | Block::Rose
                | Block::BrownMushroom
                | Block::RedMushroom
                | Block::TallGrass
                | Block::Fern
                | Block::SugarCane
        )
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Sapling => BlockProperties::solid(0.0),
        Block::Wood | Block::SpruceWood | Block::BirchWood => BlockProperties::solid(2.0),
        Block::SprucePlanks | Block::BirchPlanks => BlockProperties::solid(2.0),
        Block::Leaves | Block::SpruceLeaves | Block::BirchLeaves => BlockProperties {
            opaque_cube: false,
            light_opacity: 1,
            ..BlockProperties::solid(0.2)
        },
        Block::TallGrass | Block::Fern | Block::DeadBush => {
            crossed_plant(0.0, ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9]))
        }
        Block::Dandelion | Block::Rose => crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.6, 0.7])),
        Block::BrownMushroom | Block::RedMushroom => {
            crossed_plant(0.0, ([0.3, 0.0, 0.3], [0.7, 0.4, 0.7]))
        }
        // `BlockCrops`: a quarter-block selection box and no collision.
        Block::Crops => BlockProperties {
            selection_bounds: ([0.0; 3], [1.0, 0.25, 1.0]),
            ..BlockProperties::non_colliding(0.0)
        },
        Block::Cactus => BlockProperties {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375])),
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 1.0, 0.9375]),
            ..BlockProperties::solid(0.4)
        },
        Block::SugarCane => crossed_plant(0.0, ([0.125, 0.0, 0.125], [0.875, 1.0, 0.875])),
        Block::Unknown(_) => BlockProperties::unknown(),
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

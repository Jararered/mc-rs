use crate::block::blocks::Block;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, block: Block) -> bool {
        matches!(
            block,
            Block::Air
                | Block::Stone
                | Block::Grass
                | Block::Dirt
                | Block::Cobblestone
                | Block::WoodenPlanks
                | Block::Bedrock
                | Block::Sand
                | Block::Gravel
                | Block::Sponge
                | Block::Sandstone
                | Block::NoteBlock
                | Block::Wool
                | Block::GoldBlock
                | Block::IronBlock
                | Block::DoubleStoneSlab
                | Block::Bricks
                | Block::Tnt
                | Block::Bookshelf
                | Block::MossyCobblestone
                | Block::Obsidian
                | Block::MobSpawner
                | Block::DiamondBlock
                | Block::CraftingTable
                | Block::Farmland
                | Block::SnowLayer
                | Block::Ice
                | Block::Snow
                | Block::Clay
                | Block::Jukebox
                | Block::Netherrack
        )
    }

    fn properties(&self, block: Block) -> BlockProperties {
        properties(block)
    }

    fn opaque_cube(&self, block: Block) -> bool {
        !matches!(
            block,
            Block::Air | Block::MobSpawner | Block::Ice | Block::SnowLayer | Block::Farmland
        )
    }

    fn light_opacity(&self, block: Block) -> u8 {
        match block {
            Block::Air | Block::SnowLayer => 0,
            Block::Ice => 3,
            _ => 15,
        }
    }
}

fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Air => BlockProperties::fluid(0.0),
        Block::Stone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(1.5)
        },
        Block::Grass => BlockProperties::solid(0.6),
        Block::Dirt => BlockProperties::solid(0.5),
        Block::Cobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Block::WoodenPlanks => BlockProperties::solid(2.0),
        Block::Bedrock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(-1.0)
        },
        Block::Sand => BlockProperties::solid(0.5),
        Block::Gravel => BlockProperties::solid(0.6),
        Block::Sponge => BlockProperties::solid(0.6),
        Block::Glass => BlockProperties::solid(0.0),
        Block::Sandstone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.8)
        },
        Block::NoteBlock => BlockProperties::solid(0.8),
        Block::Wool => BlockProperties::solid(0.8),
        Block::GoldBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Block::IronBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        Block::DoubleStoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Block::StoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Block::Bricks => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Block::Tnt => BlockProperties::solid(0.0),
        Block::Bookshelf => BlockProperties::solid(1.5),
        Block::MossyCobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Block::Obsidian => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(10.0)
        },
        Block::MobSpawner => BlockProperties {
            opaque_cube: false,
            ..BlockProperties::solid(0.0)
        },
        Block::WoodenStairs => BlockProperties::solid(0.0),
        Block::CobblestoneStairs => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Block::DiamondBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        Block::CraftingTable => BlockProperties::solid(2.5),
        Block::Farmland => BlockProperties {
            opaque_cube: false,
            selection_bounds: ([0.0; 3], [1.0, 15.0 / 16.0, 1.0]),
            ..BlockProperties::solid(0.6)
        },
        Block::SnowLayer => BlockProperties {
            opaque_cube: false,
            collision_bounds: None,
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            light_opacity: 0,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.1)
        },
        Block::Ice => BlockProperties {
            opaque_cube: false,
            slipperiness: 0.98,
            light_opacity: 3,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.5)
        },
        Block::Snow => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.2)
        },
        Block::Clay => BlockProperties::solid(0.6),
        Block::Jukebox => BlockProperties::solid(2.0),
        Block::Fence => BlockProperties::solid(0.0),
        Block::SoulSand => BlockProperties::solid(0.0),
        Block::Cake => BlockProperties::solid(0.0),
        Block::LockedChest => BlockProperties::solid(0.0),
        Block::Trapdoor => BlockProperties::solid(0.0),
        Block::Netherrack => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.4)
        },
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

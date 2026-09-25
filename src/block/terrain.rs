use crate::block::block::BlockId;
use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn name(&self, state: BlockId) -> &'static str {
        match state {
            BlockId::Air => "air",
            BlockId::Stone => "stone",
            BlockId::Grass => "grass",
            BlockId::Dirt => "dirt",
            BlockId::Cobblestone => "cobblestone",
            BlockId::WoodenPlanks => "wooden_planks",
            BlockId::Bedrock => "bedrock",
            BlockId::Sand => "sand",
            BlockId::Gravel => "gravel",
            BlockId::Sponge => "sponge",
            BlockId::Glass => "glass",
            BlockId::Sandstone => "sandstone",
            BlockId::NoteBlock => "note_block",
            BlockId::Wool => "wool",
            BlockId::GoldBlock => "gold_block",
            BlockId::IronBlock => "iron_block",
            BlockId::DoubleStoneSlab => "double_stone_slab",
            BlockId::StoneSlab => "stone_slab",
            BlockId::Bricks => "bricks",
            BlockId::Tnt => "tnt",
            BlockId::Bookshelf => "bookshelf",
            BlockId::MossyCobblestone => "mossy_cobblestone",
            BlockId::Obsidian => "obsidian",
            BlockId::MobSpawner => "mob_spawner",
            BlockId::WoodenStairs => "wooden_stairs",
            BlockId::CobblestoneStairs => "cobblestone_stairs",
            BlockId::DiamondBlock => "diamond_block",
            BlockId::CraftingTable => "crafting_table",
            BlockId::Farmland => "farmland",
            BlockId::SnowLayer => "snow_layer",
            BlockId::Ice => "ice",
            BlockId::Snow => "snow",
            BlockId::Clay => "clay",
            BlockId::Jukebox => "jukebox",
            BlockId::Fence => "fence",
            BlockId::SoulSand => "soul_sand",
            BlockId::Cake => "cake",
            BlockId::LockedChest => "locked_chest",
            BlockId::Trapdoor => "trapdoor",
            BlockId::Netherrack => "netherrack",
            BlockId::Unknown(_) => "unknown",
            _ => "unknown",
        }
    }

    fn in_world(&self, state: BlockId) -> bool {
        matches!(
            state,
            BlockId::Air
                | BlockId::Stone
                | BlockId::Grass
                | BlockId::Dirt
                | BlockId::Cobblestone
                | BlockId::WoodenPlanks
                | BlockId::Bedrock
                | BlockId::Sand
                | BlockId::Gravel
                | BlockId::Sponge
                | BlockId::Sandstone
                | BlockId::NoteBlock
                | BlockId::Wool
                | BlockId::GoldBlock
                | BlockId::IronBlock
                | BlockId::DoubleStoneSlab
                | BlockId::Bricks
                | BlockId::Tnt
                | BlockId::Bookshelf
                | BlockId::MossyCobblestone
                | BlockId::Obsidian
                | BlockId::MobSpawner
                | BlockId::DiamondBlock
                | BlockId::CraftingTable
                | BlockId::Farmland
                | BlockId::Ice
                | BlockId::Snow
                | BlockId::Clay
                | BlockId::Jukebox
                | BlockId::Netherrack
        )
    }

    fn properties(&self, state: BlockId) -> BlockProperties {
        properties(state)
    }

    fn opaque_cube(&self, state: BlockId) -> bool {
        !matches!(
            state,
            BlockId::Air
                | BlockId::MobSpawner
                | BlockId::Ice
                | BlockId::SnowLayer
                | BlockId::Farmland
        )
    }

    fn light_opacity(&self, state: BlockId) -> u8 {
        match state {
            BlockId::Air | BlockId::SnowLayer => 0,
            BlockId::Ice => 3,
            _ => 15,
        }
    }
}

fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Air => BlockProperties::fluid(0.0),
        BlockId::Stone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(1.5)
        },
        BlockId::Grass => BlockProperties::solid(0.6),
        BlockId::Dirt => BlockProperties::solid(0.5),
        BlockId::Cobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        BlockId::WoodenPlanks => BlockProperties::solid(2.0),
        BlockId::Bedrock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(-1.0)
        },
        BlockId::Sand => BlockProperties::solid(0.5),
        BlockId::Gravel => BlockProperties::solid(0.6),
        BlockId::Sponge => BlockProperties::solid(0.6),
        BlockId::Glass => BlockProperties::solid(0.0),
        BlockId::Sandstone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.8)
        },
        BlockId::NoteBlock => BlockProperties::solid(0.8),
        BlockId::Wool => BlockProperties::solid(0.8),
        BlockId::GoldBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        BlockId::IronBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        BlockId::DoubleStoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        BlockId::StoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::Bricks => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        BlockId::Tnt => BlockProperties::solid(0.0),
        BlockId::Bookshelf => BlockProperties::solid(1.5),
        BlockId::MossyCobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        BlockId::Obsidian => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(10.0)
        },
        BlockId::MobSpawner => BlockProperties {
            opaque_cube: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::WoodenStairs => BlockProperties::solid(0.0),
        BlockId::CobblestoneStairs => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        BlockId::DiamondBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        BlockId::CraftingTable => BlockProperties::solid(2.5),
        BlockId::Farmland => BlockProperties {
            opaque_cube: false,
            selection_bounds: ([0.0; 3], [1.0, 15.0 / 16.0, 1.0]),
            ..BlockProperties::solid(0.6)
        },
        BlockId::SnowLayer => BlockProperties {
            opaque_cube: false,
            collision_bounds: Some(([0.0; 3], [1.0, 0.125, 1.0])),
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            light_opacity: 0,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.1)
        },
        BlockId::Ice => BlockProperties {
            opaque_cube: false,
            slipperiness: 0.98,
            light_opacity: 3,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.5)
        },
        BlockId::Snow => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.2)
        },
        BlockId::Clay => BlockProperties::solid(0.6),
        BlockId::Jukebox => BlockProperties::solid(2.0),
        BlockId::Fence => BlockProperties::solid(0.0),
        BlockId::SoulSand => BlockProperties::solid(0.0),
        BlockId::Cake => BlockProperties::solid(0.0),
        BlockId::LockedChest => BlockProperties::solid(0.0),
        BlockId::Trapdoor => BlockProperties::solid(0.0),
        BlockId::Netherrack => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.4)
        },
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

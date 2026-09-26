use crate::block::definition::BlockDefinition;
use crate::block::definition::BlockProperties;
use crate::block::id::Id;

pub(super) struct Definition;
pub(super) static DEFINITION: Definition = Definition;

impl BlockDefinition for Definition {
    fn in_world(&self, id: Id) -> bool {
        matches!(
            id,
            Id::Air
                | Id::Stone
                | Id::Grass
                | Id::Dirt
                | Id::Cobblestone
                | Id::WoodenPlanks
                | Id::Bedrock
                | Id::Sand
                | Id::Gravel
                | Id::Sponge
                | Id::Sandstone
                | Id::NoteBlock
                | Id::Wool
                | Id::GoldBlock
                | Id::IronBlock
                | Id::DoubleStoneSlab
                | Id::Bricks
                | Id::Tnt
                | Id::Bookshelf
                | Id::MossyCobblestone
                | Id::Obsidian
                | Id::MobSpawner
                | Id::DiamondBlock
                | Id::CraftingTable
                | Id::Farmland
                | Id::SnowLayer
                | Id::Ice
                | Id::Snow
                | Id::Clay
                | Id::Jukebox
                | Id::Netherrack
        )
    }

    fn properties(&self, id: Id) -> BlockProperties {
        properties(id)
    }

    fn opaque_cube(&self, id: Id) -> bool {
        !matches!(
            id,
            Id::Air | Id::MobSpawner | Id::Ice | Id::SnowLayer | Id::Farmland
        )
    }

    fn light_opacity(&self, id: Id) -> u8 {
        match id {
            Id::Air | Id::SnowLayer => 0,
            Id::Ice => 3,
            _ => 15,
        }
    }
}

fn properties(id: Id) -> BlockProperties {
    match id {
        Id::Air => BlockProperties::fluid(0.0),
        Id::Stone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(1.5)
        },
        Id::Grass => BlockProperties::solid(0.6),
        Id::Dirt => BlockProperties::solid(0.5),
        Id::Cobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Id::WoodenPlanks => BlockProperties::solid(2.0),
        Id::Bedrock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(-1.0)
        },
        Id::Sand => BlockProperties::solid(0.5),
        Id::Gravel => BlockProperties::solid(0.6),
        Id::Sponge => BlockProperties::solid(0.6),
        Id::Glass => BlockProperties::solid(0.0),
        Id::Sandstone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.8)
        },
        Id::NoteBlock => BlockProperties::solid(0.8),
        Id::Wool => BlockProperties::solid(0.8),
        Id::GoldBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(3.0)
        },
        Id::IronBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        Id::DoubleStoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Id::StoneSlab => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::Bricks => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Id::Tnt => BlockProperties::solid(0.0),
        Id::Bookshelf => BlockProperties::solid(1.5),
        Id::MossyCobblestone => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(2.0)
        },
        Id::Obsidian => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(10.0)
        },
        Id::MobSpawner => BlockProperties {
            opaque_cube: false,
            ..BlockProperties::solid(0.0)
        },
        Id::WoodenStairs => BlockProperties::solid(0.0),
        Id::CobblestoneStairs => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.0)
        },
        Id::DiamondBlock => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(5.0)
        },
        Id::CraftingTable => BlockProperties::solid(2.5),
        Id::Farmland => BlockProperties {
            opaque_cube: false,
            selection_bounds: ([0.0; 3], [1.0, 15.0 / 16.0, 1.0]),
            ..BlockProperties::solid(0.6)
        },
        Id::SnowLayer => BlockProperties {
            opaque_cube: false,
            collision_bounds: Some(([0.0; 3], [1.0, 0.125, 1.0])),
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            light_opacity: 0,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.1)
        },
        Id::Ice => BlockProperties {
            opaque_cube: false,
            slipperiness: 0.98,
            light_opacity: 3,
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.5)
        },
        Id::Snow => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.2)
        },
        Id::Clay => BlockProperties::solid(0.6),
        Id::Jukebox => BlockProperties::solid(2.0),
        Id::Fence => BlockProperties::solid(0.0),
        Id::SoulSand => BlockProperties::solid(0.0),
        Id::Cake => BlockProperties::solid(0.0),
        Id::LockedChest => BlockProperties::solid(0.0),
        Id::Trapdoor => BlockProperties::solid(0.0),
        Id::Netherrack => BlockProperties {
            harvestable_by_hand: false,
            ..BlockProperties::solid(0.4)
        },
        Id::Unknown(_) => BlockProperties::unknown(),
        _ => BlockProperties::unknown(),
    }
}

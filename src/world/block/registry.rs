//! Block identities from the Beta reference, independent of native chunk storage.
use super::block::BlockId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BetaBlockId(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockDefinition {
    pub id: BetaBlockId,
    pub name: &'static str,
}

/// Species, color, and pose are metadata, not additional block IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BetaBlockState {
    pub id: BetaBlockId,
    pub metadata: u8,
}

macro_rules! blocks {
    ($(($constant:ident, $id:literal, $name:literal)),* $(,)?) => {
        impl BetaBlockId { $(pub const $constant: Self = Self($id);)* }
        pub const BLOCK_DEFINITIONS: &[BlockDefinition] = &[
            $(BlockDefinition { id: BetaBlockId::$constant, name: $name },)*
        ];
    };
}

blocks! {
    (AIR, 0, "air"),
    (STONE, 1, "stone"),
    (GRASS, 2, "grass"),
    (DIRT, 3, "dirt"),
    (COBBLESTONE, 4, "cobblestone"),
    (WOODEN_PLANKS, 5, "wooden_planks"),
    (SAPLING, 6, "sapling"),
    (BEDROCK, 7, "bedrock"),
    (FLOWING_WATER, 8, "flowing_water"),
    (WATER, 9, "water"),
    (FLOWING_LAVA, 10, "flowing_lava"),
    (LAVA, 11, "lava"),
    (SAND, 12, "sand"),
    (GRAVEL, 13, "gravel"),
    (GOLD_ORE, 14, "gold_ore"),
    (IRON_ORE, 15, "iron_ore"),
    (COAL_ORE, 16, "coal_ore"),
    (WOOD, 17, "wood"),
    (LEAVES, 18, "leaves"),
    (SPONGE, 19, "sponge"),
    (GLASS, 20, "glass"),
    (LAPIS_ORE, 21, "lapis_ore"),
    (LAPIS_BLOCK, 22, "lapis_block"),
    (DISPENSER, 23, "dispenser"),
    (SANDSTONE, 24, "sandstone"),
    (NOTE_BLOCK, 25, "note_block"),
    (BED, 26, "bed"),
    (POWERED_RAIL, 27, "powered_rail"),
    (DETECTOR_RAIL, 28, "detector_rail"),
    (STICKY_PISTON, 29, "sticky_piston"),
    (COBWEB, 30, "cobweb"),
    (TALL_GRASS, 31, "tall_grass"),
    (DEAD_BUSH, 32, "dead_bush"),
    (PISTON, 33, "piston"),
    (PISTON_HEAD, 34, "piston_head"),
    (WOOL, 35, "wool"),
    (MOVING_PISTON, 36, "moving_piston"),
    (DANDELION, 37, "dandelion"),
    (ROSE, 38, "rose"),
    (BROWN_MUSHROOM, 39, "brown_mushroom"),
    (RED_MUSHROOM, 40, "red_mushroom"),
    (GOLD_BLOCK, 41, "gold_block"),
    (IRON_BLOCK, 42, "iron_block"),
    (DOUBLE_STONE_SLAB, 43, "double_stone_slab"),
    (STONE_SLAB, 44, "stone_slab"),
    (BRICKS, 45, "bricks"),
    (TNT, 46, "tnt"),
    (BOOKSHELF, 47, "bookshelf"),
    (MOSSY_COBBLESTONE, 48, "mossy_cobblestone"),
    (OBSIDIAN, 49, "obsidian"),
    (TORCH, 50, "torch"),
    (FIRE, 51, "fire"),
    (MOB_SPAWNER, 52, "mob_spawner"),
    (WOODEN_STAIRS, 53, "wooden_stairs"),
    (CHEST, 54, "chest"),
    (REDSTONE_WIRE, 55, "redstone_wire"),
    (DIAMOND_ORE, 56, "diamond_ore"),
    (DIAMOND_BLOCK, 57, "diamond_block"),
    (CRAFTING_TABLE, 58, "crafting_table"),
    (CROPS, 59, "crops"),
    (FARMLAND, 60, "farmland"),
    (FURNACE, 61, "furnace"),
    (LIT_FURNACE, 62, "lit_furnace"),
    (STANDING_SIGN, 63, "standing_sign"),
    (WOODEN_DOOR, 64, "wooden_door"),
    (LADDER, 65, "ladder"),
    (RAIL, 66, "rail"),
    (COBBLESTONE_STAIRS, 67, "cobblestone_stairs"),
    (WALL_SIGN, 68, "wall_sign"),
    (LEVER, 69, "lever"),
    (STONE_PRESSURE_PLATE, 70, "stone_pressure_plate"),
    (IRON_DOOR, 71, "iron_door"),
    (WOODEN_PRESSURE_PLATE, 72, "wooden_pressure_plate"),
    (REDSTONE_ORE, 73, "redstone_ore"),
    (LIT_REDSTONE_ORE, 74, "lit_redstone_ore"),
    (UNLIT_REDSTONE_TORCH, 75, "unlit_redstone_torch"),
    (REDSTONE_TORCH, 76, "redstone_torch"),
    (STONE_BUTTON, 77, "stone_button"),
    (SNOW_LAYER, 78, "snow_layer"),
    (ICE, 79, "ice"),
    (SNOW, 80, "snow"),
    (CACTUS, 81, "cactus"),
    (CLAY, 82, "clay"),
    (SUGAR_CANE, 83, "sugar_cane"),
    (JUKEBOX, 84, "jukebox"),
    (FENCE, 85, "fence"),
    (PUMPKIN, 86, "pumpkin"),
    (NETHERRACK, 87, "netherrack"),
    (SOUL_SAND, 88, "soul_sand"),
    (GLOWSTONE, 89, "glowstone"),
    (NETHER_PORTAL, 90, "nether_portal"),
    (JACK_OLANTERN, 91, "jack_olantern"),
    (CAKE, 92, "cake"),
    (REPEATER, 93, "repeater"),
    (POWERED_REPEATER, 94, "powered_repeater"),
    (LOCKED_CHEST, 95, "locked_chest"),
    (TRAPDOOR, 96, "trapdoor"),
}

impl BetaBlockId {
    pub const fn as_u8(self) -> u8 {
        self.0
    }
    pub fn from_u8(value: u8) -> Option<Self> {
        block_definition(value).map(|definition| definition.id)
    }
    pub fn definition(self) -> &'static BlockDefinition {
        &BLOCK_DEFINITIONS[self.0 as usize]
    }
}

pub fn block_definition(id: u8) -> Option<&'static BlockDefinition> {
    BLOCK_DEFINITIONS.get(id as usize)
}

impl BlockId {
    pub fn beta_state(self) -> BetaBlockState {
        let (id, metadata) = match self {
            Self::SpruceWood => (BetaBlockId::WOOD, 1),
            Self::BirchWood => (BetaBlockId::WOOD, 2),
            Self::SpruceLeaves => (BetaBlockId::LEAVES, 1),
            Self::BirchLeaves => (BetaBlockId::LEAVES, 2),
            Self::Torch => (BetaBlockId::TORCH, 5),
            Self::TorchWest => (BetaBlockId::TORCH, 1),
            Self::TorchEast => (BetaBlockId::TORCH, 2),
            Self::TorchNorth => (BetaBlockId::TORCH, 3),
            Self::TorchSouth => (BetaBlockId::TORCH, 4),
            block => (
                BetaBlockId::from_u8(block.as_u8()).expect("registered native block"),
                0,
            ),
        };
        BetaBlockState { id, metadata }
    }
}

impl BetaBlockState {
    /// Returns None for blocks or metadata not implemented by our simulation.
    pub fn runtime_block(self) -> Option<BlockId> {
        match (self.id, self.metadata) {
            (BetaBlockId::WOOD, 1) => Some(BlockId::SpruceWood),
            (BetaBlockId::WOOD, 2) => Some(BlockId::BirchWood),
            (BetaBlockId::LEAVES, 1) => Some(BlockId::SpruceLeaves),
            (BetaBlockId::LEAVES, 2) => Some(BlockId::BirchLeaves),
            (BetaBlockId::TORCH, 0 | 5) => Some(BlockId::Torch),
            (BetaBlockId::TORCH, 1) => Some(BlockId::TorchWest),
            (BetaBlockId::TORCH, 2) => Some(BlockId::TorchEast),
            (BetaBlockId::TORCH, 3) => Some(BlockId::TorchNorth),
            (BetaBlockId::TORCH, 4) => Some(BlockId::TorchSouth),
            (id, 0) if id.as_u8() <= 91 => BlockId::from_u8(id.as_u8()),
            _ => None,
        }
    }
}

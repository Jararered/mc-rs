//! Chunk block identity. Discriminants `0..=96` are the Beta 1.7.3 block ids,
//! including blocks the world does not place yet. `200..` are private chunk
//! values for wood and plank species, plant data, and oriented blocks.
use crate::item::registry::ItemData;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;

/// Horizontal face presented as the front of a furnace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FurnaceFacing {
    #[default]
    North,
    East,
    South,
    West,
}

impl FurnaceFacing {
    /// Mesh face index that points outwards from the furnace front.
    pub const fn face_index(self) -> usize {
        match self {
            Self::East => 2,
            Self::West => 3,
            Self::South => 4,
            Self::North => 5,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromPrimitive, IntoPrimitive)]
pub enum BlockId {
    Air = 0,
    Stone = 1,
    Grass = 2,
    Dirt = 3,
    Cobblestone = 4,
    WoodenPlanks = 5,
    Sapling = 6,
    Bedrock = 7,
    FlowingWater = 8,
    Water = 9,
    FlowingLava = 10,
    Lava = 11,
    Sand = 12,
    Gravel = 13,
    GoldOre = 14,
    IronOre = 15,
    CoalOre = 16,
    Wood = 17,
    Leaves = 18,
    Sponge = 19,
    Glass = 20,
    LapisOre = 21,
    LapisBlock = 22,
    Dispenser = 23,
    Sandstone = 24,
    NoteBlock = 25,
    Bed = 26,
    PoweredRail = 27,
    DetectorRail = 28,
    StickyPiston = 29,
    Cobweb = 30,
    TallGrass = 31,
    DeadBush = 32,
    Piston = 33,
    PistonHead = 34,
    Wool = 35,
    MovingPiston = 36,
    Dandelion = 37,
    Rose = 38,
    BrownMushroom = 39,
    RedMushroom = 40,
    GoldBlock = 41,
    IronBlock = 42,
    DoubleStoneSlab = 43,
    StoneSlab = 44,
    Bricks = 45,
    Tnt = 46,
    Bookshelf = 47,
    MossyCobblestone = 48,
    Obsidian = 49,
    Torch = 50,
    Fire = 51,
    MobSpawner = 52,
    WoodenStairs = 53,
    Chest = 54,
    RedstoneWire = 55,
    DiamondOre = 56,
    DiamondBlock = 57,
    CraftingTable = 58,
    Crops = 59,
    Farmland = 60,
    Furnace = 61,
    LitFurnace = 62,
    StandingSign = 63,
    WoodenDoor = 64,
    Ladder = 65,
    Rail = 66,
    CobblestoneStairs = 67,
    WallSign = 68,
    Lever = 69,
    StonePressurePlate = 70,
    IronDoor = 71,
    WoodenPressurePlate = 72,
    RedstoneOre = 73,
    LitRedstoneOre = 74,
    UnlitRedstoneTorch = 75,
    RedstoneTorch = 76,
    StoneButton = 77,
    SnowLayer = 78,
    Ice = 79,
    Snow = 80,
    Cactus = 81,
    Clay = 82,
    SugarCane = 83,
    Jukebox = 84,
    Fence = 85,
    Pumpkin = 86,
    Netherrack = 87,
    SoulSand = 88,
    Glowstone = 89,
    NetherPortal = 90,
    JackOLantern = 91,
    // 92..=96 share numeric values with legacy chunk bytes. Decode maps
    // those bytes to species and torch facings before `from_u8`.
    Cake = 92,
    Repeater = 93,
    PoweredRepeater = 94,
    LockedChest = 95,
    Trapdoor = 96,
    // Beta stores wood and leaf species in block metadata. The chunk
    // stores one value per block, so each species has its own variant.
    SpruceLeaves = 200,
    BirchLeaves = 201,
    SpruceWood = 202,
    BirchWood = 203,
    // Wall attachment is a compact block value until chunk metadata
    // exists. These discriminants belong to this save format.
    TorchWest = 204,
    TorchEast = 205,
    TorchNorth = 206,
    TorchSouth = 207,
    /// Tall grass metadata 2. Beta stores this on the tall-grass block; chunks
    /// have no metadata, so fern is its own value, like birch wood.
    Fern = 208,
    FurnaceNorth = 209,
    FurnaceEast = 210,
    FurnaceSouth = 211,
    FurnaceWest = 212,
    LitFurnaceNorth = 213,
    LitFurnaceEast = 214,
    LitFurnaceSouth = 215,
    LitFurnaceWest = 216,
    SprucePlanks = 217,
    BirchPlanks = 218,
    PumpkinNorth = 219,
    PumpkinEast = 220,
    PumpkinSouth = 221,
    PumpkinWest = 222,
    ChestNorth = 223,
    ChestEast = 224,
    ChestSouth = 225,
    ChestWest = 226,
    LadderNorth = 227,
    LadderEast = 228,
    LadderSouth = 229,
    LadderWest = 230,
    #[num_enum(catch_all)]
    Unknown(u8),
}

impl BlockId {
    /// Highest block ID that can be used directly as an item ID.
    pub const MAX_ITEM_ID: u8 = 96;

    pub fn as_u8(self) -> u8 {
        self.into()
    }

    pub fn has_item_id(self) -> bool {
        let raw: u8 = self.into();
        raw != 0 && raw <= Self::MAX_ITEM_ID && !matches!(self, Self::Unknown(_))
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match Self::from(value) {
            Self::Unknown(_) => None,
            block => Some(block),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Air => "air",
            Self::Stone => "stone",
            Self::Grass => "grass",
            Self::Dirt => "dirt",
            Self::Cobblestone => "cobblestone",
            Self::WoodenPlanks => "wooden_planks",
            Self::Sapling => "sapling",
            Self::Bedrock => "bedrock",
            Self::FlowingWater => "flowing_water",
            Self::Water => "water",
            Self::FlowingLava => "flowing_lava",
            Self::Lava => "lava",
            Self::Sand => "sand",
            Self::Gravel => "gravel",
            Self::GoldOre => "gold_ore",
            Self::IronOre => "iron_ore",
            Self::CoalOre => "coal_ore",
            Self::Wood => "wood",
            Self::Leaves => "leaves",
            Self::Sponge => "sponge",
            Self::Glass => "glass",
            Self::LapisOre => "lapis_ore",
            Self::LapisBlock => "lapis_block",
            Self::Dispenser => "dispenser",
            Self::Sandstone => "sandstone",
            Self::NoteBlock => "note_block",
            Self::Bed => "bed",
            Self::PoweredRail => "powered_rail",
            Self::DetectorRail => "detector_rail",
            Self::StickyPiston => "sticky_piston",
            Self::Cobweb => "cobweb",
            Self::TallGrass => "tall_grass",
            Self::DeadBush => "dead_bush",
            Self::Piston => "piston",
            Self::PistonHead => "piston_head",
            Self::Wool => "wool",
            Self::MovingPiston => "moving_piston",
            Self::Dandelion => "dandelion",
            Self::Rose => "rose",
            Self::BrownMushroom => "brown_mushroom",
            Self::RedMushroom => "red_mushroom",
            Self::GoldBlock => "gold_block",
            Self::IronBlock => "iron_block",
            Self::DoubleStoneSlab => "double_stone_slab",
            Self::StoneSlab => "stone_slab",
            Self::Bricks => "bricks",
            Self::Tnt => "tnt",
            Self::Bookshelf => "bookshelf",
            Self::MossyCobblestone => "mossy_cobblestone",
            Self::Obsidian => "obsidian",
            Self::Torch => "torch",
            Self::Fire => "fire",
            Self::MobSpawner => "mob_spawner",
            Self::WoodenStairs => "wooden_stairs",
            Self::Chest => "chest",
            Self::ChestNorth => "chest_north",
            Self::ChestEast => "chest_east",
            Self::ChestSouth => "chest_south",
            Self::ChestWest => "chest_west",
            Self::LadderNorth => "ladder_north",
            Self::LadderEast => "ladder_east",
            Self::LadderSouth => "ladder_south",
            Self::LadderWest => "ladder_west",
            Self::RedstoneWire => "redstone_wire",
            Self::DiamondOre => "diamond_ore",
            Self::DiamondBlock => "diamond_block",
            Self::CraftingTable => "crafting_table",
            Self::Crops => "crops",
            Self::Farmland => "farmland",
            Self::Furnace => "furnace",
            Self::LitFurnace => "lit_furnace",
            Self::StandingSign => "standing_sign",
            Self::WoodenDoor => "wooden_door",
            Self::Ladder => "ladder",
            Self::Rail => "rail",
            Self::CobblestoneStairs => "cobblestone_stairs",
            Self::WallSign => "wall_sign",
            Self::Lever => "lever",
            Self::StonePressurePlate => "stone_pressure_plate",
            Self::IronDoor => "iron_door",
            Self::WoodenPressurePlate => "wooden_pressure_plate",
            Self::RedstoneOre => "redstone_ore",
            Self::LitRedstoneOre => "lit_redstone_ore",
            Self::UnlitRedstoneTorch => "unlit_redstone_torch",
            Self::RedstoneTorch => "redstone_torch",
            Self::StoneButton => "stone_button",
            Self::SnowLayer => "snow_layer",
            Self::Ice => "ice",
            Self::Snow => "snow",
            Self::Cactus => "cactus",
            Self::Clay => "clay",
            Self::SugarCane => "sugar_cane",
            Self::Jukebox => "jukebox",
            Self::Fence => "fence",
            Self::Pumpkin => "pumpkin",
            Self::Netherrack => "netherrack",
            Self::SoulSand => "soul_sand",
            Self::Glowstone => "glowstone",
            Self::NetherPortal => "nether_portal",
            Self::JackOLantern => "jack_olantern",
            Self::Cake => "cake",
            Self::Repeater => "repeater",
            Self::PoweredRepeater => "powered_repeater",
            Self::LockedChest => "locked_chest",
            Self::Trapdoor => "trapdoor",
            Self::SpruceLeaves => "spruce_leaves",
            Self::BirchLeaves => "birch_leaves",
            Self::SpruceWood => "spruce_wood",
            Self::BirchWood => "birch_wood",
            Self::SprucePlanks => "spruce_planks",
            Self::BirchPlanks => "birch_planks",
            Self::PumpkinNorth => "pumpkin_north",
            Self::PumpkinEast => "pumpkin_east",
            Self::PumpkinSouth => "pumpkin_south",
            Self::PumpkinWest => "pumpkin_west",
            Self::TorchWest => "torch_west",
            Self::TorchEast => "torch_east",
            Self::TorchNorth => "torch_north",
            Self::TorchSouth => "torch_south",
            Self::Fern => "fern",
            Self::FurnaceNorth => "furnace_north",
            Self::FurnaceEast => "furnace_east",
            Self::FurnaceSouth => "furnace_south",
            Self::FurnaceWest => "furnace_west",
            Self::LitFurnaceNorth => "lit_furnace_north",
            Self::LitFurnaceEast => "lit_furnace_east",
            Self::LitFurnaceSouth => "lit_furnace_south",
            Self::LitFurnaceWest => "lit_furnace_west",
            Self::Unknown(_) => "unknown",
        }
    }

    /// Blocks the simulation generates, meshes, and saves. Catalog-only values
    /// such as glass and cake are inventory identities.
    pub const fn in_world(self) -> bool {
        matches!(
            self,
            Self::Air
                | Self::Stone
                | Self::Grass
                | Self::Dirt
                | Self::Farmland
                | Self::Cobblestone
                | Self::WoodenPlanks
                | Self::Bedrock
                | Self::FlowingWater
                | Self::Water
                | Self::FlowingLava
                | Self::Lava
                | Self::Sand
                | Self::Gravel
                | Self::GoldOre
                | Self::IronOre
                | Self::CoalOre
                | Self::Wood
                | Self::Leaves
                | Self::Sponge
                | Self::LapisOre
                | Self::LapisBlock
                | Self::Dispenser
                | Self::Sandstone
                | Self::NoteBlock
                | Self::Wool
                | Self::GoldBlock
                | Self::IronBlock
                | Self::DoubleStoneSlab
                | Self::Bricks
                | Self::Tnt
                | Self::Bookshelf
                | Self::MossyCobblestone
                | Self::MobSpawner
                | Self::Chest
                | Self::ChestNorth
                | Self::ChestEast
                | Self::ChestSouth
                | Self::ChestWest
                | Self::Ladder
                | Self::LadderNorth
                | Self::LadderEast
                | Self::LadderSouth
                | Self::LadderWest
                | Self::Obsidian
                | Self::DeadBush
                | Self::Torch
                | Self::TallGrass
                | Self::Dandelion
                | Self::Rose
                | Self::BrownMushroom
                | Self::RedMushroom
                | Self::Fern
                | Self::Cactus
                | Self::SugarCane
                | Self::FurnaceNorth
                | Self::FurnaceEast
                | Self::FurnaceSouth
                | Self::FurnaceWest
                | Self::LitFurnaceNorth
                | Self::LitFurnaceEast
                | Self::LitFurnaceSouth
                | Self::LitFurnaceWest
                | Self::DiamondOre
                | Self::DiamondBlock
                | Self::CraftingTable
                | Self::Furnace
                | Self::LitFurnace
                | Self::RedstoneOre
                | Self::LitRedstoneOre
                | Self::Ice
                | Self::Snow
                | Self::Clay
                | Self::Jukebox
                | Self::Pumpkin
                | Self::PumpkinNorth
                | Self::PumpkinEast
                | Self::PumpkinSouth
                | Self::PumpkinWest
                | Self::Netherrack
                | Self::Glowstone
                | Self::JackOLantern
                | Self::SpruceLeaves
                | Self::BirchLeaves
                | Self::SpruceWood
                | Self::BirchWood
                | Self::SprucePlanks
                | Self::BirchPlanks
                | Self::TorchWest
                | Self::TorchEast
                | Self::TorchNorth
                | Self::TorchSouth
        )
    }

    pub const fn is_furnace(self) -> bool {
        matches!(
            self,
            Self::Furnace
                | Self::LitFurnace
                | Self::FurnaceNorth
                | Self::FurnaceEast
                | Self::FurnaceSouth
                | Self::FurnaceWest
                | Self::LitFurnaceNorth
                | Self::LitFurnaceEast
                | Self::LitFurnaceSouth
                | Self::LitFurnaceWest
        )
    }

    pub const fn is_chest(self) -> bool {
        matches!(
            self,
            Self::Chest | Self::ChestNorth | Self::ChestEast | Self::ChestSouth | Self::ChestWest
        )
    }

    pub const fn chest_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::ChestNorth | Self::Chest => Some(FurnaceFacing::North),
            Self::ChestEast => Some(FurnaceFacing::East),
            Self::ChestSouth => Some(FurnaceFacing::South),
            Self::ChestWest => Some(FurnaceFacing::West),
            _ => None,
        }
    }

    pub const fn with_chest_facing(self, facing: FurnaceFacing) -> Self {
        match facing {
            FurnaceFacing::North => Self::ChestNorth,
            FurnaceFacing::East => Self::ChestEast,
            FurnaceFacing::South => Self::ChestSouth,
            FurnaceFacing::West => Self::ChestWest,
        }
    }

    /// Ladder orientation names the wall the ladder is attached to.
    pub const fn is_ladder(self) -> bool {
        matches!(
            self,
            Self::Ladder
                | Self::LadderNorth
                | Self::LadderEast
                | Self::LadderSouth
                | Self::LadderWest
        )
    }

    /// The offset from this ladder cell to its supporting wall.
    pub const fn ladder_support_offset(self) -> Option<[i32; 3]> {
        match self {
            Self::LadderNorth => Some([0, 0, -1]),
            Self::LadderEast => Some([1, 0, 0]),
            Self::LadderSouth => Some([0, 0, 1]),
            Self::LadderWest => Some([-1, 0, 0]),
            _ => None,
        }
    }

    pub const fn with_ladder_support(self, support: FurnaceFacing) -> Self {
        match support {
            FurnaceFacing::North => Self::LadderNorth,
            FurnaceFacing::East => Self::LadderEast,
            FurnaceFacing::South => Self::LadderSouth,
            FurnaceFacing::West => Self::LadderWest,
        }
    }

    pub const fn is_lit_furnace(self) -> bool {
        matches!(
            self,
            Self::LitFurnace
                | Self::LitFurnaceNorth
                | Self::LitFurnaceEast
                | Self::LitFurnaceSouth
                | Self::LitFurnaceWest
        )
    }

    pub const fn furnace_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::FurnaceEast | Self::LitFurnaceEast => Some(FurnaceFacing::East),
            Self::FurnaceSouth | Self::LitFurnaceSouth => Some(FurnaceFacing::South),
            Self::FurnaceWest | Self::LitFurnaceWest => Some(FurnaceFacing::West),
            Self::Furnace | Self::LitFurnace | Self::FurnaceNorth | Self::LitFurnaceNorth => {
                Some(FurnaceFacing::North)
            }
            _ => None,
        }
    }

    pub const fn with_furnace_state(self, facing: FurnaceFacing, lit: bool) -> Self {
        match (facing, lit) {
            (FurnaceFacing::North, false) => Self::FurnaceNorth,
            (FurnaceFacing::East, false) => Self::FurnaceEast,
            (FurnaceFacing::South, false) => Self::FurnaceSouth,
            (FurnaceFacing::West, false) => Self::FurnaceWest,
            (FurnaceFacing::North, true) => Self::LitFurnaceNorth,
            (FurnaceFacing::East, true) => Self::LitFurnaceEast,
            (FurnaceFacing::South, true) => Self::LitFurnaceSouth,
            (FurnaceFacing::West, true) => Self::LitFurnaceWest,
        }
    }

    pub const fn pumpkin_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::PumpkinNorth => Some(FurnaceFacing::North),
            Self::PumpkinEast => Some(FurnaceFacing::East),
            Self::PumpkinSouth => Some(FurnaceFacing::South),
            Self::PumpkinWest | Self::Pumpkin => Some(FurnaceFacing::West),
            _ => None,
        }
    }

    pub const fn with_pumpkin_facing(self, facing: FurnaceFacing) -> Self {
        match facing {
            FurnaceFacing::North => Self::PumpkinNorth,
            FurnaceFacing::East => Self::PumpkinEast,
            FurnaceFacing::South => Self::PumpkinSouth,
            FurnaceFacing::West => Self::PumpkinWest,
        }
    }

    /// Convert Beta pumpkin metadata into the outward-facing side.
    pub const fn pumpkin_from_metadata(metadata: u32) -> Self {
        match metadata & 3 {
            0 => Self::PumpkinWest,
            1 => Self::PumpkinSouth,
            2 => Self::PumpkinEast,
            _ => Self::PumpkinNorth,
        }
    }

    pub fn with_furnace_lit(self, lit: bool) -> Self {
        match self {
            Self::Furnace if lit => Self::LitFurnace,
            Self::LitFurnace if !lit => Self::Furnace,
            Self::FurnaceNorth | Self::LitFurnaceNorth => {
                Self::Furnace.with_furnace_state(FurnaceFacing::North, lit)
            }
            Self::FurnaceEast | Self::LitFurnaceEast => {
                Self::Furnace.with_furnace_state(FurnaceFacing::East, lit)
            }
            Self::FurnaceSouth | Self::LitFurnaceSouth => {
                Self::Furnace.with_furnace_state(FurnaceFacing::South, lit)
            }
            Self::FurnaceWest | Self::LitFurnaceWest => {
                Self::Furnace.with_furnace_state(FurnaceFacing::West, lit)
            }
            block => block,
        }
    }

    /// Stack identity for this chunk block. Species stays in the metadata.
    /// Torch facing is dropped.
    pub const fn item_form(self) -> (Self, u8) {
        match self {
            Self::SpruceWood => (Self::Wood, 1),
            Self::BirchWood => (Self::Wood, 2),
            Self::SprucePlanks => (Self::WoodenPlanks, 1),
            Self::BirchPlanks => (Self::WoodenPlanks, 2),
            Self::SpruceLeaves => (Self::Leaves, 1),
            Self::BirchLeaves => (Self::Leaves, 2),
            Self::Torch
            | Self::TorchWest
            | Self::TorchEast
            | Self::TorchNorth
            | Self::TorchSouth => (Self::Torch, 0),
            Self::Fern => (Self::TallGrass, 2),
            Self::FurnaceNorth
            | Self::FurnaceEast
            | Self::FurnaceSouth
            | Self::FurnaceWest
            | Self::LitFurnaceNorth
            | Self::LitFurnaceEast
            | Self::LitFurnaceSouth
            | Self::LitFurnaceWest
            | Self::LitFurnace => (Self::Furnace, 0),
            Self::PumpkinNorth | Self::PumpkinEast | Self::PumpkinSouth | Self::PumpkinWest => {
                (Self::Pumpkin, 0)
            }
            Self::ChestNorth | Self::ChestEast | Self::ChestSouth | Self::ChestWest => {
                (Self::Chest, 0)
            }
            Self::LadderNorth | Self::LadderEast | Self::LadderSouth | Self::LadderWest => {
                (Self::Ladder, 0)
            }
            block => (block, 0),
        }
    }

    /// Chunk block for a placed stack. `None` for blocks the world does not
    /// simulate, and for metadata it does not implement.
    pub const fn placed(self, metadata: u8) -> Option<Self> {
        match (self, metadata) {
            (Self::Wood, 1) => Some(Self::SpruceWood),
            (Self::Wood, 2) => Some(Self::BirchWood),
            (Self::WoodenPlanks, 1) => Some(Self::SprucePlanks),
            (Self::WoodenPlanks, 2) => Some(Self::BirchPlanks),
            (Self::Leaves, 1) => Some(Self::SpruceLeaves),
            (Self::Leaves, 2) => Some(Self::BirchLeaves),
            (Self::Torch, 0 | 5) => Some(Self::Torch),
            (Self::Torch, 1) => Some(Self::TorchWest),
            (Self::Torch, 2) => Some(Self::TorchEast),
            (Self::Torch, 3) => Some(Self::TorchNorth),
            (Self::Torch, 4) => Some(Self::TorchSouth),
            (Self::TallGrass, 0 | 1) => Some(Self::TallGrass),
            (Self::TallGrass, 2) => Some(Self::Fern),
            (Self::Pumpkin, 0) => Some(Self::Pumpkin),
            (Self::Ladder, 0 | 2) => Some(Self::Ladder),
            (block, 0) if block.in_world() => Some(block),
            _ => None,
        }
    }

    /// Inventory subtype for the direct block item.
    pub const fn item_data(self) -> ItemData {
        match self {
            Self::Sapling | Self::Wood | Self::Leaves | Self::WoodenPlanks => ItemData::Subtype(2),
            Self::Wool => ItemData::Subtype(15),
            Self::StoneSlab => ItemData::Subtype(3),
            _ => ItemData::None,
        }
    }
}

//! Compact chunk block identity. Discriminants `0..=96` retain Beta block ids;
//! private values encode species and oriented states in the current save format.

pub use super::state::FurnaceFacing;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromPrimitive, IntoPrimitive)]
pub enum Block {
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

impl Block {
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

    /// Blocks the simulation generates, meshes, and saves. Catalog-only values
    /// such as glass and cake are inventory identities.
    pub fn in_world(self) -> bool {
        super::definition::definition(self).in_world(self)
    }
}

//! Compact chunk block identity: Beta's block ids. Species (wood, leaves,
//! planks, fern) and orientation (torches, ladders, furnaces, chests,
//! pumpkins) live in the chunk's block metadata, as in Beta.

use crate::block::definition;
use crate::item::registry::ItemData;

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
    Cake = 92,
    Repeater = 93,
    PoweredRepeater = 94,
    LockedChest = 95,
    Trapdoor = 96,
    #[num_enum(catch_all)]
    Unknown(u8),
}

/// Block metadata that selects a species. Wood, planks, and leaves read it as
/// `metadata & 3`; tall grass reads [`FERN`] for the fern.
pub mod species {
    pub const OAK: u8 = 0;
    pub const SPRUCE: u8 = 1;
    pub const BIRCH: u8 = 2;
    pub const FERN: u8 = 2;
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

    pub const fn is_torch(self) -> bool {
        matches!(self, Self::Torch)
    }

    pub const fn is_ladder(self) -> bool {
        matches!(self, Self::Ladder)
    }

    pub const fn is_chest(self) -> bool {
        matches!(self, Self::Chest)
    }

    pub const fn is_leaves(self) -> bool {
        matches!(self, Self::Leaves)
    }

    pub const fn is_furnace(self) -> bool {
        matches!(self, Self::Furnace | Self::LitFurnace)
    }

    /// Local bounds used for picking and the hover outline.
    pub fn selection_bounds(self) -> definition::BlockBounds {
        definition::properties(self).selection_bounds
    }

    /// Whether the player's block selection ray should stop on this block.
    pub fn is_targetable(self) -> bool {
        definition::properties(self).targetable
    }

    /// Whether a placed block may replace this cell.
    pub fn is_replaceable(self) -> bool {
        definition::properties(self).replaceable
    }

    /// Whether the player may mine this block. Bedrock is unbreakable.
    pub fn is_breakable(self) -> bool {
        let properties = definition::properties(self);
        properties.targetable && properties.hardness >= 0.0
    }

    /// Beta `Block.blockHardness`. Negative means unbreakable (`setBlockUnbreakable`).
    pub fn hardness(self) -> f32 {
        definition::properties(self).hardness
    }

    /// Whether this block fully occludes its neighbours.
    pub fn is_opaque_cube(self) -> bool {
        definition::properties(self).opaque_cube
    }

    pub fn blocks_movement(self) -> bool {
        definition::properties(self).blocks_movement
    }

    pub fn slipperiness(self) -> f32 {
        definition::properties(self).slipperiness
    }

    pub fn collision_bounds(self) -> Option<definition::BlockBounds> {
        definition::properties(self).collision_bounds
    }

    pub fn is_crossed_plant(self) -> bool {
        definition::properties(self).crossed_plant
    }

    pub fn light_opacity(self) -> u8 {
        definition::properties(self).light_opacity
    }

    pub fn light_emission(self) -> u8 {
        definition::properties(self).light_emission
    }

    pub fn harvestable_by_hand(self) -> bool {
        definition::properties(self).harvestable_by_hand
    }

    /// Beta's `Material.isSolid` classification.
    pub fn is_solid_material(self) -> bool {
        !matches!(
            self,
            Self::Air
                | Self::Water
                | Self::FlowingWater
                | Self::Lava
                | Self::FlowingLava
                | Self::Torch
                | Self::DeadBush
                | Self::TallGrass
                | Self::Dandelion
                | Self::Rose
                | Self::BrownMushroom
                | Self::RedMushroom
                | Self::Fire
                | Self::RedstoneWire
                | Self::Crops
                | Self::Sapling
                | Self::Rail
                | Self::PoweredRail
                | Self::DetectorRail
                | Self::Ladder
                | Self::Lever
                | Self::UnlitRedstoneTorch
                | Self::RedstoneTorch
                | Self::StoneButton
                | Self::SugarCane
                | Self::SnowLayer
                | Self::Repeater
                | Self::PoweredRepeater
                | Self::NetherPortal
        )
    }

    /// `BlockFlower.canThisPlantGrowOnThisBlockID`.
    pub fn supports_plants(self) -> bool {
        matches!(self, Self::Grass | Self::Dirt | Self::Farmland)
    }

    /// Beta `Block.getExplosionResistance`.
    pub fn explosion_resistance(self) -> f32 {
        let explicit = match self {
            Self::Bedrock => Some(6_000_000.0),
            Self::Obsidian => Some(2000.0),
            Self::Stone
            | Self::Cobblestone
            | Self::GoldBlock
            | Self::IronBlock
            | Self::DoubleStoneSlab
            | Self::StoneSlab
            | Self::Bricks
            | Self::MossyCobblestone
            | Self::DiamondBlock
            | Self::Jukebox
            | Self::CobblestoneStairs => Some(10.0),
            Self::WoodenPlanks
            | Self::GoldOre
            | Self::IronOre
            | Self::CoalOre
            | Self::LapisOre
            | Self::LapisBlock
            | Self::DiamondOre
            | Self::RedstoneOre
            | Self::LitRedstoneOre
            | Self::Fence
            | Self::WoodenStairs => Some(5.0),
            Self::Lava => return 100.0,
            _ => None,
        };
        explicit.map_or_else(|| self.hardness().max(0.0), |resistance| resistance * 0.6)
    }

    /// The part of `metadata` that changes how the block is drawn: the
    /// species, or the facing. Decay flags and the like are not part of it.
    pub const fn appearance_metadata(self, metadata: u8) -> u8 {
        match self {
            Self::Wood | Self::WoodenPlanks | Self::Leaves | Self::TallGrass => metadata & 3,
            Self::Torch
            | Self::Ladder
            | Self::Furnace
            | Self::LitFurnace
            | Self::Chest
            | Self::Pumpkin => metadata,
            _ => 0,
        }
    }

    /// Picking bounds for the block with `metadata`. Only torches and ladders
    /// depend on it.
    pub fn selection_bounds_for(self, metadata: u8) -> definition::BlockBounds {
        match self {
            Self::Torch | Self::Ladder => definition::oriented_bounds(self, metadata),
            _ => self.selection_bounds(),
        }
    }

    /// Collision bounds for the block with `metadata`. Ladders depend on their
    /// facing, and snow layers collide only from three layers up
    /// (`BlockSnow.getCollisionBoundingBoxFromPool`).
    pub fn collision_bounds_for(self, metadata: u8) -> Option<definition::BlockBounds> {
        match self {
            Self::Ladder => Some(definition::oriented_bounds(self, metadata)),
            Self::SnowLayer if metadata & 7 >= 3 => Some(([0.0; 3], [1.0, 0.5, 1.0])),
            _ => self.collision_bounds(),
        }
    }

    /// Stack identity for this block with `metadata`: the block that
    /// represents the item, and its subtype. Species stays in the subtype;
    /// orientation, leaf decay flags, and the lit furnace are dropped.
    pub const fn item_form(self, metadata: u8) -> (Self, u8) {
        match self {
            Self::Wood | Self::WoodenPlanks | Self::Leaves => (self, metadata & 3),
            Self::TallGrass if metadata & 3 == 2 => (self, 2),
            Self::LitFurnace => (Self::Furnace, 0),
            block => (block, 0),
        }
    }

    /// The block and metadata a placed stack of `self` with item `data`
    /// starts as. `None` for blocks the world does not simulate, and for
    /// subtypes it does not implement.
    pub fn placed(self, data: u8) -> Option<(Self, u8)> {
        match (self, data) {
            (Self::Wood | Self::WoodenPlanks | Self::Leaves, 0..=2) => Some((self, data)),
            // Torch data 5 is the floor torch; 1..=4 name a wall side.
            (Self::Torch, 0 | 5) => Some((Self::Torch, 0)),
            (Self::Torch, 1..=4) => Some((Self::Torch, data)),
            // Tall grass item data 0 and 1 both place the plain shrub.
            (Self::TallGrass, 0 | 1) => Some((Self::TallGrass, 0)),
            (Self::TallGrass, 2) => Some((Self::TallGrass, 2)),
            // Ladders place unattached; the placement code picks the wall.
            (Self::Ladder, 0 | 2) => Some((Self::Ladder, 0)),
            (block, 0) => Some((block, 0)),
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

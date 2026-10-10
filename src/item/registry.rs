//! Inventory identities and Beta stack rules. Definitions do not imply that
//! an item's use, crafting recipe, or rendering has been implemented.
use crate::block::blocks::Block;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;
use serde::Deserialize;
use serde::de::IntoDeserializer;
use serde::de::value::Error as NameError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemData {
    None,
    /// Highest supported subtype value (inclusive).
    Subtype(u16),
    /// Beta `maxDamage`. Stored data is uses so far; the item breaks when uses
    /// exceed this value.
    Durability(u16),
    Map,
}

impl ItemData {
    pub fn accepts(self, value: u16) -> bool {
        match self {
            Self::None => value == 0,
            Self::Subtype(max) | Self::Durability(max) => value <= max,
            Self::Map => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemProperties {
    pub item: Item,
    pub max_stack_size: u8,
    pub data: ItemData,
    /// Present for direct block items, not special items such as doors/buckets.
    pub block: Option<Block>,
}

impl ItemProperties {
    /// Beta crafting remainder, for example an empty bucket left by milk.
    pub const fn container_item(self) -> Option<Item> {
        match self.item {
            Item::MilkBucket => Some(Item::Bucket),
            _ => None,
        }
    }
}

/// Block items share their block's Beta id. Standalone items start at 256.
///
/// Standalone variants use their Beta item ids as discriminants. Block item ids
/// are represented by `BlockOrUnknown` and resolved through [`Self::block`].
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, FromPrimitive, IntoPrimitive, Deserialize)]
pub enum Item {
    IronShovel = 256,
    IronPickaxe = 257,
    IronAxe = 258,
    FlintAndSteel = 259,
    Apple = 260,
    Bow = 261,
    Arrow = 262,
    Coal = 263,
    Diamond = 264,
    IronIngot = 265,
    GoldIngot = 266,
    IronSword = 267,
    WoodenSword = 268,
    WoodenShovel = 269,
    WoodenPickaxe = 270,
    WoodenAxe = 271,
    StoneSword = 272,
    StoneShovel = 273,
    StonePickaxe = 274,
    StoneAxe = 275,
    DiamondSword = 276,
    DiamondShovel = 277,
    DiamondPickaxe = 278,
    DiamondAxe = 279,
    Stick = 280,
    Bowl = 281,
    MushroomStew = 282,
    GoldSword = 283,
    GoldShovel = 284,
    GoldPickaxe = 285,
    GoldAxe = 286,
    String = 287,
    Feather = 288,
    Gunpowder = 289,
    WoodenHoe = 290,
    StoneHoe = 291,
    IronHoe = 292,
    DiamondHoe = 293,
    GoldHoe = 294,
    Seeds = 295,
    Wheat = 296,
    Bread = 297,
    LeatherHelmet = 298,
    LeatherChestplate = 299,
    LeatherLeggings = 300,
    LeatherBoots = 301,
    ChainmailHelmet = 302,
    ChainmailChestplate = 303,
    ChainmailLeggings = 304,
    ChainmailBoots = 305,
    IronHelmet = 306,
    IronChestplate = 307,
    IronLeggings = 308,
    IronBoots = 309,
    DiamondHelmet = 310,
    DiamondChestplate = 311,
    DiamondLeggings = 312,
    DiamondBoots = 313,
    GoldHelmet = 314,
    GoldChestplate = 315,
    GoldLeggings = 316,
    GoldBoots = 317,
    Flint = 318,
    RawPorkchop = 319,
    CookedPorkchop = 320,
    Painting = 321,
    GoldenApple = 322,
    Sign = 323,
    WoodenDoor = 324,
    Bucket = 325,
    WaterBucket = 326,
    LavaBucket = 327,
    Minecart = 328,
    Saddle = 329,
    IronDoor = 330,
    Redstone = 331,
    Snowball = 332,
    Boat = 333,
    Leather = 334,
    MilkBucket = 335,
    Brick = 336,
    ClayBall = 337,
    SugarCane = 338,
    Paper = 339,
    Book = 340,
    Slimeball = 341,
    ChestMinecart = 342,
    FurnaceMinecart = 343,
    Egg = 344,
    Compass = 345,
    FishingRod = 346,
    Clock = 347,
    GlowstoneDust = 348,
    RawFish = 349,
    CookedFish = 350,
    Dye = 351,
    Bone = 352,
    Sugar = 353,
    Cake = 354,
    Bed = 355,
    Repeater = 356,
    Cookie = 357,
    Map = 358,
    Shears = 359,
    Record13 = 2256,
    RecordCat = 2257,
    #[num_enum(catch_all)]
    #[serde(skip)]
    BlockOrUnknown(u16),
}
const fn standalone(item: Item, max_stack_size: u8, data: ItemData) -> ItemProperties {
    ItemProperties {
        item,
        max_stack_size,
        data,
        block: None,
    }
}

impl Item {
    pub fn from_block(block: Block) -> Option<Self> {
        block
            .has_item_id()
            .then(|| Self::from(u16::from(block.as_u8())))
    }

    /// The item a data file names: a standalone item's variant, or else a
    /// block's, as [`Display`](std::fmt::Display) writes them.
    pub fn named(name: &str) -> Option<Self> {
        let name = || IntoDeserializer::<NameError>::into_deserializer(name);
        Self::deserialize(name())
            .ok()
            .or_else(|| Self::from_block(Block::deserialize(name()).ok()?))
    }

    pub fn as_u16(self) -> u16 {
        self.into()
    }

    pub fn from_u16(value: u16) -> Option<Self> {
        match Self::from(value) {
            item @ Self::BlockOrUnknown(_) if item.block().is_some() => Some(item),
            Self::BlockOrUnknown(_) => None,
            item => Some(item),
        }
    }

    pub fn block(self) -> Option<Block> {
        let Self::BlockOrUnknown(raw) = self else {
            return None;
        };
        let block = Block::from_u8(u8::try_from(raw).ok()?)?;
        block.has_item_id().then_some(block)
    }

    /// `ItemFood.healAmount`: half-hearts restored when eaten.
    pub fn heal_amount(self) -> Option<u8> {
        Some(match self {
            Self::Apple => 4,
            Self::Bread => 5,
            Self::RawPorkchop => 3,
            Self::CookedPorkchop => 8,
            Self::GoldenApple => 42,
            Self::RawFish => 2,
            Self::CookedFish => 5,
            Self::MushroomStew => 10,
            Self::Cookie => 1,
            _ => return None,
        })
    }

    /// `SlotArmor.isItemValid`: the armor slot this item is worn in, counting
    /// helmet, chestplate, leggings, boots. A pumpkin is worn as a helmet.
    pub fn armor_slot(self) -> Option<usize> {
        use Item as I;
        Some(match self {
            I::LeatherHelmet
            | I::ChainmailHelmet
            | I::IronHelmet
            | I::DiamondHelmet
            | I::GoldHelmet => 0,
            I::LeatherChestplate
            | I::ChainmailChestplate
            | I::IronChestplate
            | I::DiamondChestplate
            | I::GoldChestplate => 1,
            I::LeatherLeggings
            | I::ChainmailLeggings
            | I::IronLeggings
            | I::DiamondLeggings
            | I::GoldLeggings => 2,
            I::LeatherBoots | I::ChainmailBoots | I::IronBoots | I::DiamondBoots | I::GoldBoots => {
                3
            }
            _ if self.block() == Some(Block::Pumpkin) => 0,
            _ => return None,
        })
    }

    pub fn properties(self) -> Option<ItemProperties> {
        if let Some(block) = self.block() {
            return Some(ItemProperties {
                item: self,
                max_stack_size: 64,
                data: block.item_data(),
                block: Some(block),
            });
        }
        match self {
            Self::IronShovel => Some(standalone(self, 1, ItemData::Durability(250))),
            Self::IronPickaxe => Some(standalone(self, 1, ItemData::Durability(250))),
            Self::IronAxe => Some(standalone(self, 1, ItemData::Durability(250))),
            Self::FlintAndSteel => Some(standalone(self, 1, ItemData::Durability(64))),
            Self::Apple => Some(standalone(self, 1, ItemData::None)),
            Self::Bow => Some(standalone(self, 1, ItemData::None)),
            Self::Arrow => Some(standalone(self, 64, ItemData::None)),
            Self::Coal => Some(standalone(self, 64, ItemData::Subtype(1))),
            Self::Diamond => Some(standalone(self, 64, ItemData::None)),
            Self::IronIngot => Some(standalone(self, 64, ItemData::None)),
            Self::GoldIngot => Some(standalone(self, 64, ItemData::None)),
            Self::IronSword => Some(standalone(self, 1, ItemData::Durability(250))),
            Self::WoodenSword => Some(standalone(self, 1, ItemData::Durability(59))),
            Self::WoodenShovel => Some(standalone(self, 1, ItemData::Durability(59))),
            Self::WoodenPickaxe => Some(standalone(self, 1, ItemData::Durability(59))),
            Self::WoodenAxe => Some(standalone(self, 1, ItemData::Durability(59))),
            Self::StoneSword => Some(standalone(self, 1, ItemData::Durability(131))),
            Self::StoneShovel => Some(standalone(self, 1, ItemData::Durability(131))),
            Self::StonePickaxe => Some(standalone(self, 1, ItemData::Durability(131))),
            Self::StoneAxe => Some(standalone(self, 1, ItemData::Durability(131))),
            Self::DiamondSword => Some(standalone(self, 1, ItemData::Durability(1561))),
            Self::DiamondShovel => Some(standalone(self, 1, ItemData::Durability(1561))),
            Self::DiamondPickaxe => Some(standalone(self, 1, ItemData::Durability(1561))),
            Self::DiamondAxe => Some(standalone(self, 1, ItemData::Durability(1561))),
            Self::Stick => Some(standalone(self, 64, ItemData::None)),
            Self::Bowl => Some(standalone(self, 64, ItemData::None)),
            Self::MushroomStew => Some(standalone(self, 1, ItemData::None)),
            Self::GoldSword => Some(standalone(self, 1, ItemData::Durability(32))),
            Self::GoldShovel => Some(standalone(self, 1, ItemData::Durability(32))),
            Self::GoldPickaxe => Some(standalone(self, 1, ItemData::Durability(32))),
            Self::GoldAxe => Some(standalone(self, 1, ItemData::Durability(32))),
            Self::String => Some(standalone(self, 64, ItemData::None)),
            Self::Feather => Some(standalone(self, 64, ItemData::None)),
            Self::Gunpowder => Some(standalone(self, 64, ItemData::None)),
            Self::WoodenHoe => Some(standalone(self, 1, ItemData::Durability(59))),
            Self::StoneHoe => Some(standalone(self, 1, ItemData::Durability(131))),
            Self::IronHoe => Some(standalone(self, 1, ItemData::Durability(250))),
            Self::DiamondHoe => Some(standalone(self, 1, ItemData::Durability(1561))),
            Self::GoldHoe => Some(standalone(self, 1, ItemData::Durability(32))),
            Self::Seeds => Some(standalone(self, 64, ItemData::None)),
            Self::Wheat => Some(standalone(self, 64, ItemData::None)),
            Self::Bread => Some(standalone(self, 1, ItemData::None)),
            Self::LeatherHelmet => Some(standalone(self, 1, ItemData::Durability(33))),
            Self::LeatherChestplate => Some(standalone(self, 1, ItemData::Durability(48))),
            Self::LeatherLeggings => Some(standalone(self, 1, ItemData::Durability(45))),
            Self::LeatherBoots => Some(standalone(self, 1, ItemData::Durability(39))),
            Self::ChainmailHelmet => Some(standalone(self, 1, ItemData::Durability(66))),
            Self::ChainmailChestplate => Some(standalone(self, 1, ItemData::Durability(96))),
            Self::ChainmailLeggings => Some(standalone(self, 1, ItemData::Durability(90))),
            Self::ChainmailBoots => Some(standalone(self, 1, ItemData::Durability(78))),
            Self::IronHelmet => Some(standalone(self, 1, ItemData::Durability(132))),
            Self::IronChestplate => Some(standalone(self, 1, ItemData::Durability(192))),
            Self::IronLeggings => Some(standalone(self, 1, ItemData::Durability(180))),
            Self::IronBoots => Some(standalone(self, 1, ItemData::Durability(156))),
            Self::DiamondHelmet => Some(standalone(self, 1, ItemData::Durability(264))),
            Self::DiamondChestplate => Some(standalone(self, 1, ItemData::Durability(384))),
            Self::DiamondLeggings => Some(standalone(self, 1, ItemData::Durability(360))),
            Self::DiamondBoots => Some(standalone(self, 1, ItemData::Durability(312))),
            Self::GoldHelmet => Some(standalone(self, 1, ItemData::Durability(66))),
            Self::GoldChestplate => Some(standalone(self, 1, ItemData::Durability(96))),
            Self::GoldLeggings => Some(standalone(self, 1, ItemData::Durability(90))),
            Self::GoldBoots => Some(standalone(self, 1, ItemData::Durability(78))),
            Self::Flint => Some(standalone(self, 64, ItemData::None)),
            Self::RawPorkchop => Some(standalone(self, 1, ItemData::None)),
            Self::CookedPorkchop => Some(standalone(self, 1, ItemData::None)),
            Self::Painting => Some(standalone(self, 64, ItemData::None)),
            Self::GoldenApple => Some(standalone(self, 1, ItemData::None)),
            Self::Sign => Some(standalone(self, 1, ItemData::None)),
            Self::WoodenDoor => Some(standalone(self, 1, ItemData::None)),
            Self::Bucket => Some(standalone(self, 1, ItemData::None)),
            Self::WaterBucket => Some(standalone(self, 1, ItemData::None)),
            Self::LavaBucket => Some(standalone(self, 1, ItemData::None)),
            Self::Minecart => Some(standalone(self, 1, ItemData::None)),
            Self::Saddle => Some(standalone(self, 1, ItemData::None)),
            Self::IronDoor => Some(standalone(self, 1, ItemData::None)),
            Self::Redstone => Some(ItemProperties {
                block: Some(Block::RedstoneWire),
                ..standalone(self, 64, ItemData::None)
            }),
            Self::Snowball => Some(standalone(self, 16, ItemData::None)),
            Self::Boat => Some(standalone(self, 1, ItemData::None)),
            Self::Leather => Some(standalone(self, 64, ItemData::None)),
            Self::MilkBucket => Some(standalone(self, 1, ItemData::None)),
            Self::Brick => Some(standalone(self, 64, ItemData::None)),
            Self::ClayBall => Some(standalone(self, 64, ItemData::None)),
            Self::SugarCane => Some(ItemProperties {
                block: Some(Block::SugarCane),
                ..standalone(self, 64, ItemData::None)
            }),
            Self::Paper => Some(standalone(self, 64, ItemData::None)),
            Self::Book => Some(standalone(self, 64, ItemData::None)),
            Self::Slimeball => Some(standalone(self, 64, ItemData::None)),
            Self::ChestMinecart => Some(standalone(self, 1, ItemData::None)),
            Self::FurnaceMinecart => Some(standalone(self, 1, ItemData::None)),
            Self::Egg => Some(standalone(self, 16, ItemData::None)),
            Self::Compass => Some(standalone(self, 64, ItemData::None)),
            Self::FishingRod => Some(standalone(self, 1, ItemData::Durability(64))),
            Self::Clock => Some(standalone(self, 64, ItemData::None)),
            Self::GlowstoneDust => Some(standalone(self, 64, ItemData::None)),
            Self::RawFish => Some(standalone(self, 1, ItemData::None)),
            Self::CookedFish => Some(standalone(self, 1, ItemData::None)),
            Self::Dye => Some(standalone(self, 64, ItemData::Subtype(15))),
            Self::Bone => Some(standalone(self, 64, ItemData::None)),
            Self::Sugar => Some(standalone(self, 64, ItemData::None)),
            Self::Cake => Some(standalone(self, 1, ItemData::None)),
            Self::Bed => Some(standalone(self, 1, ItemData::None)),
            Self::Repeater => Some(ItemProperties {
                block: Some(Block::Repeater),
                ..standalone(self, 64, ItemData::None)
            }),
            Self::Cookie => Some(standalone(self, 8, ItemData::None)),
            Self::Map => Some(standalone(self, 1, ItemData::Map)),
            Self::Shears => Some(standalone(self, 1, ItemData::Durability(238))),
            Self::Record13 => Some(standalone(self, 1, ItemData::None)),
            Self::RecordCat => Some(standalone(self, 1, ItemData::None)),
            _ => None,
        }
    }
}

/// Formats block item IDs as their block variant and other IDs as item variants.
impl std::fmt::Display for Item {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.block() {
            Some(block) => std::fmt::Debug::fmt(&block, formatter),
            None => std::fmt::Debug::fmt(self, formatter),
        }
    }
}

/// Raw id ranges that contain an item: block items, the main item list, and records.
const RAW_RANGES: [(u16, u16); 3] = [(1, Block::MAX_ITEM_ID as u16), (256, 359), (2256, 2257)];

/// Stateless immutable registry; lookups allocate nothing.
pub struct ItemRegistry;

impl ItemRegistry {
    pub fn get(raw: u16) -> Option<ItemProperties> {
        Item::from_u16(raw).and_then(Item::properties)
    }

    pub fn iter() -> impl Iterator<Item = ItemProperties> {
        RAW_RANGES
            .into_iter()
            .flat_map(|(start, end)| start..=end)
            .filter_map(ItemRegistry::get)
    }
}

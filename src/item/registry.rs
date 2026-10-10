//! Inventory identities and Beta stack rules. Definitions do not imply that
//! an item's use, crafting recipe, or rendering has been implemented.
use super::tools::ToolType;
use crate::block::blocks::Block;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;
use serde::Deserialize;
use serde::de::IntoDeserializer;
use serde::de::value::Error as NameError;
use std::collections::HashMap;
use std::sync::LazyLock;

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
    /// `ItemFood.healAmount`: half-hearts restored when eaten.
    pub heal: Option<u8>,
    pub armor: Option<Armor>,
    pub tool: Option<ToolType>,
    /// Beta crafting remainder, for example an empty bucket left by milk.
    pub container: Option<Item>,
}

impl ItemProperties {
    /// Beta crafting remainder, for example an empty bucket left by milk.
    pub const fn container_item(self) -> Option<Item> {
        self.container
    }
}

/// `ItemArmor`: where a piece is worn and its `damageReduceAmount`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Armor {
    pub slot: ArmorSlot,
    pub points: u8,
}

/// The armor slots in the order the inventory holds them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum ArmorSlot {
    Helmet,
    Chestplate,
    Leggings,
    Boots,
}

/// One row of `data/items.ron`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    #[serde(default = "full_stack")]
    stack: u8,
    durability: Option<u16>,
    subtypes: Option<u16>,
    #[serde(default)]
    map: bool,
    places: Option<Block>,
    heal: Option<u8>,
    armor: Option<Armor>,
    tool: Option<ToolType>,
    leaves: Option<Item>,
}

fn full_stack() -> u8 {
    64
}

impl Row {
    fn build(self, item: Item) -> ItemProperties {
        let data = match (self.durability, self.subtypes) {
            (Some(uses), _) => ItemData::Durability(uses),
            (None, Some(highest)) => ItemData::Subtype(highest),
            (None, None) if self.map => ItemData::Map,
            (None, None) => ItemData::None,
        };
        ItemProperties {
            item,
            max_stack_size: self.stack,
            data,
            block: self.places,
            heal: self.heal,
            armor: self.armor,
            tool: self.tool,
            container: self.leaves,
        }
    }
}

/// Beta's `Item` registry, read once from `data/items.ron`: the standalone
/// items in the order of [`STANDALONE_RANGES`].
static STANDALONE: LazyLock<Vec<ItemProperties>> = LazyLock::new(|| {
    let mut rows: HashMap<Item, Row> = ron::from_str(include_str!("../../data/items.ron"))
        .unwrap_or_else(|error| panic!("data/items.ron: {error}"));
    STANDALONE_RANGES
        .into_iter()
        .flat_map(|(start, end)| start..=end)
        .map(|raw| {
            let item = Item::from(raw);
            let row = rows.remove(&item);
            row.unwrap_or_else(|| panic!("data/items.ron has no row for {item:?}"))
                .build(item)
        })
        .collect()
});

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
impl Item {
    pub fn from_block(block: Block) -> Option<Self> {
        block
            .has_item_id()
            .then(|| Self::from(u16::from(block.as_u8())))
    }

    /// The item a data file names: a standalone item's variant, or else a
    /// block's, as [`Display`](std::fmt::Display) writes them.
    ///
    /// `"block.Name"` is the block's own item where an item shares its name
    /// (`"WoodenDoor"` is the door item, `"block.WoodenDoor"` the half-door
    /// block), and `"Name:data"` adds a subtype or damage value.
    pub fn named(text: &str) -> Option<(Self, Option<u16>)> {
        let (name, data) = match text.split_once(':') {
            Some((name, data)) => (name, Some(data.parse().ok()?)),
            None => (text, None),
        };
        let variant = |name| IntoDeserializer::<NameError>::into_deserializer(name);
        let block = |name| Self::from_block(Block::deserialize(variant(name)).ok()?);
        let item = match name.strip_prefix("block.") {
            Some(name) => block(name),
            None => Self::deserialize(variant(name))
                .ok()
                .or_else(|| block(name)),
        }?;
        Some((item, data))
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
        self.properties()?.heal
    }

    /// `SlotArmor.isItemValid`: the armor slot this item is worn in, counting
    /// helmet, chestplate, leggings, boots. A pumpkin is worn as a helmet.
    pub fn armor_slot(self) -> Option<usize> {
        if self.block() == Some(Block::Pumpkin) {
            return Some(0);
        }
        Some(self.properties()?.armor?.slot as usize)
    }

    pub fn properties(self) -> Option<ItemProperties> {
        if let Some(block) = self.block() {
            return Some(ItemProperties {
                item: self,
                max_stack_size: 64,
                data: block.item_data(),
                block: Some(block),
                heal: None,
                armor: None,
                tool: None,
                container: None,
            });
        }
        let raw = self.as_u16();
        let mut before = 0;
        for (start, end) in STANDALONE_RANGES {
            if (start..=end).contains(&raw) {
                return Some(STANDALONE[usize::from(before + raw - start)]);
            }
            before += end - start + 1;
        }
        None
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
const RAW_RANGES: [(u16, u16); 3] = [
    (1, Block::MAX_ITEM_ID as u16),
    STANDALONE_RANGES[0],
    STANDALONE_RANGES[1],
];

/// Raw ids of the standalone items: the main item list, and records.
const STANDALONE_RANGES: [(u16, u16); 2] = [(256, 359), (2256, 2257)];

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

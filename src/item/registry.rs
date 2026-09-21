//! Inventory identities and Beta stack rules. Definitions do not imply that
//! an item's use, crafting recipe, or rendering has been implemented.
use super::ItemId;
use crate::world::block::registry::BLOCK_DEFINITIONS;
use crate::world::block::registry::BetaBlockId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemData {
    None,
    /// Highest supported subtype value (inclusive).
    Subtype(u16),
    /// Beta's maxDamage threshold, not a simulation of tool behavior.
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
pub struct ItemDefinition {
    pub id: ItemId,
    pub name: &'static str,
    pub max_stack_size: u8,
    pub data: ItemData,
    /// Present for direct block items, not special items such as doors/buckets.
    pub block: Option<BetaBlockId>,
}

impl ItemDefinition {
    /// Beta crafting remainder, for example an empty bucket left by milk.
    pub const fn container_item(self) -> Option<ItemId> {
        match self.id {
            ItemId::MILK_BUCKET => Some(ItemId::BUCKET),
            _ => None,
        }
    }
}

macro_rules! items {
    ($(($constant:ident, $id:literal, $name:literal, $limit:literal, $data:expr)),* $(,)?) => {
        impl ItemId { $(pub const $constant: Self = Self($id);)* }
        pub const ITEM_DEFINITIONS: &[ItemDefinition] = &[
            $(ItemDefinition {
                id: ItemId::$constant, name: $name, max_stack_size: $limit,
                data: $data, block: None,
            },)*
        ];
    };
}

items! {
    (IRON_SHOVEL, 256, "iron_shovel", 1, ItemData::Durability(250)),
    (IRON_PICKAXE, 257, "iron_pickaxe", 1, ItemData::Durability(250)),
    (IRON_AXE, 258, "iron_axe", 1, ItemData::Durability(250)),
    (FLINT_AND_STEEL, 259, "flint_and_steel", 1, ItemData::Durability(64)),
    (APPLE, 260, "apple", 1, ItemData::None),
    (BOW, 261, "bow", 1, ItemData::None),
    (ARROW, 262, "arrow", 64, ItemData::None),
    (COAL, 263, "coal", 64, ItemData::Subtype(1)),
    (DIAMOND, 264, "diamond", 64, ItemData::None),
    (IRON_INGOT, 265, "iron_ingot", 64, ItemData::None),
    (GOLD_INGOT, 266, "gold_ingot", 64, ItemData::None),
    (IRON_SWORD, 267, "iron_sword", 1, ItemData::Durability(250)),
    (WOODEN_SWORD, 268, "wooden_sword", 1, ItemData::Durability(59)),
    (WOODEN_SHOVEL, 269, "wooden_shovel", 1, ItemData::Durability(59)),
    (WOODEN_PICKAXE, 270, "wooden_pickaxe", 1, ItemData::Durability(59)),
    (WOODEN_AXE, 271, "wooden_axe", 1, ItemData::Durability(59)),
    (STONE_SWORD, 272, "stone_sword", 1, ItemData::Durability(131)),
    (STONE_SHOVEL, 273, "stone_shovel", 1, ItemData::Durability(131)),
    (STONE_PICKAXE, 274, "stone_pickaxe", 1, ItemData::Durability(131)),
    (STONE_AXE, 275, "stone_axe", 1, ItemData::Durability(131)),
    (DIAMOND_SWORD, 276, "diamond_sword", 1, ItemData::Durability(1561)),
    (DIAMOND_SHOVEL, 277, "diamond_shovel", 1, ItemData::Durability(1561)),
    (DIAMOND_PICKAXE, 278, "diamond_pickaxe", 1, ItemData::Durability(1561)),
    (DIAMOND_AXE, 279, "diamond_axe", 1, ItemData::Durability(1561)),
    (STICK, 280, "stick", 64, ItemData::None),
    (BOWL, 281, "bowl", 64, ItemData::None),
    (MUSHROOM_STEW, 282, "mushroom_stew", 1, ItemData::None),
    (GOLD_SWORD, 283, "gold_sword", 1, ItemData::Durability(32)),
    (GOLD_SHOVEL, 284, "gold_shovel", 1, ItemData::Durability(32)),
    (GOLD_PICKAXE, 285, "gold_pickaxe", 1, ItemData::Durability(32)),
    (GOLD_AXE, 286, "gold_axe", 1, ItemData::Durability(32)),
    (STRING, 287, "string", 64, ItemData::None),
    (FEATHER, 288, "feather", 64, ItemData::None),
    (GUNPOWDER, 289, "gunpowder", 64, ItemData::None),
    (WOODEN_HOE, 290, "wooden_hoe", 1, ItemData::Durability(59)),
    (STONE_HOE, 291, "stone_hoe", 1, ItemData::Durability(131)),
    (IRON_HOE, 292, "iron_hoe", 1, ItemData::Durability(250)),
    (DIAMOND_HOE, 293, "diamond_hoe", 1, ItemData::Durability(1561)),
    (GOLD_HOE, 294, "gold_hoe", 1, ItemData::Durability(32)),
    (SEEDS, 295, "seeds", 64, ItemData::None),
    (WHEAT, 296, "wheat", 64, ItemData::None),
    (BREAD, 297, "bread", 1, ItemData::None),
    (LEATHER_HELMET, 298, "leather_helmet", 1, ItemData::Durability(33)),
    (LEATHER_CHESTPLATE, 299, "leather_chestplate", 1, ItemData::Durability(48)),
    (LEATHER_LEGGINGS, 300, "leather_leggings", 1, ItemData::Durability(45)),
    (LEATHER_BOOTS, 301, "leather_boots", 1, ItemData::Durability(39)),
    (CHAINMAIL_HELMET, 302, "chainmail_helmet", 1, ItemData::Durability(66)),
    (CHAINMAIL_CHESTPLATE, 303, "chainmail_chestplate", 1, ItemData::Durability(96)),
    (CHAINMAIL_LEGGINGS, 304, "chainmail_leggings", 1, ItemData::Durability(90)),
    (CHAINMAIL_BOOTS, 305, "chainmail_boots", 1, ItemData::Durability(78)),
    (IRON_HELMET, 306, "iron_helmet", 1, ItemData::Durability(132)),
    (IRON_CHESTPLATE, 307, "iron_chestplate", 1, ItemData::Durability(192)),
    (IRON_LEGGINGS, 308, "iron_leggings", 1, ItemData::Durability(180)),
    (IRON_BOOTS, 309, "iron_boots", 1, ItemData::Durability(156)),
    (DIAMOND_HELMET, 310, "diamond_helmet", 1, ItemData::Durability(264)),
    (DIAMOND_CHESTPLATE, 311, "diamond_chestplate", 1, ItemData::Durability(384)),
    (DIAMOND_LEGGINGS, 312, "diamond_leggings", 1, ItemData::Durability(360)),
    (DIAMOND_BOOTS, 313, "diamond_boots", 1, ItemData::Durability(312)),
    (GOLD_HELMET, 314, "gold_helmet", 1, ItemData::Durability(66)),
    (GOLD_CHESTPLATE, 315, "gold_chestplate", 1, ItemData::Durability(96)),
    (GOLD_LEGGINGS, 316, "gold_leggings", 1, ItemData::Durability(90)),
    (GOLD_BOOTS, 317, "gold_boots", 1, ItemData::Durability(78)),
    (FLINT, 318, "flint", 64, ItemData::None),
    (RAW_PORKCHOP, 319, "raw_porkchop", 1, ItemData::None),
    (COOKED_PORKCHOP, 320, "cooked_porkchop", 1, ItemData::None),
    (PAINTING, 321, "painting", 64, ItemData::None),
    (GOLDEN_APPLE, 322, "golden_apple", 1, ItemData::None),
    (SIGN, 323, "sign", 1, ItemData::None),
    (WOODEN_DOOR, 324, "wooden_door", 1, ItemData::None),
    (BUCKET, 325, "bucket", 1, ItemData::None),
    (WATER_BUCKET, 326, "water_bucket", 1, ItemData::None),
    (LAVA_BUCKET, 327, "lava_bucket", 1, ItemData::None),
    (MINECART, 328, "minecart", 1, ItemData::None),
    (SADDLE, 329, "saddle", 1, ItemData::None),
    (IRON_DOOR, 330, "iron_door", 1, ItemData::None),
    (REDSTONE, 331, "redstone", 64, ItemData::None),
    (SNOWBALL, 332, "snowball", 16, ItemData::None),
    (BOAT, 333, "boat", 1, ItemData::None),
    (LEATHER, 334, "leather", 64, ItemData::None),
    (MILK_BUCKET, 335, "milk_bucket", 1, ItemData::None),
    (BRICK, 336, "brick", 64, ItemData::None),
    (CLAY_BALL, 337, "clay_ball", 64, ItemData::None),
    (SUGAR_CANE, 338, "sugar_cane", 64, ItemData::None),
    (PAPER, 339, "paper", 64, ItemData::None),
    (BOOK, 340, "book", 64, ItemData::None),
    (SLIMEBALL, 341, "slimeball", 64, ItemData::None),
    (CHEST_MINECART, 342, "chest_minecart", 1, ItemData::None),
    (FURNACE_MINECART, 343, "furnace_minecart", 1, ItemData::None),
    (EGG, 344, "egg", 16, ItemData::None),
    (COMPASS, 345, "compass", 64, ItemData::None),
    (FISHING_ROD, 346, "fishing_rod", 1, ItemData::Durability(64)),
    (CLOCK, 347, "clock", 64, ItemData::None),
    (GLOWSTONE_DUST, 348, "glowstone_dust", 64, ItemData::None),
    (RAW_FISH, 349, "raw_fish", 1, ItemData::None),
    (COOKED_FISH, 350, "cooked_fish", 1, ItemData::None),
    (DYE, 351, "dye", 64, ItemData::Subtype(15)),
    (BONE, 352, "bone", 64, ItemData::None),
    (SUGAR, 353, "sugar", 64, ItemData::None),
    (CAKE, 354, "cake", 1, ItemData::None),
    (BED, 355, "bed", 1, ItemData::None),
    (REPEATER, 356, "repeater", 64, ItemData::None),
    (COOKIE, 357, "cookie", 8, ItemData::None),
    (MAP, 358, "map", 1, ItemData::Map),
    (SHEARS, 359, "shears", 1, ItemData::Durability(238)),
    (RECORD13, 2256, "record13", 1, ItemData::None),
    (RECORD_CAT, 2257, "record_cat", 1, ItemData::None),
}

const fn block_items() -> [ItemDefinition; 96] {
    let mut definitions = [ItemDefinition {
        id: ItemId(1),
        name: "",
        max_stack_size: 64,
        data: ItemData::None,
        block: None,
    }; 96];
    let mut index = 0;
    while index < definitions.len() {
        let block = BLOCK_DEFINITIONS[index + 1];
        definitions[index] = ItemDefinition {
            id: ItemId::from_block(block.id),
            name: block.name,
            max_stack_size: 64,
            data: match block.id.as_u8() {
                6 | 17 | 18 => ItemData::Subtype(2),
                35 => ItemData::Subtype(15),
                44 => ItemData::Subtype(3),
                _ => ItemData::None,
            },
            block: Some(block.id),
        };
        index += 1;
    }
    definitions
}

/// Mirrors Beta's automatic ItemBlock registration, including technical blocks.
/// Air is not an inventory item.
pub const BLOCK_ITEM_DEFINITIONS: [ItemDefinition; 96] = block_items();

/// Stateless immutable registry; lookups allocate nothing and are constant time.
pub struct ItemRegistry;

impl ItemRegistry {
    pub fn get(id: ItemId) -> Option<&'static ItemDefinition> {
        match id.0 {
            1..=96 => BLOCK_ITEM_DEFINITIONS.get(id.0 as usize - 1),
            256..=359 => ITEM_DEFINITIONS.get(id.0 as usize - 256),
            2256..=2257 => ITEM_DEFINITIONS.get(id.0 as usize - 2256 + 104),
            _ => None,
        }
    }

    pub fn iter() -> impl Iterator<Item = &'static ItemDefinition> {
        BLOCK_ITEM_DEFINITIONS.iter().chain(ITEM_DEFINITIONS.iter())
    }
}

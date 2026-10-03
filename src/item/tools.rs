//! Beta 1.7.3 tool speed, harvest, and block-break durability.
//!
//! `ItemPickaxe`, `ItemAxe`, `ItemSpade`, `ItemSword`, and `ItemShears`.
//! Speed lists are block identity checks. Harvest level is separate, and a
//! tool that cannot harvest a block mines at the slow fist rate.

use super::ItemId;
use super::ItemStack;
use crate::block::id::Id;
use crate::block::properties::harvestable_by_hand;
use crate::block::properties::is_breakable;
use crate::block::properties::mine_progress_per_tick;

/// `EnumToolMaterial` harvest level and `efficiencyOnProperMaterial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier {
    Wood,
    Stone,
    Iron,
    Diamond,
    Gold,
}

impl Tier {
    const fn level(self) -> u8 {
        match self {
            Self::Wood | Self::Gold => 0,
            Self::Stone => 1,
            Self::Iron => 2,
            Self::Diamond => 3,
        }
    }

    const fn efficiency(self) -> f32 {
        match self {
            Self::Wood => 2.0,
            Self::Stone => 4.0,
            Self::Iron => 6.0,
            Self::Diamond => 8.0,
            Self::Gold => 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Pick(Tier),
    Axe(Tier),
    Shovel(Tier),
    Sword(Tier),
    Hoe(Tier),
    Shears,
}

fn kind(id: ItemId) -> Option<Kind> {
    Some(match id {
        ItemId::WoodenPickaxe => Kind::Pick(Tier::Wood),
        ItemId::StonePickaxe => Kind::Pick(Tier::Stone),
        ItemId::IronPickaxe => Kind::Pick(Tier::Iron),
        ItemId::DiamondPickaxe => Kind::Pick(Tier::Diamond),
        ItemId::GoldPickaxe => Kind::Pick(Tier::Gold),
        ItemId::WoodenAxe => Kind::Axe(Tier::Wood),
        ItemId::StoneAxe => Kind::Axe(Tier::Stone),
        ItemId::IronAxe => Kind::Axe(Tier::Iron),
        ItemId::DiamondAxe => Kind::Axe(Tier::Diamond),
        ItemId::GoldAxe => Kind::Axe(Tier::Gold),
        ItemId::WoodenShovel => Kind::Shovel(Tier::Wood),
        ItemId::StoneShovel => Kind::Shovel(Tier::Stone),
        ItemId::IronShovel => Kind::Shovel(Tier::Iron),
        ItemId::DiamondShovel => Kind::Shovel(Tier::Diamond),
        ItemId::GoldShovel => Kind::Shovel(Tier::Gold),
        ItemId::WoodenSword => Kind::Sword(Tier::Wood),
        ItemId::StoneSword => Kind::Sword(Tier::Stone),
        ItemId::IronSword => Kind::Sword(Tier::Iron),
        ItemId::DiamondSword => Kind::Sword(Tier::Diamond),
        ItemId::GoldSword => Kind::Sword(Tier::Gold),
        ItemId::WoodenHoe => Kind::Hoe(Tier::Wood),
        ItemId::StoneHoe => Kind::Hoe(Tier::Stone),
        ItemId::IronHoe => Kind::Hoe(Tier::Iron),
        ItemId::DiamondHoe => Kind::Hoe(Tier::Diamond),
        ItemId::GoldHoe => Kind::Hoe(Tier::Gold),
        ItemId::Shears => Kind::Shears,
        _ => return None,
    })
}

/// `InventoryPlayer.getDamageVsEntity`: `ItemSword` deals `4 + 2 *` its
/// material's damage, `ItemTool` its base (shovel 1, pickaxe 2, axe 3) plus
/// the material's damage, and anything else, or an empty hand, deals 1.
pub fn damage_vs_entity(held: Option<ItemStack>) -> i16 {
    let Some(kind) = held.and_then(|stack| kind(stack.item())) else {
        return 1;
    };
    let material = |tier: Tier| match tier {
        Tier::Wood | Tier::Gold => 0,
        Tier::Stone => 1,
        Tier::Iron => 2,
        Tier::Diamond => 3,
    };
    match kind {
        Kind::Sword(tier) => 4 + material(tier) * 2,
        Kind::Shovel(tier) => 1 + material(tier),
        Kind::Pick(tier) => 2 + material(tier),
        Kind::Axe(tier) => 3 + material(tier),
        Kind::Hoe(_) | Kind::Shears => 1,
    }
}

/// `Item.hitEntity` wear: a sword loses 1 durability per hit and a tool 2.
/// Hoes and shears are plain items in Beta and do not wear.
pub fn hit_durability(held: ItemStack) -> u16 {
    match kind(held.item()) {
        Some(Kind::Sword(_)) => 1,
        Some(Kind::Pick(_) | Kind::Axe(_) | Kind::Shovel(_)) => 2,
        _ => 0,
    }
}

/// Whether an item is one of Beta's hoes.
pub fn is_hoe(id: ItemId) -> bool {
    matches!(kind(id), Some(Kind::Hoe(_)))
}

fn is_log(block: Id) -> bool {
    matches!(block, Id::Wood | Id::SpruceWood | Id::BirchWood)
}

fn is_leaves(block: Id) -> bool {
    matches!(block, Id::Leaves | Id::SpruceLeaves | Id::BirchLeaves)
}

/// `ItemPickaxe.blocksEffectiveAgainst`. Obsidian, redstone ore, furnaces,
/// dispensers, bricks, and glowstone are absent on purpose.
fn pick_effective(block: Id) -> bool {
    matches!(
        block,
        Id::Cobblestone
            | Id::DoubleStoneSlab
            | Id::Stone
            | Id::Sandstone
            | Id::MossyCobblestone
            | Id::IronOre
            | Id::IronBlock
            | Id::CoalOre
            | Id::GoldBlock
            | Id::GoldOre
            | Id::DiamondOre
            | Id::DiamondBlock
            | Id::Ice
            | Id::Netherrack
            | Id::LapisOre
            | Id::LapisBlock
    )
}

/// `ItemAxe.blocksEffectiveAgainst`. Crafting tables, note blocks, jukeboxes,
/// and pumpkins are wood or pumpkin and are not in this list.
fn axe_effective(block: Id) -> bool {
    is_log(block)
        || block.is_chest()
        || matches!(
            block,
            Id::WoodenPlanks | Id::SprucePlanks | Id::BirchPlanks | Id::Bookshelf
        )
}

/// `ItemSpade.blocksEffectiveAgainst`. The snow block is the layered entry
/// that has a real hardness here. Farmland and the snow layer are catalog blocks.
fn shovel_effective(block: Id) -> bool {
    matches!(
        block,
        Id::Grass | Id::Dirt | Id::Sand | Id::Gravel | Id::Snow | Id::Farmland | Id::Clay
    )
}

/// `Item.getStrVsBlock` for the held stack. An empty hand is `1.0`.
pub fn str_vs_block(tool: Option<ItemStack>, block: Id) -> f32 {
    let Some(tool) = tool else {
        return 1.0;
    };
    match kind(tool.item()) {
        Some(Kind::Pick(tier)) if pick_effective(block) => tier.efficiency(),
        Some(Kind::Axe(tier)) if axe_effective(block) => tier.efficiency(),
        Some(Kind::Shovel(tier)) if shovel_effective(block) => tier.efficiency(),
        // `ItemSword.getStrVsBlock` is 15 on web and 1.5 on everything else.
        Some(Kind::Sword(_)) if block == Id::Cobweb => 15.0,
        Some(Kind::Sword(_)) => 1.5,
        Some(Kind::Shears) if is_leaves(block) || block == Id::Cobweb => 15.0,
        Some(Kind::Shears) if block == Id::Wool => 5.0,
        _ => 1.0,
    }
}

/// `InventoryPlayer.canHarvestBlock`. Hand-harvestable materials win before
/// the tool is consulted. A false result still breaks the block, slowly, and
/// suppresses its drop.
pub fn can_harvest(tool: Option<ItemStack>, block: Id) -> bool {
    if harvestable_by_hand(block) {
        return true;
    }
    tool.is_some_and(|tool| tool_can_harvest(tool.item(), block))
}

fn tool_can_harvest(id: ItemId, block: Id) -> bool {
    match kind(id) {
        Some(Kind::Pick(tier)) => pick_can_harvest(tier, block),
        // `ItemSpade.canHarvestBlock`: the snow layer and the snow block.
        Some(Kind::Shovel(_)) => matches!(block, Id::Snow | Id::SnowLayer),
        // `ItemSword` and `ItemShears` harvest web only. Leaves are already
        // hand-harvestable; shears change the drop, not this gate.
        Some(Kind::Sword(_) | Kind::Shears) => block == Id::Cobweb,
        _ => false,
    }
}

/// `ItemPickaxe.canHarvestBlock`. Rock and iron fall through to any pick.
/// Iron blocks and the tiered ores are handled before that fallthrough.
fn pick_can_harvest(tier: Tier, block: Id) -> bool {
    let level = tier.level();
    if block == Id::Obsidian {
        return level == 3;
    }
    if matches!(block, Id::DiamondBlock | Id::DiamondOre) {
        return level >= 2;
    }
    if matches!(block, Id::GoldBlock | Id::GoldOre) {
        return level >= 2;
    }
    if matches!(block, Id::IronBlock | Id::IronOre) {
        return level >= 1;
    }
    if matches!(block, Id::LapisBlock | Id::LapisOre) {
        return level >= 1;
    }
    if matches!(block, Id::RedstoneOre | Id::LitRedstoneOre) {
        return level >= 2;
    }
    matches!(
        block,
        Id::Stone
            | Id::Cobblestone
            | Id::Bedrock
            | Id::CoalOre
            | Id::Dispenser
            | Id::Sandstone
            | Id::DoubleStoneSlab
            | Id::StoneSlab
            | Id::Bricks
            | Id::MossyCobblestone
            | Id::Furnace
            | Id::LitFurnace
            | Id::CobblestoneStairs
            | Id::StonePressurePlate
            | Id::IronDoor
            | Id::Netherrack
            | Id::Glowstone
    )
}

/// Damage from `onBlockDestroyed`. Picks, axes, and shovels always lose one
/// use. Swords lose two. Shears lose one on leaves and web. Hoes lose none.
pub fn break_durability(tool: ItemStack, block: Id) -> u16 {
    match kind(tool.item()) {
        Some(Kind::Pick(_) | Kind::Axe(_) | Kind::Shovel(_)) => 1,
        Some(Kind::Sword(_)) => 2,
        Some(Kind::Shears) if is_leaves(block) || block == Id::Cobweb => 1,
        _ => 0,
    }
}

/// One tick of `Block.blockStrength` for this held stack.
pub fn mine_step(block: Id, tool: Option<ItemStack>, on_ground: bool, in_water: bool) -> f32 {
    mine_progress_per_tick(
        block,
        str_vs_block(tool, block),
        can_harvest(tool, block),
        on_ground,
        in_water,
    )
}

/// Ticks of mining needed to break `block`, or `None` if it cannot break.
/// A result of `1` is an instant break on click (`blockStrength >= 1`).
pub fn ticks_to_break(
    block: Id,
    tool: Option<ItemStack>,
    on_ground: bool,
    in_water: bool,
) -> Option<u32> {
    if !is_breakable(block) {
        return None;
    }
    let step = mine_step(block, tool, on_ground, in_water);
    if !step.is_finite() || step >= 1.0 {
        return Some(1);
    }
    if step <= 0.0 {
        return None;
    }
    Some((1.0 / step).ceil() as u32)
}

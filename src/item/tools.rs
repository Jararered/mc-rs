//! Beta 1.7.3 tool speed, harvest, and block-break durability.
//!
//! `ItemPickaxe`, `ItemAxe`, `ItemSpade`, `ItemSword`, and `ItemShears`.
//! Speed lists are block identity checks. Harvest level is separate, and a
//! tool that cannot harvest a block mines at the slow fist rate.

use super::ItemId;
use super::ItemStack;
use crate::world::block::block::BlockId;
use crate::world::block::properties::harvestable_by_hand;
use crate::world::block::properties::is_breakable;
use crate::world::block::properties::mine_progress_per_tick;

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
        ItemId::WOODEN_PICKAXE => Kind::Pick(Tier::Wood),
        ItemId::STONE_PICKAXE => Kind::Pick(Tier::Stone),
        ItemId::IRON_PICKAXE => Kind::Pick(Tier::Iron),
        ItemId::DIAMOND_PICKAXE => Kind::Pick(Tier::Diamond),
        ItemId::GOLD_PICKAXE => Kind::Pick(Tier::Gold),
        ItemId::WOODEN_AXE => Kind::Axe(Tier::Wood),
        ItemId::STONE_AXE => Kind::Axe(Tier::Stone),
        ItemId::IRON_AXE => Kind::Axe(Tier::Iron),
        ItemId::DIAMOND_AXE => Kind::Axe(Tier::Diamond),
        ItemId::GOLD_AXE => Kind::Axe(Tier::Gold),
        ItemId::WOODEN_SHOVEL => Kind::Shovel(Tier::Wood),
        ItemId::STONE_SHOVEL => Kind::Shovel(Tier::Stone),
        ItemId::IRON_SHOVEL => Kind::Shovel(Tier::Iron),
        ItemId::DIAMOND_SHOVEL => Kind::Shovel(Tier::Diamond),
        ItemId::GOLD_SHOVEL => Kind::Shovel(Tier::Gold),
        ItemId::WOODEN_SWORD => Kind::Sword(Tier::Wood),
        ItemId::STONE_SWORD => Kind::Sword(Tier::Stone),
        ItemId::IRON_SWORD => Kind::Sword(Tier::Iron),
        ItemId::DIAMOND_SWORD => Kind::Sword(Tier::Diamond),
        ItemId::GOLD_SWORD => Kind::Sword(Tier::Gold),
        ItemId::WOODEN_HOE => Kind::Hoe(Tier::Wood),
        ItemId::STONE_HOE => Kind::Hoe(Tier::Stone),
        ItemId::IRON_HOE => Kind::Hoe(Tier::Iron),
        ItemId::DIAMOND_HOE => Kind::Hoe(Tier::Diamond),
        ItemId::GOLD_HOE => Kind::Hoe(Tier::Gold),
        ItemId::SHEARS => Kind::Shears,
        _ => return None,
    })
}

fn is_log(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Wood | BlockId::SpruceWood | BlockId::BirchWood
    )
}

fn is_leaves(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves
    )
}

/// `ItemPickaxe.blocksEffectiveAgainst`. Obsidian, redstone ore, furnaces,
/// dispensers, bricks, and glowstone are absent on purpose.
fn pick_effective(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Cobblestone
            | BlockId::DoubleStoneSlab
            | BlockId::Stone
            | BlockId::Sandstone
            | BlockId::MossyCobblestone
            | BlockId::IronOre
            | BlockId::IronBlock
            | BlockId::CoalOre
            | BlockId::GoldBlock
            | BlockId::GoldOre
            | BlockId::DiamondOre
            | BlockId::DiamondBlock
            | BlockId::Ice
            | BlockId::Netherrack
            | BlockId::LapisOre
            | BlockId::LapisBlock
    )
}

/// `ItemAxe.blocksEffectiveAgainst`. Crafting tables, note blocks, jukeboxes,
/// and pumpkins are wood or pumpkin and are not in this list.
fn axe_effective(block: BlockId) -> bool {
    is_log(block) || matches!(block, BlockId::WoodenPlanks | BlockId::Bookshelf)
}

/// `ItemSpade.blocksEffectiveAgainst`. Snow layers and farmland are not blocks
/// in this game; the snow block is.
fn shovel_effective(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Grass
            | BlockId::Dirt
            | BlockId::Sand
            | BlockId::Gravel
            | BlockId::Snow
            | BlockId::Clay
    )
}

/// `Item.getStrVsBlock` for the held stack. An empty hand is `1.0`.
pub fn str_vs_block(tool: Option<ItemStack>, block: BlockId) -> f32 {
    let Some(tool) = tool else {
        return 1.0;
    };
    match kind(tool.item()) {
        Some(Kind::Pick(tier)) if pick_effective(block) => tier.efficiency(),
        Some(Kind::Axe(tier)) if axe_effective(block) => tier.efficiency(),
        Some(Kind::Shovel(tier)) if shovel_effective(block) => tier.efficiency(),
        Some(Kind::Sword(_)) => 1.5,
        Some(Kind::Shears) if is_leaves(block) => 15.0,
        Some(Kind::Shears) if block == BlockId::Wool => 5.0,
        _ => 1.0,
    }
}

/// `InventoryPlayer.canHarvestBlock`. Hand-harvestable materials win before
/// the tool is consulted. A false result still breaks the block, slowly, and
/// suppresses its drop.
pub fn can_harvest(tool: Option<ItemStack>, block: BlockId) -> bool {
    if harvestable_by_hand(block) {
        return true;
    }
    tool.is_some_and(|tool| tool_can_harvest(tool.item(), block))
}

fn tool_can_harvest(id: ItemId, block: BlockId) -> bool {
    match kind(id) {
        Some(Kind::Pick(tier)) => pick_can_harvest(tier, block),
        // `ItemSpade` harvests the snow layer and the snow block. Only the
        // snow block exists here.
        Some(Kind::Shovel(_)) => block == BlockId::Snow,
        _ => false,
    }
}

/// `ItemPickaxe.canHarvestBlock`. Rock falls through to any pick. Iron blocks
/// and the tiered ores are handled before that fallthrough.
fn pick_can_harvest(tier: Tier, block: BlockId) -> bool {
    let level = tier.level();
    if block == BlockId::Obsidian {
        return level == 3;
    }
    if matches!(block, BlockId::DiamondBlock | BlockId::DiamondOre) {
        return level >= 2;
    }
    if matches!(block, BlockId::GoldBlock | BlockId::GoldOre) {
        return level >= 2;
    }
    if matches!(block, BlockId::IronBlock | BlockId::IronOre) {
        return level >= 1;
    }
    if matches!(block, BlockId::LapisBlock | BlockId::LapisOre) {
        return level >= 1;
    }
    if matches!(block, BlockId::RedstoneOre | BlockId::LitRedstoneOre) {
        return level >= 2;
    }
    matches!(
        block,
        BlockId::Stone
            | BlockId::Cobblestone
            | BlockId::Bedrock
            | BlockId::CoalOre
            | BlockId::Dispenser
            | BlockId::Sandstone
            | BlockId::DoubleStoneSlab
            | BlockId::Bricks
            | BlockId::MossyCobblestone
            | BlockId::Furnace
            | BlockId::LitFurnace
            | BlockId::Netherrack
            | BlockId::Glowstone
    )
}

/// Damage from `onBlockDestroyed`. Picks, axes, and shovels always lose one
/// use. Swords lose two. Shears lose one only on leaves. Hoes lose none.
pub fn break_durability(tool: ItemStack, block: BlockId) -> u16 {
    match kind(tool.item()) {
        Some(Kind::Pick(_) | Kind::Axe(_) | Kind::Shovel(_)) => 1,
        Some(Kind::Sword(_)) => 2,
        Some(Kind::Shears) if is_leaves(block) => 1,
        _ => 0,
    }
}

/// One tick of `Block.blockStrength` for this held stack.
pub fn mine_step(block: BlockId, tool: Option<ItemStack>, on_ground: bool, in_water: bool) -> f32 {
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
    block: BlockId,
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

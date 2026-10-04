//! Beta 1.7.3 tool speed, harvest, and block-break durability.
//!
//! `ItemPickaxe`, `ItemAxe`, `ItemSpade`, `ItemSword`, and `ItemShears`.
//! Speed lists are block identity checks. Harvest level is separate, and a
//! tool that cannot harvest a block mines at the slow fist rate.

use super::Item;
use super::ItemStack;
use crate::block::blocks::Block;
use crate::block::properties::harvestable_by_hand;
use crate::block::properties::mine_progress_per_tick;

/// `EnumToolMaterial` harvest level and `efficiencyOnProperMaterial`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolTier {
    Wood,
    Stone,
    Iron,
    Diamond,
    Gold,
}

impl ToolTier {
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
enum ToolType {
    Pick(ToolTier),
    Axe(ToolTier),
    Shovel(ToolTier),
    Sword(ToolTier),
    Hoe(ToolTier),
    Shears,
}

fn tool_type(item: Item) -> Option<ToolType> {
    Some(match item {
        Item::WoodenPickaxe => ToolType::Pick(ToolTier::Wood),
        Item::StonePickaxe => ToolType::Pick(ToolTier::Stone),
        Item::IronPickaxe => ToolType::Pick(ToolTier::Iron),
        Item::DiamondPickaxe => ToolType::Pick(ToolTier::Diamond),
        Item::GoldPickaxe => ToolType::Pick(ToolTier::Gold),
        Item::WoodenAxe => ToolType::Axe(ToolTier::Wood),
        Item::StoneAxe => ToolType::Axe(ToolTier::Stone),
        Item::IronAxe => ToolType::Axe(ToolTier::Iron),
        Item::DiamondAxe => ToolType::Axe(ToolTier::Diamond),
        Item::GoldAxe => ToolType::Axe(ToolTier::Gold),
        Item::WoodenShovel => ToolType::Shovel(ToolTier::Wood),
        Item::StoneShovel => ToolType::Shovel(ToolTier::Stone),
        Item::IronShovel => ToolType::Shovel(ToolTier::Iron),
        Item::DiamondShovel => ToolType::Shovel(ToolTier::Diamond),
        Item::GoldShovel => ToolType::Shovel(ToolTier::Gold),
        Item::WoodenSword => ToolType::Sword(ToolTier::Wood),
        Item::StoneSword => ToolType::Sword(ToolTier::Stone),
        Item::IronSword => ToolType::Sword(ToolTier::Iron),
        Item::DiamondSword => ToolType::Sword(ToolTier::Diamond),
        Item::GoldSword => ToolType::Sword(ToolTier::Gold),
        Item::WoodenHoe => ToolType::Hoe(ToolTier::Wood),
        Item::StoneHoe => ToolType::Hoe(ToolTier::Stone),
        Item::IronHoe => ToolType::Hoe(ToolTier::Iron),
        Item::DiamondHoe => ToolType::Hoe(ToolTier::Diamond),
        Item::GoldHoe => ToolType::Hoe(ToolTier::Gold),
        Item::Shears => ToolType::Shears,
        _ => return None,
    })
}

/// `InventoryPlayer.getDamageVsEntity`: `ItemSword` deals `4 + 2 *` its
/// material's damage, `ItemTool` its base (shovel 1, pickaxe 2, axe 3) plus
/// the material's damage, and anything else, or an empty hand, deals 1.
pub fn damage_vs_entity(held: Option<ItemStack>) -> i16 {
    let Some(kind) = held.and_then(|stack| tool_type(stack.item())) else {
        return 1;
    };
    let material = |tier: ToolTier| match tier {
        ToolTier::Wood | ToolTier::Gold => 0,
        ToolTier::Stone => 1,
        ToolTier::Iron => 2,
        ToolTier::Diamond => 3,
    };
    match kind {
        ToolType::Sword(tier) => 4 + material(tier) * 2,
        ToolType::Shovel(tier) => 1 + material(tier),
        ToolType::Pick(tier) => 2 + material(tier),
        ToolType::Axe(tier) => 3 + material(tier),
        ToolType::Hoe(_) | ToolType::Shears => 1,
    }
}

/// `Item.hitEntity` wear: a sword loses 1 durability per hit and a tool 2.
/// Hoes and shears are plain items in Beta and do not wear.
pub fn hit_durability(held: ItemStack) -> u16 {
    match tool_type(held.item()) {
        Some(ToolType::Sword(_)) => 1,
        Some(ToolType::Pick(_) | ToolType::Axe(_) | ToolType::Shovel(_)) => 2,
        _ => 0,
    }
}

/// Whether an item is one of Beta's hoes.
pub fn is_hoe(item: Item) -> bool {
    matches!(tool_type(item), Some(ToolType::Hoe(_)))
}

fn is_log(block: Block) -> bool {
    matches!(block, Block::Wood | Block::SpruceWood | Block::BirchWood)
}

fn is_leaves(block: Block) -> bool {
    matches!(
        block,
        Block::Leaves | Block::SpruceLeaves | Block::BirchLeaves
    )
}

/// `ItemPickaxe.blocksEffectiveAgainst`. Obsidian, redstone ore, furnaces,
/// dispensers, bricks, and glowstone are absent on purpose.
fn pick_effective(block: Block) -> bool {
    matches!(
        block,
        Block::Cobblestone
            | Block::DoubleStoneSlab
            | Block::Stone
            | Block::Sandstone
            | Block::MossyCobblestone
            | Block::IronOre
            | Block::IronBlock
            | Block::CoalOre
            | Block::GoldBlock
            | Block::GoldOre
            | Block::DiamondOre
            | Block::DiamondBlock
            | Block::Ice
            | Block::Netherrack
            | Block::LapisOre
            | Block::LapisBlock
    )
}

/// `ItemAxe.blocksEffectiveAgainst`. Crafting tables, note blocks, jukeboxes,
/// and pumpkins are wood or pumpkin and are not in this list.
fn axe_effective(block: Block) -> bool {
    is_log(block)
        || block.is_chest()
        || matches!(
            block,
            Block::WoodenPlanks | Block::SprucePlanks | Block::BirchPlanks | Block::Bookshelf
        )
}

/// `ItemSpade.blocksEffectiveAgainst`. The snow block is the layered entry
/// that has a real hardness here. Farmland and the snow layer are catalog blocks.
fn shovel_effective(block: Block) -> bool {
    matches!(
        block,
        Block::Grass
            | Block::Dirt
            | Block::Sand
            | Block::Gravel
            | Block::Snow
            | Block::Farmland
            | Block::Clay
    )
}

/// `Item.getStrVsBlock` for the held stack. An empty hand is `1.0`.
pub fn str_vs_block(tool: Option<ItemStack>, block: Block) -> f32 {
    let Some(tool) = tool else {
        return 1.0;
    };
    match tool_type(tool.item()) {
        Some(ToolType::Pick(tier)) if pick_effective(block) => tier.efficiency(),
        Some(ToolType::Axe(tier)) if axe_effective(block) => tier.efficiency(),
        Some(ToolType::Shovel(tier)) if shovel_effective(block) => tier.efficiency(),
        // `ItemSword.getStrVsBlock` is 15 on web and 1.5 on everything else.
        Some(ToolType::Sword(_)) if block == Block::Cobweb => 15.0,
        Some(ToolType::Sword(_)) => 1.5,
        Some(ToolType::Shears) if is_leaves(block) || block == Block::Cobweb => 15.0,
        Some(ToolType::Shears) if block == Block::Wool => 5.0,
        _ => 1.0,
    }
}

/// `InventoryPlayer.canHarvestBlock`. Hand-harvestable materials win before
/// the tool is consulted. A false result still breaks the block, slowly, and
/// suppresses its drop.
pub fn can_harvest(tool: Option<ItemStack>, block: Block) -> bool {
    if harvestable_by_hand(block) {
        return true;
    }
    tool.is_some_and(|tool| tool_can_harvest(tool.item(), block))
}

fn tool_can_harvest(item: Item, block: Block) -> bool {
    match tool_type(item) {
        Some(ToolType::Pick(tier)) => pick_can_harvest(tier, block),
        // `ItemSpade.canHarvestBlock`: the snow layer and the snow block.
        Some(ToolType::Shovel(_)) => matches!(block, Block::Snow | Block::SnowLayer),
        // `ItemSword` and `ItemShears` harvest web only. Leaves are already
        // hand-harvestable; shears change the drop, not this gate.
        Some(ToolType::Sword(_) | ToolType::Shears) => block == Block::Cobweb,
        _ => false,
    }
}

/// `ItemPickaxe.canHarvestBlock`. Rock and iron fall through to any pick.
/// Iron blocks and the tiered ores are handled before that fallthrough.
fn pick_can_harvest(tier: ToolTier, block: Block) -> bool {
    let level = tier.level();
    if block == Block::Obsidian {
        return level == 3;
    }
    if matches!(block, Block::DiamondBlock | Block::DiamondOre) {
        return level >= 2;
    }
    if matches!(block, Block::GoldBlock | Block::GoldOre) {
        return level >= 2;
    }
    if matches!(block, Block::IronBlock | Block::IronOre) {
        return level >= 1;
    }
    if matches!(block, Block::LapisBlock | Block::LapisOre) {
        return level >= 1;
    }
    if matches!(block, Block::RedstoneOre | Block::LitRedstoneOre) {
        return level >= 2;
    }
    matches!(
        block,
        Block::Stone
            | Block::Cobblestone
            | Block::Bedrock
            | Block::CoalOre
            | Block::Dispenser
            | Block::Sandstone
            | Block::DoubleStoneSlab
            | Block::StoneSlab
            | Block::Bricks
            | Block::MossyCobblestone
            | Block::Furnace
            | Block::LitFurnace
            | Block::CobblestoneStairs
            | Block::StonePressurePlate
            | Block::IronDoor
            | Block::Netherrack
            | Block::Glowstone
    )
}

/// Damage from `onBlockDestroyed`. Picks, axes, and shovels always lose one
/// use. Swords lose two. Shears lose one on leaves and web. Hoes lose none.
pub fn break_durability(tool: ItemStack, block: Block) -> u16 {
    match tool_type(tool.item()) {
        Some(ToolType::Pick(_) | ToolType::Axe(_) | ToolType::Shovel(_)) => 1,
        Some(ToolType::Sword(_)) => 2,
        Some(ToolType::Shears) if is_leaves(block) || block == Block::Cobweb => 1,
        _ => 0,
    }
}

/// One tick of `Block.blockStrength` for this held stack.
pub fn mine_step(block: Block, tool: Option<ItemStack>, on_ground: bool, in_water: bool) -> f32 {
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
    block: Block,
    tool: Option<ItemStack>,
    on_ground: bool,
    in_water: bool,
) -> Option<u32> {
    if !block.is_breakable() {
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

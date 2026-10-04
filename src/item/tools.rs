//! Beta 1.7.3 tool speed, harvest, and block-break durability.
//!
//! `ItemPickaxe`, `ItemAxe`, `ItemSpade`, `ItemSword`, and `ItemShears`.
//! Speed lists are block identity checks. Harvest level is separate, and a
//! tool that cannot harvest a block mines at the slow fist rate.

use super::Item;
use super::ItemStack;
use crate::block::blocks::Block;
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

fn kind(id: Item) -> Option<Kind> {
    Some(match id {
        Item::WoodenPickaxe => Kind::Pick(Tier::Wood),
        Item::StonePickaxe => Kind::Pick(Tier::Stone),
        Item::IronPickaxe => Kind::Pick(Tier::Iron),
        Item::DiamondPickaxe => Kind::Pick(Tier::Diamond),
        Item::GoldPickaxe => Kind::Pick(Tier::Gold),
        Item::WoodenAxe => Kind::Axe(Tier::Wood),
        Item::StoneAxe => Kind::Axe(Tier::Stone),
        Item::IronAxe => Kind::Axe(Tier::Iron),
        Item::DiamondAxe => Kind::Axe(Tier::Diamond),
        Item::GoldAxe => Kind::Axe(Tier::Gold),
        Item::WoodenShovel => Kind::Shovel(Tier::Wood),
        Item::StoneShovel => Kind::Shovel(Tier::Stone),
        Item::IronShovel => Kind::Shovel(Tier::Iron),
        Item::DiamondShovel => Kind::Shovel(Tier::Diamond),
        Item::GoldShovel => Kind::Shovel(Tier::Gold),
        Item::WoodenSword => Kind::Sword(Tier::Wood),
        Item::StoneSword => Kind::Sword(Tier::Stone),
        Item::IronSword => Kind::Sword(Tier::Iron),
        Item::DiamondSword => Kind::Sword(Tier::Diamond),
        Item::GoldSword => Kind::Sword(Tier::Gold),
        Item::WoodenHoe => Kind::Hoe(Tier::Wood),
        Item::StoneHoe => Kind::Hoe(Tier::Stone),
        Item::IronHoe => Kind::Hoe(Tier::Iron),
        Item::DiamondHoe => Kind::Hoe(Tier::Diamond),
        Item::GoldHoe => Kind::Hoe(Tier::Gold),
        Item::Shears => Kind::Shears,
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
pub fn is_hoe(id: Item) -> bool {
    matches!(kind(id), Some(Kind::Hoe(_)))
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
    match kind(tool.item()) {
        Some(Kind::Pick(tier)) if pick_effective(block) => tier.efficiency(),
        Some(Kind::Axe(tier)) if axe_effective(block) => tier.efficiency(),
        Some(Kind::Shovel(tier)) if shovel_effective(block) => tier.efficiency(),
        // `ItemSword.getStrVsBlock` is 15 on web and 1.5 on everything else.
        Some(Kind::Sword(_)) if block == Block::Cobweb => 15.0,
        Some(Kind::Sword(_)) => 1.5,
        Some(Kind::Shears) if is_leaves(block) || block == Block::Cobweb => 15.0,
        Some(Kind::Shears) if block == Block::Wool => 5.0,
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

fn tool_can_harvest(id: Item, block: Block) -> bool {
    match kind(id) {
        Some(Kind::Pick(tier)) => pick_can_harvest(tier, block),
        // `ItemSpade.canHarvestBlock`: the snow layer and the snow block.
        Some(Kind::Shovel(_)) => matches!(block, Block::Snow | Block::SnowLayer),
        // `ItemSword` and `ItemShears` harvest web only. Leaves are already
        // hand-harvestable; shears change the drop, not this gate.
        Some(Kind::Sword(_) | Kind::Shears) => block == Block::Cobweb,
        _ => false,
    }
}

/// `ItemPickaxe.canHarvestBlock`. Rock and iron fall through to any pick.
/// Iron blocks and the tiered ores are handled before that fallthrough.
fn pick_can_harvest(tier: Tier, block: Block) -> bool {
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
    match kind(tool.item()) {
        Some(Kind::Pick(_) | Kind::Axe(_) | Kind::Shovel(_)) => 1,
        Some(Kind::Sword(_)) => 2,
        Some(Kind::Shears) if is_leaves(block) || block == Block::Cobweb => 1,
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

//! Beta 1.7.3 break drops.
//!
//! `player_break_drops` is `PlayerController.sendBlockRemoved`: `onBlockDestroyedByPlayer`,
//! then `harvestBlock` when `canHarvestBlock`. `natural_drops` is `dropBlockAsItem`
//! for a block that pops without a player, such as a torch whose support broke.
//! Each stack has count 1, matching one `EntityItem` per rolled drop.

use crate::block::blocks::Block;
use crate::item::Item;
use crate::item::ItemStack;
use crate::item::tools::can_harvest;

/// `java.util.Random.nextInt`. Tests pass scripted values; the game uses [`crate::entity::drops::items::ItemRng`].
pub trait DropRoll {
    fn next_int(&mut self, bound: u32) -> u32;
}

/// Items a player break spawns. The harvest gate is inside this function.
pub fn player_break_drops(
    block: Block,
    tool: Option<ItemStack>,
    rolls: &mut impl DropRoll,
) -> Vec<ItemStack> {
    player_break_drops_with_metadata(block, 0, tool, rolls)
}

/// [`player_break_drops`] for a block whose metadata changes its drops, such
/// as a crop's age.
pub fn player_break_drops_with_metadata(
    block: Block,
    metadata: u8,
    tool: Option<ItemStack>,
    rolls: &mut impl DropRoll,
) -> Vec<ItemStack> {
    let mut drops = Vec::new();
    // `BlockTNT.onBlockDestroyedByPlayer` runs even when the harvest drop is empty.
    // There is no primed metadata, so a player break always returns the block.
    if block == Block::Tnt {
        push_block(&mut drops, Block::Tnt, 1);
    }
    if can_harvest(tool, block) {
        push_harvest(&mut drops, block, metadata, tool, rolls);
    }
    drops
}

/// `Block.dropBlockAsItem` with chance 1. No shears shortcut and no harvest gate.
pub fn natural_drops(block: Block, rolls: &mut impl DropRoll) -> Vec<ItemStack> {
    natural_drops_with_metadata(block, 0, rolls)
}

/// [`natural_drops`] for a block whose metadata changes its drops.
pub fn natural_drops_with_metadata(
    block: Block,
    metadata: u8,
    rolls: &mut impl DropRoll,
) -> Vec<ItemStack> {
    let mut drops = Vec::new();
    push_natural(&mut drops, block, metadata, rolls);
    drops
}

fn push_harvest(
    drops: &mut Vec<ItemStack>,
    block: Block,
    metadata: u8,
    tool: Option<ItemStack>,
    rolls: &mut impl DropRoll,
) {
    // `BlockLeaves.harvestBlock`: shears drop the leaf, metadata kept in the low 2 bits.
    if block.is_leaves() && tool.is_some_and(|tool| tool.item() == Item::Shears) {
        push_block_state(drops, block, metadata, 1);
        return;
    }
    // `BlockSnow.harvestBlock` drops one snowball. `quantityDropped` is 0, so a
    // natural break (melt) drops nothing.
    if block == Block::SnowLayer {
        push_item(drops, Item::Snowball, 0, 1);
        return;
    }
    push_natural(drops, block, metadata, rolls);
}

fn push_natural(drops: &mut Vec<ItemStack>, block: Block, metadata: u8, rolls: &mut impl DropRoll) {
    match block {
        Block::Stone => push_block(drops, Block::Cobblestone, 1),
        Block::Grass | Block::Farmland => push_block(drops, Block::Dirt, 1),
        Block::CoalOre => push_item(drops, Item::Coal, 0, 1),
        Block::DiamondOre => push_item(drops, Item::Diamond, 0, 1),
        Block::LapisOre => {
            let count = 4 + rolls.next_int(5);
            push_item(drops, Item::Dye, 4, count);
        }
        Block::RedstoneOre | Block::LitRedstoneOre => {
            let count = 4 + rolls.next_int(2);
            push_item(drops, Item::Redstone, 0, count);
        }
        Block::Glowstone => {
            let count = 2 + rolls.next_int(3);
            push_item(drops, Item::GlowstoneDust, 0, count);
        }
        Block::Clay => push_item(drops, Item::ClayBall, 0, 4),
        Block::Gravel => {
            if rolls.next_int(10) == 0 {
                push_item(drops, Item::Flint, 0, 1);
            } else {
                push_block(drops, Block::Gravel, 1);
            }
        }
        Block::Snow => push_item(drops, Item::Snowball, 0, 4),
        Block::Cobweb => push_item(drops, Item::String, 0, 1),
        Block::Glass
        | Block::Ice
        | Block::Bookshelf
        | Block::Fire
        | Block::FlowingWater
        | Block::Water
        | Block::FlowingLava
        | Block::Lava
        | Block::NetherPortal
        | Block::MobSpawner
        | Block::PistonHead
        | Block::MovingPiston
        | Block::Cake
        | Block::DeadBush
        | Block::Tnt
        | Block::SnowLayer => {}
        Block::DoubleStoneSlab => push_block(drops, Block::StoneSlab, 2),
        Block::WoodenStairs => push_block(drops, Block::WoodenPlanks, 1),
        Block::CobblestoneStairs => push_block(drops, Block::Cobblestone, 1),
        Block::LitFurnace => push_block(drops, Block::Furnace, 1),
        Block::UnlitRedstoneTorch => push_block(drops, Block::RedstoneTorch, 1),
        Block::RedstoneWire => push_item(drops, Item::Redstone, 0, 1),
        Block::StandingSign | Block::WallSign => push_item(drops, Item::Sign, 0, 1),
        Block::WoodenDoor => push_item(drops, Item::WoodenDoor, 0, 1),
        Block::IronDoor => push_item(drops, Item::IronDoor, 0, 1),
        Block::SugarCane => push_item(drops, Item::SugarCane, 0, 1),
        Block::Bed => push_item(drops, Item::Bed, 0, 1),
        Block::Repeater | Block::PoweredRepeater => push_item(drops, Item::Repeater, 0, 1),
        Block::TallGrass => {
            if rolls.next_int(8) == 0 {
                push_item(drops, Item::Seeds, 0, 1);
            }
        }
        // `BlockCrops`: wheat only at age 7 (`idDropped`), then three seed
        // rolls in `dropBlockAsItemWithChance`, each kept when
        // `nextInt(15) <= age`.
        Block::Crops => {
            let age = u32::from(metadata.min(7));
            if age == 7 {
                push_item(drops, Item::Wheat, 0, 1);
            }
            for _ in 0..3 {
                if rolls.next_int(15) <= age {
                    push_item(drops, Item::Seeds, 0, 1);
                }
            }
        }
        Block::Leaves => {
            // `quantityDropped`: one sapling on `nextInt(20) == 0`, species in `damageDropped`.
            if rolls.next_int(20) == 0 {
                let species = u16::from(metadata & 3);
                push_item(
                    drops,
                    Item::from_block(Block::Sapling).expect("sapling has an item form"),
                    species,
                    1,
                );
            }
        }
        other => push_block_state(drops, other, metadata, 1),
    }
}

fn push_block(drops: &mut Vec<ItemStack>, block: Block, count: u32) {
    push_block_state(drops, block, 0, count);
}

/// Drop `count` of the item a block with `metadata` stacks as, keeping species.
fn push_block_state(drops: &mut Vec<ItemStack>, block: Block, metadata: u8, count: u32) {
    let Ok(stack) = ItemStack::from_block_state(block, metadata, 1) else {
        return;
    };
    for _ in 0..count {
        drops.push(stack);
    }
}

fn push_item(drops: &mut Vec<ItemStack>, item: Item, data: u16, count: u32) {
    let Ok(stack) = ItemStack::with_data(item, 1, data) else {
        return;
    };
    for _ in 0..count {
        drops.push(stack);
    }
}

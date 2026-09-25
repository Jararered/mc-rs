//! Beta 1.7.3 break drops.
//!
//! `player_break_drops` is `PlayerController.sendBlockRemoved`: `onBlockDestroyedByPlayer`,
//! then `harvestBlock` when `canHarvestBlock`. `natural_drops` is `dropBlockAsItem`
//! for a block that pops without a player, such as a torch whose support broke.
//! Each stack has count 1, matching one `EntityItem` per rolled drop.

use crate::block::block::BlockId;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::item::tools::can_harvest;

/// `java.util.Random.nextInt`. Tests pass scripted values; the game uses [`crate::entity::drops::items::ItemRng`].
pub trait DropRoll {
    fn next_int(&mut self, bound: u32) -> u32;
}

/// Items a player break spawns. The harvest gate is inside this function.
pub fn player_break_drops(
    block: BlockId,
    tool: Option<ItemStack>,
    rolls: &mut impl DropRoll,
) -> Vec<ItemStack> {
    let mut drops = Vec::new();
    // `BlockTNT.onBlockDestroyedByPlayer` runs even when the harvest drop is empty.
    // There is no primed metadata, so a player break always returns the block.
    if block == BlockId::Tnt {
        push_block(&mut drops, BlockId::Tnt, 1);
    }
    if can_harvest(tool, block) {
        push_harvest(&mut drops, block, tool, rolls);
    }
    drops
}

/// `Block.dropBlockAsItem` with chance 1. No shears shortcut and no harvest gate.
pub fn natural_drops(block: BlockId, rolls: &mut impl DropRoll) -> Vec<ItemStack> {
    let mut drops = Vec::new();
    push_natural(&mut drops, block, rolls);
    drops
}

fn push_harvest(
    drops: &mut Vec<ItemStack>,
    block: BlockId,
    tool: Option<ItemStack>,
    rolls: &mut impl DropRoll,
) {
    // `BlockLeaves.harvestBlock`: shears drop the leaf, metadata kept in the low 2 bits.
    if is_leaves(block) && tool.is_some_and(|tool| tool.item() == ItemId::Shears) {
        push_block(drops, block, 1);
        return;
    }
    // `BlockSnow.harvestBlock` drops one snowball. `quantityDropped` is 0, so a
    // natural break (melt) drops nothing.
    if block == BlockId::SnowLayer {
        push_item(drops, ItemId::Snowball, 0, 1);
        return;
    }
    push_natural(drops, block, rolls);
}

fn push_natural(drops: &mut Vec<ItemStack>, block: BlockId, rolls: &mut impl DropRoll) {
    match block {
        BlockId::Stone => push_block(drops, BlockId::Cobblestone, 1),
        BlockId::Grass | BlockId::Farmland => push_block(drops, BlockId::Dirt, 1),
        BlockId::CoalOre => push_item(drops, ItemId::Coal, 0, 1),
        BlockId::DiamondOre => push_item(drops, ItemId::Diamond, 0, 1),
        BlockId::LapisOre => {
            let count = 4 + rolls.next_int(5);
            push_item(drops, ItemId::Dye, 4, count);
        }
        BlockId::RedstoneOre | BlockId::LitRedstoneOre => {
            let count = 4 + rolls.next_int(2);
            push_item(drops, ItemId::Redstone, 0, count);
        }
        BlockId::Glowstone => {
            let count = 2 + rolls.next_int(3);
            push_item(drops, ItemId::GlowstoneDust, 0, count);
        }
        BlockId::Clay => push_item(drops, ItemId::ClayBall, 0, 4),
        BlockId::Gravel => {
            if rolls.next_int(10) == 0 {
                push_item(drops, ItemId::Flint, 0, 1);
            } else {
                push_block(drops, BlockId::Gravel, 1);
            }
        }
        BlockId::Snow => push_item(drops, ItemId::Snowball, 0, 4),
        BlockId::Cobweb => push_item(drops, ItemId::String, 0, 1),
        BlockId::Glass
        | BlockId::Ice
        | BlockId::Bookshelf
        | BlockId::Fire
        | BlockId::FlowingWater
        | BlockId::Water
        | BlockId::FlowingLava
        | BlockId::Lava
        | BlockId::NetherPortal
        | BlockId::MobSpawner
        | BlockId::PistonHead
        | BlockId::MovingPiston
        | BlockId::Cake
        | BlockId::DeadBush
        | BlockId::Tnt
        | BlockId::SnowLayer => {}
        BlockId::DoubleStoneSlab => push_block(drops, BlockId::StoneSlab, 2),
        BlockId::WoodenStairs => push_block(drops, BlockId::WoodenPlanks, 1),
        BlockId::CobblestoneStairs => push_block(drops, BlockId::Cobblestone, 1),
        BlockId::LitFurnace => push_block(drops, BlockId::Furnace, 1),
        BlockId::UnlitRedstoneTorch => push_block(drops, BlockId::RedstoneTorch, 1),
        BlockId::RedstoneWire => push_item(drops, ItemId::Redstone, 0, 1),
        BlockId::StandingSign | BlockId::WallSign => push_item(drops, ItemId::Sign, 0, 1),
        BlockId::WoodenDoor => push_item(drops, ItemId::WoodenDoor, 0, 1),
        BlockId::IronDoor => push_item(drops, ItemId::IronDoor, 0, 1),
        BlockId::SugarCane => push_item(drops, ItemId::SugarCane, 0, 1),
        BlockId::Bed => push_item(drops, ItemId::Bed, 0, 1),
        BlockId::Repeater | BlockId::PoweredRepeater => push_item(drops, ItemId::Repeater, 0, 1),
        BlockId::TallGrass | BlockId::Fern => {
            if rolls.next_int(8) == 0 {
                push_item(drops, ItemId::Seeds, 0, 1);
            }
        }
        // Crop age is not stored. Age 0 never drops wheat (`idDropped` only at 7).
        // `dropBlockAsItemWithChance` still rolls seeds three times: `nextInt(15) <= age`.
        BlockId::Crops => {
            for _ in 0..3 {
                if rolls.next_int(15) == 0 {
                    push_item(drops, ItemId::Seeds, 0, 1);
                }
            }
        }
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => {
            // `quantityDropped`: one sapling on `nextInt(20) == 0`, species in `damageDropped`.
            if rolls.next_int(20) == 0 {
                let species = block.item_form().1 as u16;
                push_item(
                    drops,
                    ItemId::from_block(BlockId::Sapling).expect("sapling has an item form"),
                    species,
                    1,
                );
            }
        }
        other => push_block(drops, other, 1),
    }
}

fn is_leaves(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves
    )
}

fn push_block(drops: &mut Vec<ItemStack>, block: BlockId, count: u32) {
    let Ok(stack) = ItemStack::from_block(block, 1) else {
        return;
    };
    for _ in 0..count {
        drops.push(stack);
    }
}

fn push_item(drops: &mut Vec<ItemStack>, item: ItemId, data: u16, count: u32) {
    let Ok(stack) = ItemStack::with_data(item, 1, data) else {
        return;
    };
    for _ in 0..count {
        drops.push(stack);
    }
}

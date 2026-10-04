//! Beta 1.7.3 furnace recipes, fuels, and block-local smelting state.

use bevy::math::IVec3;
use bevy::prelude::Res;
use bevy::prelude::ResMut;

use crate::block::blocks::Block;
use crate::item::Item;
use crate::item::ItemStack;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

pub const SMELT_TICKS: u16 = 200;
pub const FURNACE_SLOTS: usize = 3;

/// Slots follow Beta `TileEntityFurnace`: input, fuel, output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Furnace {
    pub slots: [Option<ItemStack>; FURNACE_SLOTS],
    pub burn_ticks: u16,
    pub fuel_ticks: u16,
    pub cook_ticks: u16,
}

impl Furnace {
    pub fn is_burning(&self) -> bool {
        self.burn_ticks > 0
    }

    /// Advance one Beta world tick. Returns whether the block's lit state changed.
    pub fn tick(&mut self) -> bool {
        let was_burning = self.is_burning();
        if self.burn_ticks > 0 {
            self.burn_ticks -= 1;
        }

        if self.burn_ticks == 0 && self.can_smelt() {
            if let Some(fuel) = self.slots[1]
                && let Some(duration) = fuel_ticks(fuel)
            {
                self.burn_ticks = duration;
                self.fuel_ticks = duration;
                consume_one(&mut self.slots[1]);
            }
        }

        if self.can_smelt() {
            if self.is_burning() {
                self.cook_ticks += 1;
                if self.cook_ticks >= SMELT_TICKS {
                    self.cook_ticks = 0;
                    self.smelt_one();
                }
            }
        } else {
            self.cook_ticks = 0;
        }

        was_burning != self.is_burning()
    }

    pub fn can_smelt(&self) -> bool {
        let Some(input) = self.slots[0] else {
            return false;
        };
        let Some(result) = smelting_result(input) else {
            return false;
        };
        match self.slots[2] {
            None => true,
            Some(output) if output.item() == result.item() && output.data() == result.data() => {
                output.count() < output.definition().max_stack_size
            }
            _ => false,
        }
    }

    fn smelt_one(&mut self) {
        if !self.can_smelt() {
            return;
        }
        let Some(input) = self.slots[0] else {
            return;
        };
        let Some(result) = smelting_result(input) else {
            return;
        };
        match &mut self.slots[2] {
            Some(output) => {
                let _ = output.merge(result);
            }
            slot @ None => *slot = Some(result),
        }
        consume_one(&mut self.slots[0]);
    }
}

/// Complete Beta 1.7.3 smelting map. Log species share the same charcoal result.
pub fn smelting_result(input: ItemStack) -> Option<ItemStack> {
    let (item, data) = (input.item().block(), input.data());
    let result = match (item, input.item()) {
        (Some(Block::IronOre), _) => ItemStack::new(Item::IronIngot, 1).ok(),
        (Some(Block::GoldOre), _) => ItemStack::new(Item::GoldIngot, 1).ok(),
        (Some(Block::DiamondOre), _) => ItemStack::new(Item::Diamond, 1).ok(),
        (Some(Block::Sand), _) => ItemStack::from_block(Block::Glass, 1).ok(),
        (Some(Block::Cobblestone), _) => ItemStack::from_block(Block::Stone, 1).ok(),
        (Some(Block::Cactus), _) => ItemStack::with_data(Item::Dye, 1, 2).ok(),
        (Some(Block::Wood), _) if data <= 2 => ItemStack::with_data(Item::Coal, 1, 1).ok(),
        (_, Item::RawPorkchop) => ItemStack::new(Item::CookedPorkchop, 1).ok(),
        (_, Item::RawFish) => ItemStack::new(Item::CookedFish, 1).ok(),
        (_, Item::ClayBall) => ItemStack::new(Item::Brick, 1).ok(),
        _ => None,
    }?;
    Some(result)
}

/// Beta `TileEntityFurnace.getItemBurnTime` durations, in world ticks.
pub fn fuel_ticks(fuel: ItemStack) -> Option<u16> {
    match fuel.item() {
        Item::Coal => Some(1_600),
        Item::Stick => Some(100),
        Item::LavaBucket => Some(20_000),
        item if item.block() == Some(Block::Sapling) => Some(100),
        item if item.block().is_some_and(is_wood_material) => Some(300),
        _ => None,
    }
}

pub(crate) fn is_wood_material(block: Block) -> bool {
    matches!(
        block,
        Block::Wood
            | Block::WoodenPlanks
            | Block::Chest
            | Block::CraftingTable
            | Block::Bookshelf
            | Block::WoodenStairs
            | Block::WoodenDoor
            | Block::Fence
            | Block::Trapdoor
            | Block::WoodenPressurePlate
            | Block::NoteBlock
            | Block::Jukebox
            | Block::LockedChest
            | Block::StandingSign
            | Block::WallSign
    )
}

fn consume_one(slot: &mut Option<ItemStack>) {
    let Some(stack) = *slot else {
        return;
    };
    *slot = ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
}

pub fn tick_furnaces(
    tick: Res<WorldTick>,
    mut chunks: ResMut<WorldChunks>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }

    let positions = chunks.furnace_positions();
    for (x, y, z) in positions {
        let mut burning = false;
        let mut changed = false;
        if let Some(furnace) = chunks.furnace_at_mut(x, y, z) {
            let before = furnace.clone();
            for _ in 0..ticks {
                furnace.tick();
            }
            burning = furnace.is_burning();
            changed = *furnace != before;
        }
        let block = chunks.block_at(x, y, z);
        if let Some(block) = block
            && block.is_furnace()
            && (block == Block::LitFurnace) != burning
        {
            let next = if burning {
                Block::LitFurnace
            } else {
                Block::Furnace
            };
            // The swap keeps the furnace's facing.
            let facing = chunks.metadata_at(x, y, z);
            chunks.set_block_with_metadata(x, y, z, next, facing);
            // `BlockFurnace.updateFurnaceBlockState` swaps with notify.
            if let Some(block_ticks) = block_ticks.as_deref_mut() {
                block_ticks.block_changed(IVec3::new(x, y, z), block, facing);
            }
            if let Some(streaming) = streaming.as_deref_mut() {
                streaming.request_block_update(x, y, z);
            }
        }
        if changed && let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPosition::from_block(x, z));
        }
    }
}

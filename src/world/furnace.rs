//! Beta 1.7.3 furnace recipes, fuels, and block-local smelting state.

use bevy::prelude::Res;
use bevy::prelude::ResMut;

use crate::item::ItemId;
use crate::item::ItemStack;
use crate::world::block::block::BlockId;
use crate::world::chunk::ChunkPos;
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
    let (item, data) = match input.item() {
        ItemId::Block(block) => (Some(block), input.data()),
        _ => (None, input.data()),
    };
    let result = match (item, input.item()) {
        (Some(BlockId::IronOre), _) => ItemStack::new(ItemId::IronIngot, 1).ok(),
        (Some(BlockId::GoldOre), _) => ItemStack::new(ItemId::GoldIngot, 1).ok(),
        (Some(BlockId::DiamondOre), _) => ItemStack::new(ItemId::Diamond, 1).ok(),
        (Some(BlockId::Sand), _) => ItemStack::from_block(BlockId::Glass, 1).ok(),
        (Some(BlockId::Cobblestone), _) => ItemStack::from_block(BlockId::Stone, 1).ok(),
        (Some(BlockId::Cactus), _) => ItemStack::with_data(ItemId::Dye, 1, 2).ok(),
        (Some(BlockId::Wood), _) if data <= 2 => ItemStack::with_data(ItemId::Coal, 1, 1).ok(),
        (_, ItemId::RawPorkchop) => ItemStack::new(ItemId::CookedPorkchop, 1).ok(),
        (_, ItemId::RawFish) => ItemStack::new(ItemId::CookedFish, 1).ok(),
        (_, ItemId::ClayBall) => ItemStack::new(ItemId::Brick, 1).ok(),
        _ => None,
    }?;
    Some(result)
}

/// Beta `TileEntityFurnace.getItemBurnTime` durations, in world ticks.
pub fn fuel_ticks(fuel: ItemStack) -> Option<u16> {
    match fuel.item() {
        ItemId::Coal => Some(1_600),
        ItemId::Stick => Some(100),
        ItemId::LavaBucket => Some(20_000),
        ItemId::Block(BlockId::Sapling) => Some(100),
        ItemId::Block(block) if is_wood_material(block) => Some(300),
        _ => None,
    }
}

fn is_wood_material(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Wood
            | BlockId::SpruceWood
            | BlockId::BirchWood
            | BlockId::WoodenPlanks
            | BlockId::SprucePlanks
            | BlockId::BirchPlanks
            | BlockId::Chest
            | BlockId::CraftingTable
            | BlockId::Bookshelf
            | BlockId::WoodenStairs
            | BlockId::WoodenDoor
            | BlockId::Fence
            | BlockId::Trapdoor
            | BlockId::WoodenPressurePlate
            | BlockId::NoteBlock
            | BlockId::Jukebox
            | BlockId::LockedChest
            | BlockId::StandingSign
            | BlockId::WallSign
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
            && block.is_lit_furnace() != burning
        {
            let next = block.with_furnace_lit(burning);
            chunks.set_block(x, y, z, next);
            if let Some(streaming) = streaming.as_deref_mut() {
                let center = ChunkPos::from_block(x, z);
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        streaming.request_remesh(ChunkPos {
                            x: center.x + dx,
                            z: center.z + dz,
                        });
                    }
                }
            }
        }
        if changed && let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPos::from_block(x, z));
        }
    }
}

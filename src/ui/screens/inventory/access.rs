//! What the open screen's slots live in: the player's inventory, the
//! workbench grid, and the furnace or chest in the world.

use super::Slot;
use super::chest::read_chest_group_slots;
use super::clicks::to_slot_id;
use crate::crafting::beta_recipe_book;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::item::ItemStack;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use bevy::prelude::*;

/// Everything a click on the open screen can change. The click rules are
/// its methods, in `clicks.rs` and `chest.rs`. The hotbar and inventory stay
/// behind `Mut` so only a click that writes one marks it changed; the HUD
/// redraws on `Changed<Hotbar>`.
pub(super) struct SlotAccess<'a> {
    pub(super) screen: &'a InventorySession,
    pub(super) hotbar: Mut<'a, Hotbar>,
    pub(super) inventory: Mut<'a, Inventory>,
    pub(super) workbench: &'a mut ActiveWorkbench,
    pub(super) chunks: &'a mut WorldChunks,
    pub(super) persistence: Option<&'a mut WorldPersistence>,
}

impl SlotAccess<'_> {
    pub(super) fn mark_dirty(&mut self, position: ChunkPosition) {
        if let Some(persistence) = self.persistence.as_deref_mut() {
            persistence.mark_dirty(position);
        }
    }

    pub(super) fn mark_chest_dirty(&mut self, group: ChestGroup) {
        self.mark_dirty(ChunkPosition::from_block(group.first.0, group.first.2));
        if let Some((x, _, z)) = group.second {
            self.mark_dirty(ChunkPosition::from_block(x, z));
        }
    }
}

/// The same slots, read only, for drawing.
#[derive(Clone, Copy)]
pub(super) struct SlotView<'a> {
    pub(super) screen: &'a InventorySession,
    pub(super) hotbar: &'a Hotbar,
    pub(super) inventory: &'a Inventory,
    pub(super) workbench: &'a ActiveWorkbench,
    pub(super) chunks: &'a WorldChunks,
}

impl SlotView<'_> {
    /// What `slot` holds.
    pub(super) fn stack(self, slot: Slot) -> Option<ItemStack> {
        match slot {
            Slot::Hotbar(i) => self.hotbar.slots[i],
            Slot::Main(i) => self.inventory.main[i],
            Slot::Craft(i) => self.inventory.crafting[i],
            Slot::CraftResult if self.screen.workbench => {
                beta_recipe_book().find(&self.workbench.grid)
            }
            Slot::CraftResult => self.inventory.crafting_result(),
            Slot::Workbench(i) => self.workbench.grid.get(i % 3, i / 3),
            Slot::Chest(i) => self
                .screen
                .chest_group
                .map(|group| read_chest_group_slots(self.chunks, group))
                .and_then(|slots| slots.get(i).copied().flatten()),
            Slot::Furnace(i) => self
                .screen
                .furnace_position
                .and_then(|(x, y, z)| self.chunks.furnace_at(x, y, z))
                .and_then(|furnace| furnace.slots.get(i).copied().flatten()),
            Slot::Armor(i) => self.inventory.armor[i],
        }
    }

    /// What `slot` shows: a drag's `preview` of it, or what it holds.
    pub(super) fn displayed(
        self,
        slot: Slot,
        preview: &[(SlotId, ItemStack)],
    ) -> Option<ItemStack> {
        preview
            .iter()
            .find_map(|(preview_slot, stack)| (*preview_slot == to_slot_id(slot)).then_some(*stack))
            .or_else(|| self.stack(slot))
    }
}

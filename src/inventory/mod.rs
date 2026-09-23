mod transfer;

use bevy::prelude::Component;

use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::item::ItemStack;

pub use transfer::DragPlace;
pub use transfer::SlotId;
pub use transfer::chest_drag_place;
pub use transfer::chest_slot_accepts_drag;
pub use transfer::collect_matching_stacks;
pub use transfer::drag_place;
pub use transfer::hotbar_key_swap;
pub use transfer::hotbar_key_swap_chest;
pub use transfer::preview_chest_drag_place;
pub use transfer::preview_drag_place;
pub use transfer::shift_click_chest_slot;
pub use transfer::shift_click_slot;
pub use transfer::slot_accepts_drag;

/// Number of hotbar slots shown on the in-game HUD.
pub const HOTBAR_SLOTS: usize = 9;
pub const MAIN_SLOTS: usize = 27;

#[derive(Component, Clone, Debug, Default)]
pub struct Inventory {
    pub main: [Option<ItemStack>; MAIN_SLOTS],
    pub crafting: [Option<ItemStack>; 4],
    pub armor: [Option<ItemStack>; 4],
    pub carried: Option<ItemStack>,
}

impl Inventory {
    pub fn crafting_result(&self) -> Option<ItemStack> {
        beta_recipe_book().find(&CraftingGrid::from_slots(2, 2, &self.crafting))
    }

    /// Atomically picks up one craft result. Inputs are changed only if the
    /// output, container remainders, and carried-stack merge can all fit.
    pub fn take_crafting_result(&mut self, hotbar: &mut Hotbar) -> bool {
        let mut grid = CraftingGrid::from_slots(2, 2, &self.crafting);
        let Some(output) = beta_recipe_book().find(&grid) else {
            return false;
        };
        let mut simulated_inventory = self.clone();
        let mut simulated_hotbar = hotbar.clone();
        let Some(remainders) = beta_recipe_book().consume_one(&mut grid) else {
            return false;
        };
        simulated_inventory.crafting = [None; 4];
        for (slot, value) in simulated_inventory.crafting.iter_mut().zip(grid.slots()) {
            *slot = value;
        }
        for remainder in remainders {
            if simulated_inventory
                .insert(&mut simulated_hotbar, remainder)
                .is_some()
            {
                return false;
            }
        }
        match simulated_inventory.carried {
            None => simulated_inventory.carried = Some(output),
            Some(mut carried)
                if carried.item() == output.item() && carried.data() == output.data() =>
            {
                if carried.merge(output).is_some() {
                    return false;
                }
                simulated_inventory.carried = Some(carried);
            }
            Some(_) => return false,
        }
        *self = simulated_inventory;
        *hotbar = simulated_hotbar;
        true
    }

    pub fn insert(&mut self, hotbar: &mut Hotbar, stack: ItemStack) -> Option<ItemStack> {
        let mut remainder = hotbar.insert(stack)?;
        for slot in self.main.iter_mut().flatten() {
            remainder = slot.merge(remainder)?;
        }
        for slot in &mut self.main {
            if slot.is_none() {
                *slot = Some(remainder);
                return None;
            }
        }
        Some(remainder)
    }
}

/// Player hotbar: nine slots and the currently selected index.
///
/// Slots start empty. Insertion uses registered stack limits and item data.
#[derive(Component, Clone, Debug)]
pub struct Hotbar {
    pub slots: [Option<ItemStack>; HOTBAR_SLOTS],
    pub selected: usize,
    /// Beta `ItemStack.animationsToGo` for each hotbar slot. Five ticks of pop
    /// after a pickup increases that slot's count.
    pub pop: [u8; HOTBAR_SLOTS],
}

impl Default for Hotbar {
    fn default() -> Self {
        Self {
            slots: [None; HOTBAR_SLOTS],
            selected: 0,
            pop: [0; HOTBAR_SLOTS],
        }
    }
}

impl Hotbar {
    /// Merge matching stacks before filling empty slots. Return the remainder
    /// when full, without discarding any items.
    pub fn insert(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
        let before = self.counts();
        let remainder = (|| {
            for slot in self.slots.iter_mut().flatten() {
                stack = slot.merge(stack)?;
            }
            for slot in &mut self.slots {
                if slot.is_none() {
                    *slot = Some(stack);
                    return None;
                }
            }
            Some(stack)
        })();
        self.note_gains(&before);
        remainder
    }

    /// Take up to `count` items from the selected slot. An empty slot returns nothing.
    pub fn take_selected(&mut self, count: u8) -> Option<ItemStack> {
        let current = self.slots.get(self.selected).copied().flatten()?;
        if count == 0 {
            return None;
        }
        let taken = count.min(current.count());
        let stack = ItemStack::with_data(current.item(), taken, current.data()).ok()?;
        let left = current.count() - taken;
        self.slots[self.selected] = if left == 0 {
            None
        } else {
            ItemStack::with_data(current.item(), left, current.data()).ok()
        };
        Some(stack)
    }

    fn counts(&self) -> [u8; HOTBAR_SLOTS] {
        std::array::from_fn(|index| self.slots[index].map(|stack| stack.count()).unwrap_or(0))
    }

    fn note_gains(&mut self, before: &[u8; HOTBAR_SLOTS]) {
        for index in 0..HOTBAR_SLOTS {
            let after = self.slots[index].map(|stack| stack.count()).unwrap_or(0);
            if after > before[index] {
                self.pop[index] = 5;
            }
        }
    }

    pub fn select(&mut self, slot: usize) {
        if slot < HOTBAR_SLOTS {
            self.selected = slot;
        }
    }

    /// Minecraft Beta: a positive wheel notch moves the selection left.
    pub fn scroll(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let step = if delta > 0 { -1 } else { 1 };
        self.selected = (self.selected as i32 + step).rem_euclid(HOTBAR_SLOTS as i32) as usize;
    }

    pub fn selected_stack(&self) -> Option<ItemStack> {
        self.slots[self.selected]
    }

    /// Apply block-break durability to the selected stack. A broken tool
    /// leaves the slot empty.
    pub fn damage_selected(&mut self, amount: u16) {
        let Some(stack) = self.selected_stack() else {
            return;
        };
        self.slots[self.selected] = stack.apply_damage(amount);
    }
}

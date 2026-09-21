use bevy::prelude::Component;

use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::item::ItemStack;

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
}

impl Default for Hotbar {
    fn default() -> Self {
        Self {
            slots: [None; HOTBAR_SLOTS],
            selected: 0,
        }
    }
}

impl Hotbar {
    /// Merge matching stacks before filling empty slots. Return the remainder
    /// when full, without discarding any items.
    pub fn insert(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
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
}

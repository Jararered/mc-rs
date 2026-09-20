use bevy::prelude::Component;

use crate::item::ItemStack;

/// Number of hotbar slots shown on the in-game HUD.
pub const HOTBAR_SLOTS: usize = 9;

/// Player hotbar: nine slots and the currently selected index.
///
/// Items are not implemented yet; slots stay empty until the item system fills
/// them. Selection (scroll and number keys) already works.
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

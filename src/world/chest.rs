//! Block-local storage for chests.

use crate::item::ItemStack;

pub const CHEST_SLOTS: usize = 27;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chest {
    pub slots: [Option<ItemStack>; CHEST_SLOTS],
}

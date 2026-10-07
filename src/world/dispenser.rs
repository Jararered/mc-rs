//! Block-local nine-slot dispenser inventory, matching `TileEntityDispenser`.
use crate::item::ItemStack;

pub const DISPENSER_SLOTS: usize = 9;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dispenser {
    pub slots: [Option<ItemStack>; DISPENSER_SLOTS],
}

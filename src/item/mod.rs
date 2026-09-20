//! Placeholder item types.
//!
//! A real item registry, tools, and durability come later. Hotbar slots already
//! store [`ItemStack`] so that work can fill them without changing selection.

/// Identifier for an item. Values are not assigned until the item system exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemId(pub u16);

/// A stack of one item type.
///
/// Empty hotbar slots are [`None`] rather than a zero-count stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: u8,
}

impl ItemStack {
    pub fn new(item: ItemId, count: u8) -> Self {
        Self {
            item,
            count: count.max(1),
        }
    }
}

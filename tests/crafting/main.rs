mod crafting_grid;
mod recipe_book;
mod recipe_matching;
mod transfer;

use game::block::id::Id;
use game::item::ItemId;
use game::item::ItemStack;

pub(crate) fn stack(item: ItemId, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

pub(crate) fn block(block: Id) -> ItemId {
    ItemId::from_block(block).unwrap()
}

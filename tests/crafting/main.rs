mod crafting_grid;
mod recipe_book;
mod recipe_matching;
mod transfer;

use game::item::ItemId;
use game::item::ItemStack;
use game::world::block::block::BlockId;

pub(crate) fn stack(item: ItemId, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

pub(crate) fn block(block: BlockId) -> ItemId {
    ItemId::from_block(block).unwrap()
}

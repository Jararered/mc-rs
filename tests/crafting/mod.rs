mod crafting_grid;
mod recipe_book;
mod recipe_matching;
mod transfer;

use game::block::blocks::Block;
use game::item::Item;
use game::item::ItemStack;

pub(crate) fn stack(item: Item, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

pub(crate) fn block(block: Block) -> Item {
    Item::from_block(block).unwrap()
}

use game::block::blocks::Block;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;

use super::block;
use super::stack;

#[test]
fn player_result_pickup_is_atomic_when_carried_conflicts() {
    let mut inventory = Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(Block::Wood), 1));
    inventory.carried = Some(stack(Item::StoneSword, 1));
    assert!(!inventory.take_crafting_result(&mut hotbar));
    assert_eq!(inventory.crafting[0].unwrap().item(), block(Block::Wood));
}

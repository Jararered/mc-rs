use game::inventory::HOTBAR_SLOTS;
use game::inventory::Hotbar;
use game::item::ItemId;
use game::item::ItemStack;

#[test]
fn hotbar_starts_empty_on_slot_zero() {
    let hotbar = Hotbar::default();
    assert_eq!(hotbar.selected, 0);
    assert_eq!(hotbar.slots.len(), HOTBAR_SLOTS);
    assert!(hotbar.slots.iter().all(Option::is_none));
    assert_eq!(hotbar.selected_stack(), None);
}

#[test]
fn scroll_wraps_around_the_hotbar() {
    let mut hotbar = Hotbar::default();

    hotbar.scroll(1);
    assert_eq!(hotbar.selected, HOTBAR_SLOTS - 1);

    hotbar.scroll(1);
    assert_eq!(hotbar.selected, HOTBAR_SLOTS - 2);

    hotbar.scroll(-1);
    hotbar.scroll(-1);
    assert_eq!(hotbar.selected, 0);

    hotbar.scroll(-1);
    assert_eq!(hotbar.selected, 1);
}

#[test]
fn select_clamps_to_valid_slots() {
    let mut hotbar = Hotbar::default();
    hotbar.select(8);
    assert_eq!(hotbar.selected, 8);
    hotbar.select(9);
    assert_eq!(hotbar.selected, 8);
}

#[test]
fn selected_stack_reads_the_registered_item() {
    let mut hotbar = Hotbar::default();
    hotbar.slots[2] = Some(ItemStack::new(ItemId(1), 4).unwrap());
    hotbar.select(2);
    assert_eq!(
        hotbar.selected_stack(),
        Some(ItemStack::new(ItemId(1), 4).unwrap())
    );
    hotbar.select(0);
    assert_eq!(hotbar.selected_stack(), None);
}

#[test]
fn inventory_merges_into_hotbar_then_main() {
    use game::inventory::Inventory;
    let mut hotbar = Hotbar::default();
    for slot in &mut hotbar.slots {
        *slot = Some(ItemStack::new(ItemId::COAL, 64).unwrap());
    }
    let mut inventory = Inventory::default();
    assert_eq!(
        inventory.insert(&mut hotbar, ItemStack::new(ItemId::DIAMOND, 3).unwrap()),
        None
    );
    assert_eq!(inventory.main[0].unwrap().count(), 3);
}

#[test]
fn stored_player_round_trips_inventory_and_rejects_invalid_stacks() {
    use bevy::prelude::Transform;
    use game::inventory::Inventory;
    use game::world::persistence::StoredPlayer;
    let mut hotbar = Hotbar::default();
    hotbar.slots[2] = Some(ItemStack::with_data(ItemId::COAL, 7, 1).unwrap());
    hotbar.select(2);
    let mut inventory = Inventory::default();
    inventory.main[5] = Some(ItemStack::new(ItemId::DIAMOND, 3).unwrap());
    inventory.carried = Some(ItemStack::new(ItemId::IRON_PICKAXE, 1).unwrap());
    let stored =
        StoredPlayer::from_transform(&Transform::default()).with_inventory(&hotbar, &inventory);
    let encoded = serde_json::to_string(&stored).unwrap();
    let decoded: StoredPlayer = serde_json::from_str(&encoded).unwrap();
    let (bar, bag) = decoded.to_inventory();
    assert_eq!(bar.selected, 2);
    assert_eq!(bar.slots[2], hotbar.slots[2]);
    assert_eq!(bag.main[5], inventory.main[5]);
    assert_eq!(bag.carried, inventory.carried);
    let mut invalid = decoded;
    invalid.main[5] = Some(game::world::persistence::StoredStack {
        id: 264,
        count: 65,
        data: 0,
    });
    assert_eq!(invalid.to_inventory().1.main[5], None);
}

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

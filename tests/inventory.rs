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
    hotbar.slots[2] = Some(ItemStack::new(ItemId::from_u16(1).unwrap(), 4).unwrap());
    hotbar.select(2);
    assert_eq!(
        hotbar.selected_stack(),
        Some(ItemStack::new(ItemId::from_u16(1).unwrap(), 4).unwrap())
    );
    hotbar.select(0);
    assert_eq!(hotbar.selected_stack(), None);
}

#[test]
fn inventory_merges_into_hotbar_then_main() {
    use game::inventory::Inventory;
    let mut hotbar = Hotbar::default();
    for slot in &mut hotbar.slots {
        *slot = Some(ItemStack::new(ItemId::Coal, 64).unwrap());
    }
    let mut inventory = Inventory::default();
    assert_eq!(
        inventory.insert(&mut hotbar, ItemStack::new(ItemId::Diamond, 3).unwrap()),
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
    hotbar.slots[2] = Some(ItemStack::with_data(ItemId::Coal, 7, 1).unwrap());
    hotbar.select(2);
    let mut inventory = Inventory::default();
    inventory.main[5] = Some(ItemStack::new(ItemId::Diamond, 3).unwrap());
    inventory.carried = Some(ItemStack::new(ItemId::IronPickaxe, 1).unwrap());
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

fn stack(item: ItemId, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

fn block(block: game::world::block::block::BlockId) -> ItemId {
    ItemId::from_block(block).unwrap()
}

fn fill_storage(inventory: &mut game::inventory::Inventory, hotbar: &mut Hotbar, item: ItemStack) {
    for slot in &mut inventory.main {
        *slot = Some(item);
    }
    for slot in &mut hotbar.slots {
        *slot = Some(item);
    }
}

#[test]
fn shift_click_moves_hotbar_into_main_and_main_into_hotbar() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(stack(ItemId::Diamond, 5));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(0)
    ));
    assert!(hotbar.slots[0].is_none());
    assert_eq!(inventory.main[0], Some(stack(ItemId::Diamond, 5)));

    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Main(0)
    ));
    assert!(inventory.main[0].is_none());
    assert_eq!(hotbar.slots[0], Some(stack(ItemId::Diamond, 5)));
}

#[test]
fn shift_click_merges_before_using_an_empty_slot() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.main[3] = Some(stack(ItemId::Diamond, 60));
    hotbar.slots[4] = Some(stack(ItemId::Diamond, 10));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(4)
    ));
    assert!(hotbar.slots[4].is_none());
    assert_eq!(inventory.main[3], Some(stack(ItemId::Diamond, 64)));
    assert_eq!(inventory.main[0], Some(stack(ItemId::Diamond, 6)));
}

#[test]
fn shift_click_from_hotbar_does_not_spill_into_other_hotbar_slots() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    fill_storage(&mut inventory, &mut hotbar, stack(ItemId::Coal, 64));
    hotbar.slots[0] = Some(stack(ItemId::Diamond, 4));
    hotbar.slots[1] = None;
    assert!(!shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(0)
    ));
    assert_eq!(hotbar.slots[0], Some(stack(ItemId::Diamond, 4)));
    assert!(hotbar.slots[1].is_none());
}

#[test]
fn shift_click_returns_crafting_inputs_to_main_storage_first() {
    use game::crafting::CraftingGrid;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[2] = Some(stack(ItemId::Diamond, 3));
    inventory.armor[0] = Some(stack(ItemId::DiamondSword, 1));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Craft(2)
    ));
    assert!(inventory.crafting[2].is_none());
    assert_eq!(inventory.main[0], Some(stack(ItemId::Diamond, 3)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Armor(0)
    ));
    assert!(inventory.armor[0].is_none());
    assert_eq!(inventory.main[1], Some(stack(ItemId::DiamondSword, 1)));

    let mut grid = CraftingGrid::workbench();
    grid.set(2, 1, Some(stack(ItemId::Stick, 6)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        Some(&mut grid),
        true,
        SlotId::Workbench(5)
    ));
    assert!(grid.get(2, 1).is_none());
    assert_eq!(inventory.main[2], Some(stack(ItemId::Stick, 6)));
}

#[test]
fn shift_click_crafting_result_repeats_and_fills_the_hotbar_from_the_right() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    use game::world::block::block::BlockId;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(BlockId::Wood), 2));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult
    ));
    assert!(inventory.crafting.iter().all(Option::is_none));
    assert_eq!(
        hotbar.slots[8],
        Some(stack(block(BlockId::WoodenPlanks), 8))
    );
    assert!(hotbar.slots[..8].iter().all(Option::is_none));
}

#[test]
fn shift_click_crafting_stops_when_the_next_output_does_not_fit() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    use game::world::block::block::BlockId;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    fill_storage(&mut inventory, &mut hotbar, stack(ItemId::Diamond, 64));
    hotbar.slots[8] = Some(stack(block(BlockId::WoodenPlanks), 60));
    inventory.crafting[0] = Some(stack(block(BlockId::Wood), 2));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult
    ));
    assert_eq!(inventory.crafting[0], Some(stack(block(BlockId::Wood), 1)));
    assert_eq!(
        hotbar.slots[8],
        Some(stack(block(BlockId::WoodenPlanks), 64))
    );
    assert_eq!(hotbar.slots[7], Some(stack(ItemId::Diamond, 64)));
}

#[test]
fn shift_click_workbench_result_uses_the_three_by_three_grid() {
    use game::crafting::CraftingGrid;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    use game::world::block::block::BlockId;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut grid = CraftingGrid::workbench();
    grid.set(0, 0, Some(stack(block(BlockId::Wood), 1)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        Some(&mut grid),
        true,
        SlotId::CraftResult
    ));
    assert!(grid.get(0, 0).is_none());
    assert_eq!(
        hotbar.slots[8],
        Some(stack(block(BlockId::WoodenPlanks), 4))
    );
}

#[test]
fn left_drag_splits_a_stack_and_keeps_the_remainder() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(ItemId::Diamond, 64));
    inventory.main[1] = Some(stack(ItemId::Diamond, 10));
    let slots = [SlotId::Main(0), SlotId::Main(1), SlotId::Main(2)];
    assert!(drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &slots,
        DragPlace::Split
    ));
    assert_eq!(inventory.main[0], Some(stack(ItemId::Diamond, 21)));
    assert_eq!(inventory.main[1], Some(stack(ItemId::Diamond, 31)));
    assert_eq!(inventory.main[2], Some(stack(ItemId::Diamond, 21)));
    assert_eq!(inventory.carried, Some(stack(ItemId::Diamond, 1)));
}

#[test]
fn right_drag_places_one_item_in_each_slot() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(ItemId::Diamond, 5));
    inventory.main[2] = Some(stack(ItemId::Diamond, 63));
    inventory.main[1] = Some(stack(ItemId::Coal, 1));
    let slots = [
        SlotId::Main(0),
        SlotId::Main(1),
        SlotId::Main(2),
        SlotId::Main(3),
    ];
    assert!(drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &slots,
        DragPlace::OneEach
    ));
    assert_eq!(inventory.main[0], Some(stack(ItemId::Diamond, 1)));
    assert_eq!(inventory.main[1], Some(stack(ItemId::Coal, 1)));
    assert_eq!(inventory.main[2], Some(stack(ItemId::Diamond, 64)));
    assert_eq!(inventory.main[3], Some(stack(ItemId::Diamond, 1)));
    assert_eq!(inventory.carried, Some(stack(ItemId::Diamond, 2)));
}

#[test]
fn drag_over_one_slot_is_left_for_a_normal_click() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(ItemId::Diamond, 8));
    assert!(!drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &[SlotId::Main(0), SlotId::CraftResult],
        DragPlace::Split
    ));
    assert_eq!(inventory.carried, Some(stack(ItemId::Diamond, 8)));
    assert!(inventory.main[0].is_none());
}

#[test]
fn hotbar_number_key_swaps_the_hovered_stack_into_that_slot() {
    use game::inventory::SlotId;
    use game::inventory::hotbar_key_swap;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.main[4] = Some(stack(ItemId::Diamond, 3));
    hotbar.slots[1] = Some(stack(ItemId::Coal, 9));
    assert!(hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Main(4),
        1
    ));
    assert_eq!(inventory.main[4], Some(stack(ItemId::Coal, 9)));
    assert_eq!(hotbar.slots[1], Some(stack(ItemId::Diamond, 3)));
    hotbar.slots[2] = Some(stack(ItemId::Stick, 4));
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(2),
        2
    ));
    assert_eq!(hotbar.slots[2], Some(stack(ItemId::Stick, 4)));
}

#[test]
fn hotbar_number_key_crafts_into_an_empty_slot_only() {
    use game::inventory::SlotId;
    use game::inventory::hotbar_key_swap;
    use game::world::block::block::BlockId;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(BlockId::Wood), 1));
    hotbar.slots[0] = Some(stack(ItemId::StoneSword, 1));
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult,
        0
    ));
    assert_eq!(inventory.crafting[0], Some(stack(block(BlockId::Wood), 1)));
    assert!(hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult,
        4
    ));
    assert!(inventory.crafting[0].is_none());
    assert_eq!(
        hotbar.slots[4],
        Some(stack(block(BlockId::WoodenPlanks), 4))
    );
}

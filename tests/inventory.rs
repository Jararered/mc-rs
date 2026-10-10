use game::block::blocks::Block;
use game::inventory::HOTBAR_SLOTS;
use game::inventory::Hotbar;
use game::inventory::collect_matching_stacks;
use game::inventory::sort_container_slots;
use game::inventory::sort_main_inventory;
use game::inventory::take_from_stack;
use game::inventory::take_matching_stacks;
use game::item::Item;
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
fn double_click_collection_fills_cursor_and_leaves_overflow_in_slots() {
    let mut carried = Some(ItemStack::new(Item::Coal, 60).unwrap());
    let mut slots = [
        Some(ItemStack::new(Item::Coal, 8).unwrap()),
        Some(ItemStack::new(Item::Coal, 5).unwrap()),
        Some(ItemStack::new(Item::Diamond, 2).unwrap()),
    ];

    let moved = collect_matching_stacks(&mut carried, &mut slots);

    assert_eq!(moved, 4);
    assert_eq!(carried.unwrap().count(), 64);
    assert_eq!(slots[0], Some(ItemStack::new(Item::Coal, 4).unwrap()));
    assert_eq!(slots[1], Some(ItemStack::new(Item::Coal, 5).unwrap()));
    assert_eq!(slots[2], Some(ItemStack::new(Item::Diamond, 2).unwrap()));
}

#[test]
fn double_click_collection_requires_stack_compatible_item_data() {
    let mut carried = Some(ItemStack::with_data(Item::Coal, 1, 0).unwrap());
    let mut slots = [
        Some(ItemStack::with_data(Item::Coal, 3, 1).unwrap()),
        Some(ItemStack::with_data(Item::Coal, 4, 0).unwrap()),
    ];

    assert_eq!(collect_matching_stacks(&mut carried, &mut slots), 4);
    assert_eq!(carried.unwrap().count(), 5);
    assert_eq!(
        slots[0],
        Some(ItemStack::with_data(Item::Coal, 3, 1).unwrap())
    );
    assert_eq!(slots[1], None);
}

#[test]
fn sorting_uses_block_families_then_materials_and_equipment() {
    let mut slots = [
        Some(ItemStack::new(Item::IronBoots, 1).unwrap()),
        Some(ItemStack::from_block(Block::Torch, 4).unwrap()),
        Some(ItemStack::new(Item::WoodenPickaxe, 1).unwrap()),
        Some(ItemStack::from_block(Block::Wool, 2).unwrap()),
        Some(ItemStack::new(Item::Diamond, 3).unwrap()),
        Some(ItemStack::from_block(Block::WoodenPlanks, 6).unwrap()),
        Some(ItemStack::from_block(Block::Dirt, 5).unwrap()),
        Some(ItemStack::from_block(Block::Stone, 7).unwrap()),
        Some(ItemStack::new(Item::SugarCane, 2).unwrap()),
        Some(ItemStack::new(Item::Bow, 1).unwrap()),
    ];

    sort_container_slots(&mut slots);

    let items: Vec<_> = slots
        .iter()
        .filter_map(|stack| stack.map(ItemStack::item))
        .collect();
    assert_eq!(
        items,
        vec![
            Item::from_block(Block::Stone).unwrap(),
            Item::from_block(Block::Dirt).unwrap(),
            Item::SugarCane,
            Item::from_block(Block::WoodenPlanks).unwrap(),
            Item::from_block(Block::Wool).unwrap(),
            Item::from_block(Block::Torch).unwrap(),
            Item::Diamond,
            Item::Bow,
            Item::WoodenPickaxe,
            Item::IronBoots,
        ]
    );
}

#[test]
fn sorting_orders_variants_preserves_stacks_and_moves_empty_slots_last() {
    let mut slots = [
        Some(ItemStack::with_data(Item::Coal, 9, 1).unwrap()),
        None,
        Some(ItemStack::new(Item::Coal, 4).unwrap()),
    ];

    sort_container_slots(&mut slots);

    assert_eq!(slots[0], Some(ItemStack::new(Item::Coal, 4).unwrap()));
    assert_eq!(
        slots[1],
        Some(ItemStack::with_data(Item::Coal, 9, 1).unwrap())
    );
    assert_eq!(slots[2], None);
}

#[test]
fn sorting_merges_matching_stacks_and_keeps_overflow_in_a_new_stack() {
    let mut slots = [
        Some(ItemStack::new(Item::Coal, 40).unwrap()),
        Some(ItemStack::new(Item::Diamond, 3).unwrap()),
        Some(ItemStack::new(Item::Coal, 30).unwrap()),
        None,
    ];

    sort_container_slots(&mut slots);

    assert_eq!(slots[0], Some(ItemStack::new(Item::Diamond, 3).unwrap()));
    assert_eq!(slots[1], Some(ItemStack::new(Item::Coal, 64).unwrap()));
    assert_eq!(slots[2], Some(ItemStack::new(Item::Coal, 6).unwrap()));
    assert_eq!(slots[3], None);
}

#[test]
fn sorting_follows_the_top_level_tree_and_places_equipment_last() {
    let mut slots = [
        Some(ItemStack::new(Item::IronBoots, 1).unwrap()),
        Some(ItemStack::new(Item::Compass, 1).unwrap()),
        Some(ItemStack::new(Item::Boat, 1).unwrap()),
        Some(ItemStack::new(Item::GoldIngot, 2).unwrap()),
        Some(ItemStack::new(Item::Apple, 1).unwrap()),
        Some(ItemStack::new(Item::Arrow, 8).unwrap()),
        Some(ItemStack::new(Item::Bow, 1).unwrap()),
        Some(ItemStack::new(Item::WoodenSword, 1).unwrap()),
        Some(ItemStack::from_block(Block::Stone, 1).unwrap()),
        Some(ItemStack::new(Item::WoodenPickaxe, 1).unwrap()),
    ];

    sort_container_slots(&mut slots);

    let items: Vec<_> = slots
        .iter()
        .filter_map(|stack| stack.map(ItemStack::item))
        .collect();
    assert_eq!(
        items,
        vec![
            Item::from_block(Block::Stone).unwrap(),
            Item::Apple,
            Item::GoldIngot,
            Item::Boat,
            Item::Compass,
            Item::WoodenSword,
            Item::Bow,
            Item::Arrow,
            Item::WoodenPickaxe,
            Item::IronBoots,
        ]
    );
}

#[test]
fn block_sorting_follows_natural_building_functional_and_redstone_groups() {
    let mut slots = [
        Some(ItemStack::from_block(Block::RedstoneWire, 1).unwrap()),
        Some(ItemStack::from_block(Block::Chest, 1).unwrap()),
        Some(ItemStack::from_block(Block::Wool, 1).unwrap()),
        Some(ItemStack::from_block(Block::Dirt, 1).unwrap()),
        Some(ItemStack::from_block(Block::Stone, 1).unwrap()),
        Some(ItemStack::from_block(Block::CraftingTable, 1).unwrap()),
        Some(ItemStack::from_block(Block::WoodenPlanks, 1).unwrap()),
        Some(ItemStack::from_block(Block::Repeater, 1).unwrap()),
        Some(ItemStack::from_block(Block::Torch, 1).unwrap()),
    ];

    sort_container_slots(&mut slots);

    let items: Vec<_> = slots
        .iter()
        .filter_map(|stack| stack.map(ItemStack::item))
        .collect();
    assert_eq!(
        items,
        vec![
            Item::from_block(Block::Stone).unwrap(),
            Item::from_block(Block::Dirt).unwrap(),
            Item::from_block(Block::WoodenPlanks).unwrap(),
            Item::from_block(Block::Wool).unwrap(),
            Item::from_block(Block::Chest).unwrap(),
            Item::from_block(Block::CraftingTable).unwrap(),
            Item::from_block(Block::RedstoneWire).unwrap(),
            Item::from_block(Block::Torch).unwrap(),
            Item::from_block(Block::Repeater).unwrap(),
        ]
    );
}

#[test]
fn sorting_main_inventory_leaves_hotbar_and_cursor_unchanged() {
    use game::inventory::Inventory;

    let mut inventory = Inventory::default();
    inventory.main[0] = Some(ItemStack::new(Item::WoodenPickaxe, 1).unwrap());
    inventory.main[1] = Some(ItemStack::from_block(Block::Stone, 3).unwrap());
    inventory.carried = Some(ItemStack::new(Item::Diamond, 2).unwrap());
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(ItemStack::new(Item::IronAxe, 1).unwrap());
    let hotbar_before = hotbar.slots;
    let carried_before = inventory.carried;

    sort_main_inventory(&mut inventory);

    assert_eq!(
        inventory.main[0],
        Some(ItemStack::from_block(Block::Stone, 3).unwrap())
    );
    assert_eq!(
        inventory.main[1],
        Some(ItemStack::new(Item::WoodenPickaxe, 1).unwrap())
    );
    assert_eq!(hotbar.slots, hotbar_before);
    assert_eq!(inventory.carried, carried_before);
}

#[test]
fn selected_stack_reads_the_registered_item() {
    let mut hotbar = Hotbar::default();
    hotbar.slots[2] = Some(ItemStack::new(Item::from_u16(1).unwrap(), 4).unwrap());
    hotbar.select(2);
    assert_eq!(
        hotbar.selected_stack(),
        Some(ItemStack::new(Item::from_u16(1).unwrap(), 4).unwrap())
    );
    hotbar.select(0);
    assert_eq!(hotbar.selected_stack(), None);
}

#[test]
fn inventory_merges_into_hotbar_then_main() {
    use game::inventory::Inventory;
    let mut hotbar = Hotbar::default();
    for slot in &mut hotbar.slots {
        *slot = Some(ItemStack::new(Item::Coal, 64).unwrap());
    }
    let mut inventory = Inventory::default();
    assert_eq!(
        inventory.insert(&mut hotbar, ItemStack::new(Item::Diamond, 3).unwrap()),
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
    hotbar.slots[2] = Some(ItemStack::with_data(Item::Coal, 7, 1).unwrap());
    hotbar.select(2);
    let mut inventory = Inventory::default();
    inventory.main[5] = Some(ItemStack::new(Item::Diamond, 3).unwrap());
    inventory.carried = Some(ItemStack::new(Item::IronPickaxe, 1).unwrap());
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

fn stack(item: Item, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

fn block(block: game::block::blocks::Block) -> Item {
    Item::from_block(block).unwrap()
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
    hotbar.slots[0] = Some(stack(Item::Diamond, 5));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(0)
    ));
    assert!(hotbar.slots[0].is_none());
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 5)));

    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Main(0)
    ));
    assert!(inventory.main[0].is_none());
    assert_eq!(hotbar.slots[0], Some(stack(Item::Diamond, 5)));
}

#[test]
fn shift_click_merges_before_using_an_empty_slot() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.main[3] = Some(stack(Item::Diamond, 60));
    hotbar.slots[4] = Some(stack(Item::Diamond, 10));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(4)
    ));
    assert!(hotbar.slots[4].is_none());
    assert_eq!(inventory.main[3], Some(stack(Item::Diamond, 64)));
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 6)));
}

#[test]
fn shift_click_from_hotbar_does_not_spill_into_other_hotbar_slots() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    fill_storage(&mut inventory, &mut hotbar, stack(Item::Coal, 64));
    hotbar.slots[0] = Some(stack(Item::Diamond, 4));
    hotbar.slots[1] = None;
    assert!(!shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(0)
    ));
    assert_eq!(hotbar.slots[0], Some(stack(Item::Diamond, 4)));
    assert!(hotbar.slots[1].is_none());
}

#[test]
fn quick_move_drag_moves_slots_in_both_directions_once_per_gesture() {
    use game::inventory::SlotId;
    use game::inventory::quick_move_drag_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut chest = [None; 27];
    let mut visited = Vec::new();

    inventory.main[0] = Some(stack(Item::Diamond, 5));
    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut chest),
        None,
        SlotId::Main(0),
    ));
    assert!(inventory.main[0].is_none());
    assert_eq!(chest[0], Some(stack(Item::Diamond, 5)));

    inventory.main[1] = Some(stack(Item::Coal, 3));
    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut chest),
        None,
        SlotId::Main(1),
    ));
    assert!(inventory.main[1].is_none());
    assert_eq!(chest[1], Some(stack(Item::Coal, 3)));

    assert!(!quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut chest),
        None,
        SlotId::Main(0),
    ));
    assert_eq!(chest[0], Some(stack(Item::Diamond, 5)));

    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut chest),
        None,
        SlotId::Chest(0),
    ));
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 5)));
    assert!(chest[0].is_none());
}

#[test]
fn quick_move_drag_moves_only_what_fits_and_leaves_full_destinations_untouched() {
    use game::inventory::SlotId;
    use game::inventory::quick_move_drag_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut chest = [Some(stack(Item::Coal, 60))];
    let mut visited = Vec::new();
    inventory.main[0] = Some(stack(Item::Coal, 10));

    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut chest),
        None,
        SlotId::Main(0),
    ));
    assert_eq!(inventory.main[0], Some(stack(Item::Coal, 6)));
    assert_eq!(chest[0], Some(stack(Item::Coal, 64)));

    let mut full_chest = [Some(stack(Item::Diamond, 64))];
    let mut full_visited = Vec::new();
    inventory.main[1] = Some(stack(Item::Coal, 4));
    assert!(!quick_move_drag_slot(
        &mut full_visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut full_chest),
        None,
        SlotId::Main(1),
    ));
    assert_eq!(inventory.main[1], Some(stack(Item::Coal, 4)));
    assert_eq!(full_chest[0], Some(stack(Item::Diamond, 64)));

    fill_storage(&mut inventory, &mut hotbar, stack(Item::Diamond, 64));
    let mut blocked_chest = [Some(stack(Item::Coal, 4))];
    let mut blocked_visited = Vec::new();
    assert!(!quick_move_drag_slot(
        &mut blocked_visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        Some(&mut blocked_chest),
        None,
        SlotId::Chest(0),
    ));
    assert_eq!(blocked_chest[0], Some(stack(Item::Coal, 4)));
}

#[test]
fn quick_move_drag_respects_furnace_slot_eligibility_and_empty_slots() {
    use game::block::blocks::Block;
    use game::inventory::SlotId;
    use game::inventory::quick_move_drag_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut furnace = [None; 3];
    let mut visited = Vec::new();

    assert!(!quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        None,
        Some(&mut furnace),
        SlotId::Furnace(0),
    ));
    inventory.main[0] = Some(stack(Item::Diamond, 2));
    assert!(!quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        None,
        Some(&mut furnace),
        SlotId::Main(0),
    ));
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 2)));
    assert!(furnace.iter().all(Option::is_none));

    let mut visited = Vec::new();
    inventory.main[1] = Some(stack(block(Block::IronOre), 3));
    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        None,
        Some(&mut furnace),
        SlotId::Main(1),
    ));
    assert!(inventory.main[1].is_none());
    assert_eq!(furnace[0], Some(stack(block(Block::IronOre), 3)));

    let mut visited = Vec::new();
    assert!(quick_move_drag_slot(
        &mut visited,
        &mut inventory,
        &mut hotbar,
        None,
        false,
        None,
        Some(&mut furnace),
        SlotId::Furnace(0),
    ));
    assert!(furnace[0].is_none());
    assert_eq!(hotbar.slots[0], Some(stack(block(Block::IronOre), 3)));
}

#[test]
fn shift_click_from_chest_fills_the_hotbar_before_main_storage() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_chest_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut chest = [None; 27];
    chest[0] = Some(stack(Item::Diamond, 5));

    assert!(shift_click_chest_slot(
        &mut inventory,
        &mut hotbar,
        &mut chest,
        SlotId::Chest(0)
    ));
    assert!(chest[0].is_none());
    assert_eq!(hotbar.slots[0], Some(stack(Item::Diamond, 5)));
    assert!(inventory.main[0].is_none());

    // Matching stacks in main storage are topped up only after the hotbar.
    hotbar.slots[0] = Some(stack(Item::Coal, 60));
    inventory.main[0] = Some(stack(Item::Coal, 60));
    chest[1] = Some(stack(Item::Coal, 10));
    assert!(shift_click_chest_slot(
        &mut inventory,
        &mut hotbar,
        &mut chest,
        SlotId::Chest(1)
    ));
    assert!(chest[1].is_none());
    assert_eq!(hotbar.slots[0], Some(stack(Item::Coal, 64)));
    assert_eq!(inventory.main[0], Some(stack(Item::Coal, 64)));
    assert_eq!(hotbar.slots[1], Some(stack(Item::Coal, 2)));
}

#[test]
fn shift_click_returns_crafting_inputs_to_main_storage_first() {
    use game::crafting::CraftingGrid;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[2] = Some(stack(Item::Diamond, 3));
    inventory.armor[0] = Some(stack(Item::DiamondSword, 1));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Craft(2)
    ));
    assert!(inventory.crafting[2].is_none());
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 3)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Armor(0)
    ));
    assert!(inventory.armor[0].is_none());
    assert_eq!(inventory.main[1], Some(stack(Item::DiamondSword, 1)));

    let mut grid = CraftingGrid::workbench();
    grid.set(2, 1, Some(stack(Item::Stick, 6)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        Some(&mut grid),
        true,
        SlotId::Workbench(5)
    ));
    assert!(grid.get(2, 1).is_none());
    assert_eq!(inventory.main[2], Some(stack(Item::Stick, 6)));
}

#[test]
fn shift_click_crafting_result_repeats_and_fills_the_hotbar_from_the_right() {
    use game::block::blocks::Block;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(Block::Wood), 2));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult
    ));
    assert!(inventory.crafting.iter().all(Option::is_none));
    assert_eq!(hotbar.slots[8], Some(stack(block(Block::WoodenPlanks), 8)));
    assert!(hotbar.slots[..8].iter().all(Option::is_none));
}

#[test]
fn shift_click_crafting_stops_when_the_next_output_does_not_fit() {
    use game::block::blocks::Block;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    fill_storage(&mut inventory, &mut hotbar, stack(Item::Diamond, 64));
    hotbar.slots[8] = Some(stack(block(Block::WoodenPlanks), 60));
    inventory.crafting[0] = Some(stack(block(Block::Wood), 2));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult
    ));
    assert_eq!(inventory.crafting[0], Some(stack(block(Block::Wood), 1)));
    assert_eq!(hotbar.slots[8], Some(stack(block(Block::WoodenPlanks), 64)));
    assert_eq!(hotbar.slots[7], Some(stack(Item::Diamond, 64)));
}

#[test]
fn shift_click_workbench_result_uses_the_three_by_three_grid() {
    use game::block::blocks::Block;
    use game::crafting::CraftingGrid;
    use game::inventory::SlotId;
    use game::inventory::shift_click_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let mut grid = CraftingGrid::workbench();
    grid.set(0, 0, Some(stack(block(Block::Wood), 1)));
    assert!(shift_click_slot(
        &mut inventory,
        &mut hotbar,
        Some(&mut grid),
        true,
        SlotId::CraftResult
    ));
    assert!(grid.get(0, 0).is_none());
    assert_eq!(hotbar.slots[8], Some(stack(block(Block::WoodenPlanks), 4)));
}

#[test]
fn left_drag_splits_a_stack_and_keeps_the_remainder() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(Item::Diamond, 64));
    inventory.main[1] = Some(stack(Item::Diamond, 10));
    let slots = [SlotId::Main(0), SlotId::Main(1), SlotId::Main(2)];
    assert!(drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &slots,
        DragPlace::Split
    ));
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 21)));
    assert_eq!(inventory.main[1], Some(stack(Item::Diamond, 31)));
    assert_eq!(inventory.main[2], Some(stack(Item::Diamond, 21)));
    assert_eq!(inventory.carried, Some(stack(Item::Diamond, 1)));
}

#[test]
fn right_drag_places_one_item_in_each_slot() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(Item::Diamond, 5));
    inventory.main[2] = Some(stack(Item::Diamond, 63));
    inventory.main[1] = Some(stack(Item::Coal, 1));
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
    assert_eq!(inventory.main[0], Some(stack(Item::Diamond, 1)));
    assert_eq!(inventory.main[1], Some(stack(Item::Coal, 1)));
    assert_eq!(inventory.main[2], Some(stack(Item::Diamond, 64)));
    assert_eq!(inventory.main[3], Some(stack(Item::Diamond, 1)));
    assert_eq!(inventory.carried, Some(stack(Item::Diamond, 2)));
}

#[test]
fn left_drag_preview_shows_split_results_and_merges_without_mutating() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::preview_drag_place;

    let mut inventory = game::inventory::Inventory::default();
    let hotbar = Hotbar::default();
    inventory.carried = Some(stack(Item::Diamond, 64));
    inventory.main[1] = Some(stack(Item::Diamond, 10));
    let before = inventory.clone();
    let slots = [SlotId::Main(0), SlotId::Main(1), SlotId::Main(2)];

    let preview = preview_drag_place(&inventory, &hotbar, None, &slots, DragPlace::Split);

    assert_eq!(preview.len(), 3);
    assert_eq!(preview[0], (SlotId::Main(0), stack(Item::Diamond, 21)));
    assert_eq!(preview[1], (SlotId::Main(1), stack(Item::Diamond, 31)));
    assert_eq!(preview[2], (SlotId::Main(2), stack(Item::Diamond, 21)));
    assert_eq!(inventory.main, before.main);
    assert_eq!(inventory.carried, before.carried);
}

#[test]
fn right_drag_preview_places_one_each_and_omits_invalid_slots() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::preview_drag_place;

    let mut inventory = game::inventory::Inventory::default();
    let hotbar = Hotbar::default();
    inventory.carried = Some(stack(Item::Diamond, 5));
    inventory.main[1] = Some(stack(Item::Coal, 1));
    inventory.main[2] = Some(stack(Item::Diamond, 64));
    inventory.main[3] = Some(stack(Item::Diamond, 63));
    let before = inventory.clone();
    let slots = [
        SlotId::Main(0),
        SlotId::Main(1),
        SlotId::Main(2),
        SlotId::Main(3),
    ];

    let preview = preview_drag_place(&inventory, &hotbar, None, &slots, DragPlace::OneEach);

    assert_eq!(preview.len(), 2);
    assert_eq!(preview[0], (SlotId::Main(0), stack(Item::Diamond, 1)));
    assert_eq!(preview[1], (SlotId::Main(3), stack(Item::Diamond, 64)));
    assert_eq!(inventory.main, before.main);
    assert_eq!(inventory.carried, before.carried);
}

#[test]
fn drag_over_one_slot_is_left_for_a_normal_click() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::drag_place;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.carried = Some(stack(Item::Diamond, 8));
    assert!(!drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &[SlotId::Main(0), SlotId::CraftResult],
        DragPlace::Split
    ));
    assert_eq!(inventory.carried, Some(stack(Item::Diamond, 8)));
    assert!(inventory.main[0].is_none());
}

#[test]
fn hotbar_number_key_swaps_the_hovered_stack_into_that_slot() {
    use game::inventory::SlotId;
    use game::inventory::hotbar_key_swap;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.main[4] = Some(stack(Item::Diamond, 3));
    hotbar.slots[1] = Some(stack(Item::Coal, 9));
    assert!(hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Main(4),
        1
    ));
    assert_eq!(inventory.main[4], Some(stack(Item::Coal, 9)));
    assert_eq!(hotbar.slots[1], Some(stack(Item::Diamond, 3)));
    hotbar.slots[2] = Some(stack(Item::Stick, 4));
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Hotbar(2),
        2
    ));
    assert_eq!(hotbar.slots[2], Some(stack(Item::Stick, 4)));
}

#[test]
fn hotbar_number_key_crafts_into_an_empty_slot_only() {
    use game::block::blocks::Block;
    use game::inventory::SlotId;
    use game::inventory::hotbar_key_swap;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(Block::Wood), 1));
    hotbar.slots[0] = Some(stack(Item::StoneSword, 1));
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult,
        0
    ));
    assert_eq!(inventory.crafting[0], Some(stack(block(Block::Wood), 1)));
    assert!(hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::CraftResult,
        4
    ));
    assert!(inventory.crafting[0].is_none());
    assert_eq!(hotbar.slots[4], Some(stack(block(Block::WoodenPlanks), 4)));
}

#[test]
fn closing_crafting_session_returns_inputs_or_drops_overflow_without_ui() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;
    use game::entity::DroppedItem;
    use game::inventory::Inventory;
    use game::inventory::session::ActiveWorkbench;
    use game::inventory::session::close_crafting_session;
    use game::random::ItemRng;

    for full in [false, true] {
        let stone = Item::from_u16(u16::from(Block::Stone.as_u8())).unwrap();
        let mut hotbar = Hotbar::default();
        let mut inventory = Inventory::default();
        if full {
            hotbar.slots.fill(Some(ItemStack::new(stone, 64).unwrap()));
            inventory
                .main
                .fill(Some(ItemStack::new(stone, 64).unwrap()));
        }
        inventory.crafting[0] = Some(ItemStack::new(stone, 3).unwrap());
        inventory.carried = Some(ItemStack::new(stone, 2).unwrap());
        let mut workbench = ActiveWorkbench::default();
        workbench.position = Some((4, 60, 7));
        workbench
            .grid
            .set(1, 1, Some(ItemStack::new(stone, 4).unwrap()));
        let mut world = World::new();
        world.insert_resource(workbench);
        let player = world.spawn((Transform::default(), hotbar, inventory)).id();
        world
            .run_system_once(
                |mut commands: Commands,
                 mut workbench: ResMut<ActiveWorkbench>,
                 mut player: Query<(&Transform, &mut Hotbar, &mut Inventory)>,
                 mut rng: Local<ItemRng>| {
                    let (transform, mut hotbar, mut inventory) = player.single_mut().unwrap();
                    close_crafting_session(
                        &mut commands,
                        transform,
                        &mut rng,
                        &mut hotbar,
                        &mut inventory,
                        &mut workbench,
                    );
                },
            )
            .unwrap();
        let inventory = world.get::<Inventory>(player).unwrap();
        assert!(inventory.crafting.iter().all(Option::is_none));
        assert!(inventory.carried.is_none());
        let workbench = world.resource::<ActiveWorkbench>();
        assert!(workbench.position.is_none());
        assert_eq!(workbench.grid.occupied(), 0);
        let drops: u32 = world
            .query::<&DroppedItem>()
            .iter(&world)
            .map(|drop| u32::from(drop.0.count()))
            .sum();
        assert_eq!(drops, if full { 9 } else { 0 });
        if !full {
            assert_eq!(
                world
                    .get::<Hotbar>(player)
                    .unwrap()
                    .slots
                    .iter()
                    .flatten()
                    .map(|stack| u32::from(stack.count()))
                    .sum::<u32>(),
                9
            );
        }
    }
}

#[test]
fn pickup_tops_up_a_main_stack_before_taking_an_empty_hotbar_slot() {
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.main[5] = Some(stack(Item::Coal, 60));

    assert_eq!(inventory.insert(&mut hotbar, stack(Item::Coal, 10)), None);

    assert_eq!(inventory.main[5], Some(stack(Item::Coal, 64)));
    assert_eq!(hotbar.slots[0], Some(stack(Item::Coal, 6)));
    assert_eq!(hotbar.pop[0], 5);
}

#[test]
fn armor_slots_take_only_their_own_piece() {
    use game::inventory::DragPlace;
    use game::inventory::SlotId;
    use game::inventory::armor_slot_accepts;
    use game::inventory::drag_place;
    use game::inventory::hotbar_key_swap;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let dirt = stack(block(Block::Dirt), 32);
    let helmet = stack(Item::IronHelmet, 1);
    assert!(armor_slot_accepts(0, helmet));
    assert!(!armor_slot_accepts(1, helmet));
    assert!(!armor_slot_accepts(0, dirt));

    hotbar.slots[0] = Some(dirt);
    hotbar.slots[1] = Some(helmet);
    let armor = SlotId::Armor(0);
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        armor,
        0
    ));
    assert_eq!(hotbar.slots[0], Some(dirt));
    assert!(!hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        SlotId::Armor(2),
        1
    ));
    assert!(hotbar_key_swap(
        &mut inventory,
        &mut hotbar,
        None,
        false,
        armor,
        1
    ));
    assert_eq!(inventory.armor[0], Some(helmet));
    assert_eq!(hotbar.slots[1], None);

    inventory.carried = Some(dirt);
    let painted = [SlotId::Armor(1), SlotId::Armor(2), SlotId::Armor(3)];
    assert!(!drag_place(
        &mut inventory,
        &mut hotbar,
        None,
        &painted,
        DragPlace::Split
    ));
    assert_eq!(inventory.carried, Some(dirt));
    assert_eq!(inventory.armor[1..], [None; 3]);
}

#[test]
fn hotbar_number_key_leaves_the_stack_alone_over_a_slot_it_cannot_reach() {
    use game::inventory::SlotId;
    use game::inventory::hotbar_key_swap;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    hotbar.slots[3] = Some(stack(Item::Diamond, 7));
    for slot in [SlotId::Chest(0), SlotId::Workbench(0), SlotId::Furnace(0)] {
        assert!(!hotbar_key_swap(
            &mut inventory,
            &mut hotbar,
            None,
            false,
            slot,
            3
        ));
        assert_eq!(hotbar.slots[3], Some(stack(Item::Diamond, 7)));
    }
}

#[test]
fn shift_click_falls_back_to_the_fuel_slot_when_the_input_is_taken() {
    use game::inventory::SlotId;
    use game::inventory::shift_click_furnace_slot;
    let mut inventory = game::inventory::Inventory::default();
    let mut hotbar = Hotbar::default();
    let ore = stack(block(Block::IronOre), 4);
    let logs = stack(block(Block::Wood), 8);
    let mut furnace = [Some(ore), None, None];
    inventory.main[0] = Some(logs);

    assert!(shift_click_furnace_slot(
        &mut inventory,
        &mut hotbar,
        &mut furnace,
        SlotId::Main(0)
    ));

    assert_eq!(furnace, [Some(ore), Some(logs), None]);
    assert_eq!(inventory.main[0], None);
}

#[test]
fn take_from_stack_removes_up_to_the_requested_count() {
    let mut slot = Some(ItemStack::new(Item::Coal, 5).unwrap());
    let one = take_from_stack(&mut slot, 1).unwrap();
    assert_eq!(one.count(), 1);
    assert_eq!(slot.unwrap().count(), 4);

    let rest = take_from_stack(&mut slot, 200).unwrap();
    assert_eq!(rest.count(), 4);
    assert_eq!(rest.item(), one.item());
    assert_eq!(slot, None);

    assert_eq!(take_from_stack(&mut slot, 1), None);
}

#[test]
fn take_from_stack_keeps_item_data_and_ignores_zero() {
    let mut slot = Some(ItemStack::from_block_state(Block::Wool, 5, 3).unwrap());
    let taken = take_from_stack(&mut slot, 1).unwrap();
    assert_eq!(taken.data(), 5);
    assert_eq!(slot.unwrap().data(), 5);
    assert_eq!(take_from_stack(&mut slot, 0), None);
    assert_eq!(slot.unwrap().count(), 2);
}

#[test]
fn take_matching_stacks_removes_only_the_same_item_and_data() {
    let coal = ItemStack::new(Item::Coal, 8).unwrap();
    let mut slots = [
        Some(ItemStack::new(Item::Coal, 64).unwrap()),
        Some(ItemStack::new(Item::Diamond, 2).unwrap()),
        None,
        Some(ItemStack::from_block_state(Block::Wool, 5, 3).unwrap()),
        Some(ItemStack::new(Item::Coal, 5).unwrap()),
    ];
    let wool = slots[3].unwrap();

    let taken = take_matching_stacks(slots.iter_mut(), coal);
    assert_eq!(
        taken.iter().map(|stack| stack.count()).collect::<Vec<_>>(),
        [64, 5]
    );
    assert_eq!(slots[0], None);
    assert_eq!(slots[4], None);
    assert_eq!(slots[1], Some(ItemStack::new(Item::Diamond, 2).unwrap()));
    assert_eq!(slots[3], Some(wool));

    // Same block, different data (wool colour): left alone.
    let other = ItemStack::from_block_state(Block::Wool, 1, 1).unwrap();
    assert!(take_matching_stacks(slots.iter_mut(), other).is_empty());
}

#[test]
fn consuming_an_item_takes_one_from_the_first_slot_that_holds_it() {
    use game::inventory::Inventory;

    let mut hotbar = Hotbar::default();
    let mut inventory = Inventory::default();
    assert!(!inventory.holds(&hotbar, Item::Arrow));
    assert!(!inventory.consume(&mut hotbar, Item::Arrow));

    inventory.main[5] = ItemStack::new(Item::Arrow, 2).ok();
    hotbar.slots[3] = ItemStack::new(Item::Arrow, 1).ok();
    assert!(inventory.holds(&hotbar, Item::Arrow));
    // The hotbar is searched first, and an emptied slot is cleared.
    assert!(inventory.consume(&mut hotbar, Item::Arrow));
    assert_eq!(hotbar.slots[3], None);
    assert!(inventory.consume(&mut hotbar, Item::Arrow));
    assert_eq!(inventory.main[5].map(|stack| stack.count()), Some(1));
    assert!(inventory.consume(&mut hotbar, Item::Arrow));
    assert!(!inventory.consume(&mut hotbar, Item::Arrow));
}

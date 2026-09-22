use game::crafting::CraftingGrid;
use game::crafting::Ingredient;
use game::crafting::IngredientData;
use game::crafting::Recipe;
use game::crafting::WorkbenchSession;
use game::crafting::beta_recipe_book;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::ItemId;
use game::item::ItemStack;
use game::world::block::block::BlockId;

fn stack(item: ItemId, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}
fn block(block: BlockId) -> ItemId {
    ItemId::from_block(block).unwrap()
}

#[test]
fn shaped_recipes_offset_and_mirror_inside_three_by_three() {
    let recipe = Recipe::Shaped {
        width: 2,
        height: 2,
        ingredients: vec![
            Some(Ingredient::any(ItemId::Stick)),
            Some(Ingredient::any(ItemId::Coal)),
            None,
            Some(Ingredient::any(ItemId::Diamond)),
        ],
        output: stack(block(BlockId::Torch), 1),
    };
    let mut grid = CraftingGrid::workbench();
    grid.set(1, 1, Some(stack(ItemId::Coal, 1)));
    grid.set(2, 1, Some(stack(ItemId::Stick, 1)));
    grid.set(1, 2, Some(stack(ItemId::Diamond, 1)));
    assert!(recipe.matches(&grid));
    grid.set(0, 0, Some(stack(ItemId::Bone, 1)));
    assert!(!recipe.matches(&grid));
}

#[test]
fn shapeless_matching_ignores_slot_order_and_data_can_be_exact() {
    let recipe = Recipe::Shapeless {
        ingredients: vec![
            Ingredient::exact(ItemId::Dye, 1),
            Ingredient::any(ItemId::Dye),
        ],
        output: stack(ItemId::Dye, 2),
    };
    let mut grid = CraftingGrid::player();
    grid.set(0, 0, Some(ItemStack::with_data(ItemId::Dye, 1, 4).unwrap()));
    grid.set(1, 1, Some(ItemStack::with_data(ItemId::Dye, 1, 1).unwrap()));
    assert!(recipe.matches(&grid));
    grid.set(1, 1, Some(ItemStack::with_data(ItemId::Dye, 1, 2).unwrap()));
    assert!(!recipe.matches(&grid));
    assert_eq!(Ingredient::any(ItemId::Dye).data, IngredientData::Any);
}

#[test]
fn beta_book_crafts_common_items_in_two_by_two_and_three_by_three() {
    let book = beta_recipe_book();
    let mut player = CraftingGrid::player();
    player.set(0, 0, Some(stack(block(BlockId::Wood), 1)));
    assert_eq!(
        book.find(&player).unwrap().item(),
        block(BlockId::WoodenPlanks)
    );
    assert_eq!(book.find(&player).unwrap().count(), 4);

    let mut workbench = CraftingGrid::workbench();
    for y in 0..3 {
        for x in 0..3 {
            workbench.set(x, y, Some(stack(ItemId::IronIngot, 1)));
        }
    }
    assert_eq!(
        book.find(&workbench).unwrap().item(),
        block(BlockId::IronBlock)
    );
    assert!(book.find(&player).is_some());
}

#[test]
fn paper_recipe_uses_three_sugar_cane_in_any_arrangement() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::workbench();
    grid.set(0, 0, Some(stack(ItemId::SugarCane, 1)));
    grid.set(1, 1, Some(stack(ItemId::SugarCane, 1)));
    grid.set(2, 2, Some(stack(ItemId::SugarCane, 1)));

    assert_eq!(book.find(&grid), Some(stack(ItemId::Paper, 3)));

    grid.set(2, 2, None);
    assert_eq!(book.find(&grid), None);
}

#[test]
fn logs_craft_into_matching_species_of_planks() {
    let book = beta_recipe_book();
    for (log, species, planks) in [
        (BlockId::Wood, 0, BlockId::WoodenPlanks),
        (BlockId::SpruceWood, 1, BlockId::SprucePlanks),
        (BlockId::BirchWood, 2, BlockId::BirchPlanks),
    ] {
        let mut grid = CraftingGrid::player();
        grid.set(0, 0, Some(ItemStack::from_block(log, 1).unwrap()));
        let output = book.find(&grid).expect("a log should craft into planks");
        assert_eq!(output.item(), block(BlockId::WoodenPlanks));
        assert_eq!(output.count(), 4);
        assert_eq!(output.data(), species);
        assert_eq!(output.runtime_block(), Some(planks));
    }
}

#[test]
fn cake_consumption_returns_empty_buckets_and_repeats_safely() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::workbench();
    for x in 0..3 {
        grid.set(x, 0, Some(stack(ItemId::MilkBucket, 1)));
    }
    grid.set(0, 1, Some(stack(ItemId::Sugar, 1)));
    grid.set(1, 1, Some(stack(ItemId::Egg, 1)));
    grid.set(2, 1, Some(stack(ItemId::Sugar, 1)));
    for x in 0..3 {
        grid.set(x, 2, Some(stack(ItemId::Wheat, 1)));
    }
    assert_eq!(book.find(&grid).unwrap().item(), ItemId::Cake);
    let remainders = book.consume_one(&mut grid).unwrap();
    assert_eq!(remainders.len(), 3);
    assert!(grid.slots().all(|slot| slot.is_none()));
}

#[test]
fn player_result_pickup_is_atomic_when_carried_conflicts() {
    let mut inventory = Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(BlockId::Wood), 1));
    inventory.carried = Some(stack(ItemId::StoneSword, 1));
    assert!(!inventory.take_crafting_result(&mut hotbar));
    assert_eq!(inventory.crafting[0].unwrap().item(), block(BlockId::Wood));
}

#[test]
fn workbench_session_matches_beta_reach_limit() {
    let session = WorkbenchSession::new((4, 8, -2));
    assert!(session.clone().within_reach((4.5, 8.5, -2.5)));
    assert!(!session.within_reach((13.0, 8.5, -2.5)));
}

#[test]
fn draining_a_crafting_grid_removes_all_inputs() {
    let mut grid = CraftingGrid::workbench();
    grid.set(0, 0, Some(stack(ItemId::WoodenPickaxe, 1)));
    grid.set(2, 2, Some(stack(ItemId::Stick, 3)));

    let drained = grid.drain();

    assert_eq!(drained.len(), 2);
    assert_eq!(grid.occupied(), 0);
    assert!(grid.slots().all(|slot| slot.is_none()));
}

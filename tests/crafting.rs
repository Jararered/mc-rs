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
use game::world::block::registry::BetaBlockId;

fn stack(item: ItemId, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}
fn block(block: BetaBlockId) -> ItemId {
    ItemId::from_block(block)
}

#[test]
fn shaped_recipes_offset_and_mirror_inside_three_by_three() {
    let recipe = Recipe::Shaped {
        width: 2,
        height: 2,
        ingredients: vec![
            Some(Ingredient::any(ItemId::STICK)),
            Some(Ingredient::any(ItemId::COAL)),
            None,
            Some(Ingredient::any(ItemId::DIAMOND)),
        ],
        output: stack(block(BetaBlockId::TORCH), 1),
    };
    let mut grid = CraftingGrid::workbench();
    grid.set(1, 1, Some(stack(ItemId::COAL, 1)));
    grid.set(2, 1, Some(stack(ItemId::STICK, 1)));
    grid.set(1, 2, Some(stack(ItemId::DIAMOND, 1)));
    assert!(recipe.matches(&grid));
    grid.set(0, 0, Some(stack(ItemId::BONE, 1)));
    assert!(!recipe.matches(&grid));
}

#[test]
fn shapeless_matching_ignores_slot_order_and_data_can_be_exact() {
    let recipe = Recipe::Shapeless {
        ingredients: vec![
            Ingredient::exact(ItemId::DYE, 1),
            Ingredient::any(ItemId::DYE),
        ],
        output: stack(ItemId::DYE, 2),
    };
    let mut grid = CraftingGrid::player();
    grid.set(0, 0, Some(ItemStack::with_data(ItemId::DYE, 1, 4).unwrap()));
    grid.set(1, 1, Some(ItemStack::with_data(ItemId::DYE, 1, 1).unwrap()));
    assert!(recipe.matches(&grid));
    grid.set(1, 1, Some(ItemStack::with_data(ItemId::DYE, 1, 2).unwrap()));
    assert!(!recipe.matches(&grid));
    assert_eq!(Ingredient::any(ItemId::DYE).data, IngredientData::Any);
}

#[test]
fn beta_book_crafts_common_items_in_two_by_two_and_three_by_three() {
    let book = beta_recipe_book();
    let mut player = CraftingGrid::player();
    player.set(0, 0, Some(stack(block(BetaBlockId::WOOD), 1)));
    assert_eq!(
        book.find(&player).unwrap().item(),
        block(BetaBlockId::WOODEN_PLANKS)
    );
    assert_eq!(book.find(&player).unwrap().count(), 4);

    let mut workbench = CraftingGrid::workbench();
    for y in 0..3 {
        for x in 0..3 {
            workbench.set(x, y, Some(stack(ItemId::IRON_INGOT, 1)));
        }
    }
    assert_eq!(
        book.find(&workbench).unwrap().item(),
        block(BetaBlockId::IRON_BLOCK)
    );
    assert!(book.find(&player).is_some());
}

#[test]
fn cake_consumption_returns_empty_buckets_and_repeats_safely() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::workbench();
    for x in 0..3 {
        grid.set(x, 0, Some(stack(ItemId::MILK_BUCKET, 1)));
    }
    grid.set(0, 1, Some(stack(ItemId::SUGAR, 1)));
    grid.set(1, 1, Some(stack(ItemId::EGG, 1)));
    grid.set(2, 1, Some(stack(ItemId::SUGAR, 1)));
    for x in 0..3 {
        grid.set(x, 2, Some(stack(ItemId::WHEAT, 1)));
    }
    assert_eq!(book.find(&grid).unwrap().item(), ItemId::CAKE);
    let remainders = book.consume_one(&mut grid).unwrap();
    assert_eq!(remainders.len(), 3);
    assert!(grid.slots().all(|slot| slot.is_none()));
}

#[test]
fn player_result_pickup_is_atomic_when_carried_conflicts() {
    let mut inventory = Inventory::default();
    let mut hotbar = Hotbar::default();
    inventory.crafting[0] = Some(stack(block(BetaBlockId::WOOD), 1));
    inventory.carried = Some(stack(ItemId::STONE_SWORD, 1));
    assert!(!inventory.take_crafting_result(&mut hotbar));
    assert_eq!(
        inventory.crafting[0].unwrap().item(),
        block(BetaBlockId::WOOD)
    );
}

#[test]
fn workbench_session_matches_beta_reach_limit() {
    let session = WorkbenchSession::new((4, 8, -2));
    assert!(session.clone().within_reach((4.5, 8.5, -2.5)));
    assert!(!session.within_reach((13.0, 8.5, -2.5)));
}

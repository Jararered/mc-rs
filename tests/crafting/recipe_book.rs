use game::block::blocks::Block;
use game::crafting::CraftingGrid;
use game::crafting::beta_recipe_book;
use game::item::ItemId;
use game::item::ItemStack;

use super::block;
use super::stack;

#[test]
fn beta_book_crafts_common_items_in_two_by_two_and_three_by_three() {
    let book = beta_recipe_book();
    let mut player = CraftingGrid::player();
    player.set(0, 0, Some(stack(block(Block::Wood), 1)));
    assert_eq!(
        book.find(&player).unwrap().item(),
        block(Block::WoodenPlanks)
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
        block(Block::IronBlock)
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
        (Block::Wood, 0, Block::WoodenPlanks),
        (Block::SpruceWood, 1, Block::SprucePlanks),
        (Block::BirchWood, 2, Block::BirchPlanks),
    ] {
        let mut grid = CraftingGrid::player();
        grid.set(0, 0, Some(ItemStack::from_block(log, 1).unwrap()));
        let output = book.find(&grid).expect("a log should craft into planks");
        assert_eq!(output.item(), block(Block::WoodenPlanks));
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

use game::block::blocks::Block;
use game::crafting::CraftingGrid;
use game::crafting::Ingredient;
use game::crafting::IngredientData;
use game::crafting::Recipe;
use game::item::Item;
use game::item::ItemStack;

use super::block;
use super::stack;

#[test]
fn shaped_recipes_offset_and_mirror_inside_three_by_three() {
    let recipe = Recipe::Shaped {
        width: 2,
        height: 2,
        ingredients: vec![
            Some(Ingredient::any(Item::Stick)),
            Some(Ingredient::any(Item::Coal)),
            None,
            Some(Ingredient::any(Item::Diamond)),
        ],
        output: stack(block(Block::Torch), 1),
    };
    let mut grid = CraftingGrid::workbench();
    grid.set(1, 1, Some(stack(Item::Coal, 1)));
    grid.set(2, 1, Some(stack(Item::Stick, 1)));
    grid.set(1, 2, Some(stack(Item::Diamond, 1)));
    assert!(recipe.matches(&grid));
    grid.set(0, 0, Some(stack(Item::Bone, 1)));
    assert!(!recipe.matches(&grid));
}

#[test]
fn shapeless_matching_ignores_slot_order_and_data_can_be_exact() {
    let recipe = Recipe::Shapeless {
        ingredients: vec![
            Ingredient::exact(Item::Dye, 1),
            Ingredient::any(Item::Dye),
        ],
        output: stack(Item::Dye, 2),
    };
    let mut grid = CraftingGrid::player();
    grid.set(0, 0, Some(ItemStack::with_data(Item::Dye, 1, 4).unwrap()));
    grid.set(1, 1, Some(ItemStack::with_data(Item::Dye, 1, 1).unwrap()));
    assert!(recipe.matches(&grid));
    grid.set(1, 1, Some(ItemStack::with_data(Item::Dye, 1, 2).unwrap()));
    assert!(!recipe.matches(&grid));
    assert_eq!(Ingredient::any(Item::Dye).data, IngredientData::Any);
}

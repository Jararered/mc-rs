use game::block::block::BlockId;
use game::crafting::CraftingGrid;
use game::crafting::Ingredient;
use game::crafting::IngredientData;
use game::crafting::Recipe;
use game::item::ItemId;
use game::item::ItemStack;

use super::block;
use super::stack;

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

use game::block::blocks::Block;
use game::crafting::CraftingGrid;
use game::crafting::beta_recipe_book;
use game::item::Item;
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
            workbench.set(x, y, Some(stack(Item::IronIngot, 1)));
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
    grid.set(0, 0, Some(stack(Item::SugarCane, 1)));
    grid.set(1, 1, Some(stack(Item::SugarCane, 1)));
    grid.set(2, 2, Some(stack(Item::SugarCane, 1)));

    assert_eq!(book.find(&grid), Some(stack(Item::Paper, 3)));

    grid.set(2, 2, None);
    assert_eq!(book.find(&grid), None);
}

#[test]
fn logs_craft_into_matching_species_of_planks() {
    let book = beta_recipe_book();
    for (species, planks) in [
        (0, Block::WoodenPlanks),
        (1, Block::WoodenPlanks),
        (2, Block::WoodenPlanks),
    ] {
        let mut grid = CraftingGrid::player();
        grid.set(
            0,
            0,
            Some(ItemStack::from_block_state(Block::Wood, species, 1).unwrap()),
        );
        let output = book.find(&grid).expect("a log should craft into planks");
        assert_eq!(output.item(), block(Block::WoodenPlanks));
        assert_eq!(output.count(), 4);
        assert_eq!(output.data(), u16::from(species));
        assert_eq!(output.runtime_block(), Some((planks, species)));
    }
}

#[test]
fn cake_consumption_leaves_empty_buckets_in_the_grid() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::workbench();
    for x in 0..3 {
        grid.set(x, 0, Some(stack(Item::MilkBucket, 1)));
    }
    grid.set(0, 1, Some(stack(Item::Sugar, 1)));
    grid.set(1, 1, Some(stack(Item::Egg, 1)));
    grid.set(2, 1, Some(stack(Item::Sugar, 1)));
    for x in 0..3 {
        grid.set(x, 2, Some(stack(Item::Wheat, 1)));
    }
    assert_eq!(book.find(&grid).unwrap().item(), Item::Cake);
    let remainders = book.consume_one(&mut grid).unwrap();
    assert!(remainders.is_empty());
    for x in 0..3 {
        assert_eq!(grid.get(x, 0), Some(stack(Item::Bucket, 1)));
    }
    assert_eq!(grid.slots().flatten().count(), 3);
    assert!(book.find(&grid).is_none());
}

#[test]
fn flint_and_steel_needs_iron_and_flint_on_a_diagonal() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::player();
    grid.set(0, 0, Some(stack(Item::IronIngot, 1)));
    grid.set(1, 1, Some(stack(Item::Flint, 1)));
    assert_eq!(book.find(&grid), Some(stack(Item::FlintAndSteel, 1)));

    grid.set(1, 1, None);
    grid.set(1, 0, Some(stack(Item::Flint, 1)));
    assert_eq!(book.find(&grid), None);
}

#[test]
fn minecart_and_piston_upgrades_stack_the_part_above_the_base() {
    let book = beta_recipe_book();
    for (top, base, result) in [
        (block(Block::Chest), Item::Minecart, Item::ChestMinecart),
        (
            block(Block::FurnaceInactive),
            Item::Minecart,
            Item::FurnaceMinecart,
        ),
        (
            Item::Slimeball,
            block(Block::Piston),
            block(Block::StickyPiston),
        ),
    ] {
        let mut grid = CraftingGrid::player();
        grid.set(0, 0, Some(stack(top, 1)));
        grid.set(0, 1, Some(stack(base, 1)));
        assert_eq!(book.find(&grid), Some(stack(result, 1)));

        grid.set(0, 0, Some(stack(base, 1)));
        grid.set(0, 1, Some(stack(top, 1)));
        assert_eq!(book.find(&grid), None);
    }
}

/// `CraftingManager`: `"###"` makes three slabs, not a full grid of nine.
#[test]
fn three_blocks_in_a_row_craft_three_slabs_of_their_material() {
    let book = beta_recipe_book();
    for (input, data) in [
        (Block::Stone, 0),
        (Block::Sandstone, 1),
        (Block::WoodenPlanks, 2),
        (Block::Cobblestone, 3),
    ] {
        for row in 0..3 {
            let mut grid = CraftingGrid::workbench();
            for x in 0..3 {
                grid.set(x, row, Some(stack(block(input), 1)));
            }
            let slabs = book.find(&grid).unwrap();
            assert_eq!(slabs.item(), block(Block::StoneSlab), "{input:?}");
            assert_eq!(slabs.data(), data, "{input:?}");
            assert_eq!(slabs.count(), 3);
        }
        let mut full = CraftingGrid::workbench();
        for y in 0..3 {
            for x in 0..3 {
                full.set(x, y, Some(stack(block(input), 1)));
            }
        }
        assert!(book.find(&full).is_none(), "nine {input:?}");
    }
}

#[test]
fn fences_take_two_rows_of_sticks_and_sandstone_four_sand() {
    let book = beta_recipe_book();
    let mut grid = CraftingGrid::workbench();
    for y in 0..2 {
        for x in 0..3 {
            grid.set(x, y, Some(stack(Item::Stick, 1)));
        }
    }
    let fences = book.find(&grid).unwrap();
    assert_eq!(fences.item(), block(Block::Fence));
    assert_eq!(fences.count(), 2);

    let mut player = CraftingGrid::player();
    for y in 0..2 {
        for x in 0..2 {
            player.set(x, y, Some(stack(block(Block::Sand), 1)));
        }
    }
    assert_eq!(book.find(&player).unwrap().item(), block(Block::Sandstone));
}

#[test]
fn a_dye_colors_white_wool_with_the_inverted_metadata() {
    let book = beta_recipe_book();
    for dye in 0..16 {
        let mut grid = CraftingGrid::player();
        grid.set(0, 0, Some(ItemStack::with_data(Item::Dye, 1, dye).unwrap()));
        grid.set(
            1,
            0,
            Some(ItemStack::from_block_state(Block::Wool, 0, 1).unwrap()),
        );
        let output = book.find(&grid).expect("dye and white wool craft wool");
        assert_eq!(output.item(), block(Block::Wool));
        assert_eq!(output.data(), 15 - dye, "RecipesDyes: getDyeFromBlock");
    }
}

#[test]
fn bones_grind_to_bone_meal_and_dyes_mix() {
    let book = beta_recipe_book();
    let dye = |data, count| ItemStack::with_data(Item::Dye, count, data).unwrap();
    let mut grid = CraftingGrid::player();
    grid.set(0, 0, Some(stack(Item::Bone, 1)));
    assert_eq!(book.find(&grid), Some(dye(15, 3)));

    for (inputs, output) in [
        (vec![1, 11], dye(14, 2)),
        (vec![0, 15], dye(8, 2)),
        (vec![0, 15, 15], dye(7, 3)),
        (vec![4, 2], dye(6, 2)),
        (vec![4, 1, 1, 15], dye(13, 4)),
    ] {
        let mut grid = CraftingGrid::player();
        for (slot, data) in inputs.into_iter().enumerate() {
            grid.set(slot % 2, slot / 2, Some(dye(data, 1)));
        }
        assert_eq!(book.find(&grid), Some(output));
    }
}

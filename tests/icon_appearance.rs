use game::block::block::BlockId;
use game::block::block::FurnaceFacing;
use game::ui::icon_appearance::Shape;
use game::ui::icon_appearance::block_appearance;
use game::ui::icon_appearance::item_tile;
use game::world::textures::block_tile;

#[test]
fn beta_render_types_select_flat_and_3d_appearances() {
    assert_eq!(block_appearance(1, 0).shape, Shape::Cube);
    assert_eq!(block_appearance(50, 0).shape, Shape::Flat);
    assert_eq!(block_appearance(44, 0).shape, Shape::Slab);
    assert_eq!(block_appearance(53, 0).shape, Shape::Stairs);
    assert_eq!(block_appearance(85, 0).shape, Shape::Fence);
    assert_eq!(block_appearance(81, 0).shape, Shape::Cactus);
    assert_eq!(block_appearance(66, 0).shape, Shape::Flat);
    assert_eq!(block_appearance(78, 0).shape, Shape::Thin);
    assert_eq!(block_appearance(92, 0).shape, Shape::Slab);
}

#[test]
fn beta_subtypes_select_distinct_textures() {
    assert_eq!(block_appearance(17, 0).left, 20);
    assert_eq!(block_appearance(17, 1).left, 116);
    assert_eq!(block_appearance(17, 2).left, 117);
    assert_eq!(block_appearance(35, 0).top, 64);
    assert_eq!(block_appearance(35, 15).top, 113);
    assert_eq!(block_appearance(6, 1).top, 63);
    assert_eq!(block_appearance(44, 1).top, 176);
}

#[test]
fn wood_plank_species_share_the_beta_tile_but_use_species_tints() {
    let oak = block_appearance(5, 0);
    let spruce = block_appearance(5, 1);
    let birch = block_appearance(5, 2);

    assert_eq!((oak.top, spruce.top, birch.top), (4, 4, 4));
    assert_eq!(oak.tint, [255; 3]);
    assert_ne!(spruce.tint, oak.tint);
    assert_ne!(birch.tint, oak.tint);
    assert_ne!(spruce.tint, birch.tint);
}

#[test]
fn standalone_items_use_beta_items_atlas_tiles() {
    assert_eq!(item_tile(256, 0), Some(2 + 5 * 16));
    assert_eq!(item_tile(264, 0), Some(7 + 3 * 16));
    assert_eq!(item_tile(351, 0), Some(14));
    assert_eq!(item_tile(351, 15), Some(14 + 7 * 16 + 1));
    assert_eq!(item_tile(2256, 0), Some(240));
    assert_eq!(item_tile(2257, 0), Some(241));
}

#[test]
fn crafting_table_uses_workbench_tiles_in_world_meshes() {
    assert_eq!(block_tile(BlockId::CraftingTable, 0, true), (11, 2));
    assert_eq!(block_tile(BlockId::CraftingTable, 1, true), (4, 0));
    assert_eq!(block_tile(BlockId::CraftingTable, 2, true), (12, 3));
    assert_eq!(block_tile(BlockId::CraftingTable, 3, true), (11, 3));
    assert_eq!(block_tile(BlockId::CraftingTable, 4, true), (12, 3));
    assert_eq!(block_tile(BlockId::CraftingTable, 5, true), (11, 3));
    assert_ne!(block_tile(BlockId::CraftingTable, 0, true), (1, 0));
}

#[test]
fn snow_layer_and_snow_block_use_beta_snow_tile() {
    assert_eq!(block_tile(BlockId::SnowLayer, 0, false), (2, 4));
    assert_eq!(block_tile(BlockId::Snow, 0, false), (2, 4));
}

#[test]
fn furnace_faces_follow_orientation_and_lit_state() {
    let furnace = BlockId::Furnace.with_furnace_state(FurnaceFacing::East, false);
    let lit = BlockId::Furnace.with_furnace_state(FurnaceFacing::East, true);

    assert_eq!(block_tile(furnace, 0, false), (14, 3));
    assert_eq!(block_tile(furnace, 2, false), (12, 2));
    assert_eq!(block_tile(furnace, 3, false), (13, 2));
    assert_eq!(block_tile(lit, 0, false), (14, 3));
    assert_eq!(block_tile(lit, 2, false), (13, 3));
    assert_eq!(block_tile(lit, 3, false), (13, 2));
    assert_eq!(lit.furnace_facing(), Some(FurnaceFacing::East));
    assert_eq!(lit.item_form(), (BlockId::Furnace, 0));
    assert_eq!(lit.as_u8(), BlockId::from_u8(lit.as_u8()).unwrap().as_u8());
}

#[test]
fn result_item_tiles_are_available_without_clicking_the_result_slot() {
    assert_eq!(item_tile(270, 0), Some(96));
    assert_eq!(item_tile(280, 0), Some(53));
}

use game::ui::icon_appearance::Shape;
use game::ui::icon_appearance::block_appearance;
use game::ui::icon_appearance::item_tile;

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
fn standalone_items_use_beta_items_atlas_tiles() {
    assert_eq!(item_tile(256, 0), Some(2 + 5 * 16));
    assert_eq!(item_tile(264, 0), Some(7 + 3 * 16));
    assert_eq!(item_tile(351, 0), Some(14));
    assert_eq!(item_tile(351, 15), Some(14 + 7 * 16 + 1));
    assert_eq!(item_tile(2256, 0), Some(240));
    assert_eq!(item_tile(2257, 0), Some(241));
}

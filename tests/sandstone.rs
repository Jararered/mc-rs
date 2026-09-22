use game::world::block::block::BlockId;
use game::world::textures::block_tile;

#[test]
fn sandstone_uses_distinct_top_bottom_and_side_textures() {
    assert_eq!(block_tile(BlockId::Sandstone, 0, false), (0, 11));
    assert_eq!(block_tile(BlockId::Sandstone, 1, false), (0, 13));
    for face in 2..6 {
        assert_eq!(block_tile(BlockId::Sandstone, face, false), (0, 12));
    }
}

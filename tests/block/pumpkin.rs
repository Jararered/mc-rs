use game::block::blocks::Block;
use game::block::direction::Direction;
use game::rendering::textures::block_tile;

#[test]
fn pumpkin_metadata_preserves_beta_front_directions() {
    let expected = [
        (Direction::West, 3),
        (Direction::South, 4),
        (Direction::East, 2),
        (Direction::North, 5),
    ];
    let block = Block::Pumpkin;
    assert_eq!(block.item_form(0), (Block::Pumpkin, 0));
    for (metadata, (facing, front_face)) in expected.into_iter().enumerate() {
        let metadata = metadata as u8;
        assert_eq!(block.facing(metadata), Some(facing));
        assert_eq!(block.facing_metadata(facing), metadata);
        for face in 0..6 {
            let tile = block_tile(block, metadata, face, false);
            let expected_tile = if face <= 1 {
                (6, 6)
            } else if face == front_face {
                (7, 7)
            } else {
                (6, 7)
            };
            assert_eq!(tile, expected_tile);
        }
    }
    assert_eq!(Block::Pumpkin.placed(0), Some((Block::Pumpkin, 0)));
}

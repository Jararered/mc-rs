use game::block::id::BlockId;
use game::block::id::FurnaceFacing;
use game::world::textures::block_tile;

#[test]
fn pumpkin_metadata_preserves_beta_front_directions() {
    let expected = [
        (BlockId::PumpkinWest, FurnaceFacing::West, 3),
        (BlockId::PumpkinSouth, FurnaceFacing::South, 4),
        (BlockId::PumpkinEast, FurnaceFacing::East, 2),
        (BlockId::PumpkinNorth, FurnaceFacing::North, 5),
    ];
    for (metadata, (block, facing, front_face)) in expected.into_iter().enumerate() {
        assert_eq!(BlockId::pumpkin_from_metadata(metadata as u32), block);
        assert_eq!(block.pumpkin_facing(), Some(facing));
        assert!(block.in_world());
        assert_eq!(block.item_form(), (BlockId::Pumpkin, 0));
        assert_eq!(BlockId::from_u8(block.as_u8()), Some(block));
        for face in 0..6 {
            let tile = block_tile(block, face, false);
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
    assert_eq!(BlockId::Pumpkin.placed(0), Some(BlockId::Pumpkin));
    assert_eq!(
        BlockId::Pumpkin.with_pumpkin_facing(FurnaceFacing::North),
        BlockId::PumpkinNorth
    );
}

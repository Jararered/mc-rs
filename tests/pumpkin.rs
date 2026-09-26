use game::block::id::FurnaceFacing;
use game::block::id::Id;
use game::world::textures::block_tile;

#[test]
fn pumpkin_metadata_preserves_beta_front_directions() {
    let expected = [
        (Id::PumpkinWest, FurnaceFacing::West, 3),
        (Id::PumpkinSouth, FurnaceFacing::South, 4),
        (Id::PumpkinEast, FurnaceFacing::East, 2),
        (Id::PumpkinNorth, FurnaceFacing::North, 5),
    ];
    for (metadata, (block, facing, front_face)) in expected.into_iter().enumerate() {
        assert_eq!(Id::pumpkin_from_metadata(metadata as u32), block);
        assert_eq!(block.pumpkin_facing(), Some(facing));
        assert!(block.in_world());
        assert_eq!(block.item_form(), (Id::Pumpkin, 0));
        assert_eq!(Id::from_u8(block.as_u8()), Some(block));
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
    assert_eq!(Id::Pumpkin.placed(0), Some(Id::Pumpkin));
    assert_eq!(
        Id::Pumpkin.with_pumpkin_facing(FurnaceFacing::North),
        Id::PumpkinNorth
    );
}

use game::world::chunk::ChunkPos;

#[test]
fn world_coordinates_cross_chunk_boundaries_in_both_directions() {
    assert_eq!(ChunkPos::from_world(15.99, 16.0), ChunkPos { x: 0, z: 1 });
    assert_eq!(ChunkPos::from_world(-0.1, -16.0), ChunkPos { x: -1, z: -1 });
    assert_eq!(ChunkPos::from_world(-16.1, 0.0), ChunkPos { x: -2, z: 0 });
}

#[test]
fn block_coordinates_match_world_floored_chunks() {
    assert_eq!(ChunkPos::from_block(0, 0), ChunkPos::ZERO);
    assert_eq!(ChunkPos::from_block(15, 16), ChunkPos { x: 0, z: 1 });
    assert_eq!(ChunkPos::from_block(-1, -16), ChunkPos { x: -1, z: -1 });
    assert_eq!(ChunkPos::from_block(-17, 0), ChunkPos { x: -2, z: 0 });
}

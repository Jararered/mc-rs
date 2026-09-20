use game::world::{
    chunk::ChunkPos,
    streaming::{LOAD_RADIUS, UNLOAD_RADIUS, positions_in_radius, within_radius},
};

#[test]
fn streaming_radius_is_centered_on_the_player_chunk() {
    let center = ChunkPos { x: -2, z: 3 };
    let positions = positions_in_radius(center, LOAD_RADIUS);
    assert_eq!(positions.len(), 25);
    assert!(positions.contains(&center));
    assert!(positions.contains(&ChunkPos { x: -4, z: 1 }));
    assert!(positions.contains(&ChunkPos { x: 0, z: 5 }));
    assert!(!within_radius(
        ChunkPos { x: 2, z: 3 },
        center,
        UNLOAD_RADIUS
    ));
}

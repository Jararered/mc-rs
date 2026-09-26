use game::world::chunk::ChunkPosition;
use game::world::streaming::LOAD_RADIUS;
use game::world::streaming::UNLOAD_RADIUS;
use game::world::streaming::positions_in_radius;
use game::world::streaming::within_radius;

#[test]
fn streaming_radius_is_centered_on_the_player_chunk() {
    let center = ChunkPosition { x: -2, z: 3 };
    let positions = positions_in_radius(center, LOAD_RADIUS);
    assert_eq!(positions.len(), 81);
    assert!(positions.contains(&center));
    assert!(positions.contains(&ChunkPosition { x: -6, z: -1 }));
    assert!(positions.contains(&ChunkPosition { x: 2, z: 7 }));
    assert!(!within_radius(
        ChunkPosition { x: 4, z: 3 },
        center,
        UNLOAD_RADIUS
    ));
}

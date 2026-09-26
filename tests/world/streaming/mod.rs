use game::world::chunk::ChunkPosition;
use game::world::streaming::LOAD_RADIUS;
use game::world::streaming::UNLOAD_RADIUS;
use game::world::streaming::positions_in_radius;
use game::world::streaming::sort_by_distance;
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
    assert!(within_radius(
        ChunkPosition { x: 2, z: 7 },
        center,
        LOAD_RADIUS
    ));
    assert!(!within_radius(
        ChunkPosition { x: 3, z: 7 },
        center,
        LOAD_RADIUS
    ));
}

#[test]
fn chunk_work_orders_nearest_first_with_stable_ties_and_wide_distances() {
    let center = ChunkPosition { x: -2, z: 3 };
    let mut positions = [
        ChunkPosition { x: 2, z: 3 },
        ChunkPosition { x: -2, z: 3 },
        ChunkPosition { x: -3, z: 2 },
        ChunkPosition { x: -1, z: 3 },
        ChunkPosition { x: 3, z: 3 },
        ChunkPosition {
            x: i32::MIN,
            z: i32::MAX,
        },
    ];

    sort_by_distance(&mut positions, center);

    assert_eq!(
        &positions[..4],
        &[
            ChunkPosition { x: -2, z: 3 },
            ChunkPosition { x: -1, z: 3 },
            ChunkPosition { x: -3, z: 2 },
            ChunkPosition { x: 2, z: 3 },
        ]
    );
    assert!(positions[4].x > positions[5].x);
}

mod systems;

use game::world::chunk::ChunkPosition;
use game::world::streaming::LOAD_RADIUS;
use game::world::streaming::UNLOAD_RADIUS;
use game::world::streaming::Viewers;
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
    assert!(within_radius(
        ChunkPosition {
            x: center.x + UNLOAD_RADIUS,
            z: 3
        },
        center,
        UNLOAD_RADIUS
    ));
    assert!(!within_radius(
        ChunkPosition {
            x: center.x + UNLOAD_RADIUS + 1,
            z: 3
        },
        center,
        UNLOAD_RADIUS
    ));
}

#[test]
fn viewers_cover_the_chunks_around_every_player_once() {
    let near = ChunkPosition { x: 0, z: 0 };
    let overlapping = ChunkPosition { x: 3, z: 0 };
    let far = ChunkPosition { x: 40, z: -7 };
    let viewers = Viewers::new([far, near, overlapping, near]);
    // The same players in another order are the same viewers.
    assert_eq!(viewers, Viewers::new([near, overlapping, far]));
    assert_ne!(viewers, Viewers::new([near, far]));

    assert!(viewers.within(ChunkPosition { x: 42, z: -9 }, 2));
    assert!(viewers.within(ChunkPosition { x: 5, z: 2 }, 2));
    assert!(!viewers.within(ChunkPosition { x: 20, z: 0 }, 2));

    let mut positions = viewers.positions(2);
    // Two overlapping 5x5 squares share a 2x5 strip, and the third stands apart.
    assert_eq!(positions.len(), 25 + 25 - 10 + 25);
    let mut unique = positions.clone();
    unique.sort_by_key(|position| (position.x, position.z));
    unique.dedup();
    assert_eq!(unique.len(), positions.len());

    viewers.sort_by_distance(&mut positions);
    // Each viewer's own chunk comes before anything a step away from one.
    assert!(
        positions[..3]
            .iter()
            .all(|position| [near, overlapping, far].contains(position))
    );
    assert_eq!(
        Viewers::new([near]).positions(LOAD_RADIUS),
        positions_in_radius(near, LOAD_RADIUS)
    );
}

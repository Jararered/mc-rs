//! Beta's creature pathfinder against small hand-built fields.

use bevy::prelude::*;
use game::block::id::Id;
use game::entity::mobs::MobKind;
use game::entity::pathfinding::Pathfinder;
use game::world::biome::Biome;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;

/// Grass at `y` across the nine chunks around the origin.
pub fn field(y: usize) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    for cx in -1..=1 {
        for cz in -1..=1 {
            let mut chunk = Chunk::new();
            for x in 0..16 {
                for z in 0..16 {
                    for below in 0..y {
                        chunk.set(x, below, z, Id::Stone);
                    }
                    chunk.set(x, y, z, Id::Grass);
                }
            }
            chunks.insert(
                ChunkPosition { x: cx, z: cz },
                super::block_ticks::generated(chunk, Biome::Plains),
            );
        }
    }
    chunks
}

/// Fill a column of `block` from `bottom` to `top` at every z in `-12..=12`.
fn wall(chunks: &mut WorldChunks, x: i32, bottom: i32, top: i32, block: Id) {
    for z in -12..=12 {
        for y in bottom..=top {
            chunks.set_block(x, y, z, block);
        }
    }
}

fn pig_path(chunks: &WorldChunks, target: IVec3) -> Option<Vec<IVec3>> {
    Pathfinder::default()
        .path_to_block(
            chunks,
            Vec3::new(0.5, 5.0, 0.5),
            MobKind::Pig.size(0),
            target,
            10.0,
        )
        .map(|path| path.points().to_vec())
}

#[test]
fn crosses_open_ground_in_a_straight_line() {
    let points = pig_path(&field(4), IVec3::new(5, 5, 0)).unwrap();
    let expected: Vec<_> = (0..=5).map(|x| IVec3::new(x, 5, 0)).collect();
    assert_eq!(points, expected);
}

#[test]
fn climbs_one_block_and_steps_back_down() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 5, 5, Id::Stone);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 6, 0)), "{points:?}");
    assert_eq!(points.last(), Some(&IVec3::new(6, 5, 0)));
}

#[test]
fn stops_at_the_nearest_point_when_a_wall_cannot_be_climbed() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 5, 6, Id::Stone);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert_eq!(points.last(), Some(&IVec3::new(2, 5, 0)));
}

#[test]
fn a_creature_already_at_the_closest_point_gets_no_path() {
    let mut chunks = field(4);
    wall(&mut chunks, 1, 5, 6, Id::Stone);
    assert_eq!(pig_path(&chunks, IVec3::new(6, 5, 0)), None);
}

#[test]
fn drops_three_blocks_but_not_four() {
    let mut chunks = field(4);
    for x in 3..=10 {
        for z in -12..=12 {
            for y in 2..=4 {
                chunks.set_block(x, y, z, Id::Air);
            }
        }
    }
    // Ground at y = 1: standing at y = 2 is a three-block drop.
    let points = pig_path(&chunks, IVec3::new(6, 2, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 2, 0)), "{points:?}");
    assert_eq!(points.last(), Some(&IVec3::new(6, 2, 0)));

    for x in 3..=10 {
        for z in -12..=12 {
            chunks.set_block(x, 1, z, Id::Air);
        }
    }
    let points = pig_path(&chunks, IVec3::new(6, 1, 0)).unwrap();
    assert!(points.iter().all(|point| point.x <= 2), "{points:?}");
}

#[test]
fn walks_onto_water_but_never_toward_lava() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 4, 4, Id::Water);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 5, 0)), "{points:?}");

    wall(&mut chunks, 3, 4, 4, Id::Lava);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.iter().all(|point| point.x <= 2), "{points:?}");
}

#[test]
fn passes_open_doors_and_stops_at_closed_ones() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 5, 6, Id::Stone);
    chunks.set_block(3, 5, 0, Id::WoodenDoor);
    chunks.set_block(3, 6, 0, Id::WoodenDoor);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert_eq!(points.last(), Some(&IVec3::new(2, 5, 0)));

    chunks.set_metadata(3, 5, 0, 4);
    chunks.set_metadata(3, 6, 0, 4);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert_eq!(points.last(), Some(&IVec3::new(6, 5, 0)));
    assert!(points.contains(&IVec3::new(3, 5, 0)));
}

#[test]
fn tall_bodies_need_headroom() {
    let mut chunks = field(4);
    // A one-block gap under a ceiling: the pig fits, the cow does not.
    for x in 2..=4 {
        for z in -12..=12 {
            chunks.set_block(x, 6, z, Id::Stone);
        }
    }
    let feet = Vec3::new(0.5, 5.0, 0.5);
    let target = IVec3::new(6, 5, 0);
    let mut finder = Pathfinder::default();
    let pig = finder
        .path_to_block(&chunks, feet, MobKind::Pig.size(0), target, 10.0)
        .unwrap();
    assert_eq!(pig.points().last(), Some(&target));
    let cow = finder
        .path_to_block(&chunks, feet, MobKind::Cow.size(0), target, 10.0)
        .unwrap();
    assert_eq!(cow.points().last(), Some(&IVec3::new(1, 5, 0)));
}

#[test]
fn path_positions_center_the_body_on_each_cell() {
    let mut path = Pathfinder::default()
        .path_to_block(
            &field(4),
            Vec3::new(0.5, 5.0, 0.5),
            MobKind::Pig.size(0),
            IVec3::new(2, 5, 0),
            10.0,
        )
        .unwrap();
    assert_eq!(path.position(0.9), Vec3::new(0.5, 5.0, 0.5));
    path.advance();
    assert_eq!(path.position(0.9), Vec3::new(1.5, 5.0, 0.5));
    // A spider's two-cell body is centered on the corner between cells.
    assert_eq!(path.position(1.4), Vec3::new(2.0, 5.0, 1.0));
    path.advance();
    path.advance();
    assert!(path.is_finished());
}

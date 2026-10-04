//! Beta's creature pathfinder against small hand-built fields.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::mobs::MobType;
use game::entity::pathfinding::LastSearch;
use game::entity::pathfinding::Path;
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
                        chunk.set(x, below, z, Block::Stone);
                    }
                    chunk.set(x, y, z, Block::Grass);
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
fn wall(chunks: &mut WorldChunks, x: i32, bottom: i32, top: i32, block: Block) {
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
            MobType::Pig.size(0),
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
    wall(&mut chunks, 3, 5, 5, Block::Stone);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 6, 0)), "{points:?}");
    assert_eq!(points.last(), Some(&IVec3::new(6, 5, 0)));
}

#[test]
fn stops_at_the_nearest_point_when_a_wall_cannot_be_climbed() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 5, 6, Block::Stone);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert_eq!(points.last(), Some(&IVec3::new(2, 5, 0)));
}

#[test]
fn a_creature_already_at_the_closest_point_gets_no_path() {
    let mut chunks = field(4);
    wall(&mut chunks, 1, 5, 6, Block::Stone);
    assert_eq!(pig_path(&chunks, IVec3::new(6, 5, 0)), None);
}

#[test]
fn drops_three_blocks_but_not_four() {
    let mut chunks = field(4);
    for x in 3..=10 {
        for z in -12..=12 {
            for y in 2..=4 {
                chunks.set_block(x, y, z, Block::Air);
            }
        }
    }
    // Ground at y = 1: standing at y = 2 is a three-block drop.
    let points = pig_path(&chunks, IVec3::new(6, 2, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 2, 0)), "{points:?}");
    assert_eq!(points.last(), Some(&IVec3::new(6, 2, 0)));

    for x in 3..=10 {
        for z in -12..=12 {
            chunks.set_block(x, 1, z, Block::Air);
        }
    }
    let points = pig_path(&chunks, IVec3::new(6, 1, 0)).unwrap();
    assert!(points.iter().all(|point| point.x <= 2), "{points:?}");
}

#[test]
fn walks_onto_water_but_never_toward_lava() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 4, 4, Block::Water);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.contains(&IVec3::new(3, 5, 0)), "{points:?}");

    wall(&mut chunks, 3, 4, 4, Block::Lava);
    let points = pig_path(&chunks, IVec3::new(6, 5, 0)).unwrap();
    assert!(points.iter().all(|point| point.x <= 2), "{points:?}");
}

#[test]
fn passes_open_doors_and_stops_at_closed_ones() {
    let mut chunks = field(4);
    wall(&mut chunks, 3, 5, 6, Block::Stone);
    chunks.set_block(3, 5, 0, Block::WoodenDoor);
    chunks.set_block(3, 6, 0, Block::WoodenDoor);
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
            chunks.set_block(x, 6, z, Block::Stone);
        }
    }
    let feet = Vec3::new(0.5, 5.0, 0.5);
    let target = IVec3::new(6, 5, 0);
    let mut finder = Pathfinder::default();
    let pig = finder
        .path_to_block(&chunks, feet, MobType::Pig.size(0), target, 10.0)
        .unwrap();
    assert_eq!(pig.points().last(), Some(&target));
    let cow = finder
        .path_to_block(&chunks, feet, MobType::Cow.size(0), target, 10.0)
        .unwrap();
    assert_eq!(cow.points().last(), Some(&IVec3::new(1, 5, 0)));
}

#[test]
fn path_positions_center_the_body_on_each_cell() {
    let mut path = Pathfinder::default()
        .path_to_block(
            &field(4),
            Vec3::new(0.5, 5.0, 0.5),
            MobType::Pig.size(0),
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

/// The feet of a player standing on a three-block pillar at the origin.
fn pillar(chunks: &mut WorldChunks) -> Vec3 {
    for y in 5..=7 {
        chunks.set_block(0, y, 0, Block::Stone);
    }
    Vec3::new(0.5, 8.0, 0.5)
}

/// A zombie at `feet` chasing `player`, reusing its `last` search.
fn chase(
    finder: &mut Pathfinder,
    chunks: &WorldChunks,
    feet: Vec3,
    player: Vec3,
    last: &mut LastSearch,
) -> Option<Path> {
    finder.path_to_feet_reusing(chunks, feet, MobType::Zombie.size(0), player, 16.0, last)
}

fn fresh(chunks: &WorldChunks, feet: Vec3, player: Vec3) -> Option<Path> {
    Pathfinder::default().path_to_feet(chunks, feet, MobType::Zombie.size(0), player, 16.0)
}

#[test]
fn an_unchanged_chase_reuses_the_last_search() {
    let mut chunks = field(4);
    let player = pillar(&mut chunks);
    let zombie = Vec3::new(6.5, 5.0, 0.5);
    let mut finder = Pathfinder::default();
    let mut last = LastSearch::default();
    let first = chase(&mut finder, &chunks, zombie, player, &mut last);
    // Shuffling within the same cell asks the same question.
    let again = chase(
        &mut finder,
        &chunks,
        zombie + Vec3::new(0.2, 0.0, 0.1),
        player,
        &mut last,
    );
    assert!(first.is_some());
    assert_eq!(again, first);
    assert_eq!(first, fresh(&chunks, zombie, player));
    let stats = finder.take_stats();
    assert_eq!((stats.searches, stats.reused), (1, 1));

    // At the foot of the pillar no node is closer: no path, and still reused.
    let foot = Vec3::new(1.5, 5.0, 0.5);
    assert_eq!(chase(&mut finder, &chunks, foot, player, &mut last), None);
    assert_eq!(chase(&mut finder, &chunks, foot, player, &mut last), None);
    let stats = finder.take_stats();
    assert_eq!((stats.searches, stats.reused), (1, 1));
}

#[test]
fn an_edit_the_search_could_see_runs_it_again() {
    let mut chunks = field(4);
    let player = pillar(&mut chunks);
    let zombie = Vec3::new(6.5, 5.0, 0.5);
    let mut finder = Pathfinder::default();
    let mut last = LastSearch::default();
    let before = chase(&mut finder, &chunks, zombie, player, &mut last);

    // A step beside the pillar, then a second one, lets the zombie climb up.
    chunks.set_block(1, 5, 0, Block::Stone);
    chunks.set_block(2, 5, 0, Block::Stone);
    chunks.set_block(1, 6, 0, Block::Stone);
    let after = chase(&mut finder, &chunks, zombie, player, &mut last);
    assert_ne!(after, before);
    assert_eq!(after, fresh(&chunks, zombie, player));

    // Opening a door changes only metadata, and that counts too.
    chunks.set_block(4, 5, 0, Block::WoodenDoor);
    chunks.set_block(4, 6, 0, Block::WoodenDoor);
    let closed = chase(&mut finder, &chunks, zombie, player, &mut last);
    assert_eq!(closed, fresh(&chunks, zombie, player));
    chunks.set_metadata(4, 5, 0, 4);
    chunks.set_metadata(4, 6, 0, 4);
    let open = chase(&mut finder, &chunks, zombie, player, &mut last);
    assert_eq!(open, fresh(&chunks, zombie, player));
    let stats = finder.take_stats();
    assert_eq!((stats.searches, stats.reused), (4, 0));
}

#[test]
fn only_edits_inside_the_searched_region_count() {
    let mut chunks = field(4);
    let far = ChunkPosition { x: 6, z: 0 };
    chunks.insert(
        far,
        super::block_ticks::generated(Chunk::new(), Biome::Plains),
    );
    let player = pillar(&mut chunks);
    let zombie = Vec3::new(6.5, 5.0, 0.5);
    let mut finder = Pathfinder::default();
    let mut last = LastSearch::default();
    let first = chase(&mut finder, &chunks, zombie, player, &mut last);

    // The search sees the chunks within 32 blocks of the zombie; x = 100 is
    // well beyond them.
    chunks.set_block(100, 5, 0, Block::Stone);
    assert_eq!(
        chase(&mut finder, &chunks, zombie, player, &mut last),
        first
    );
    assert_eq!(finder.take_stats().reused, 1);

    // Loading or unloading any chunk asks again.
    chunks.remove(far);
    assert_eq!(
        chase(&mut finder, &chunks, zombie, player, &mut last),
        first
    );
    let stats = finder.take_stats();
    assert_eq!((stats.searches, stats.reused), (1, 0));
}

use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::rendering::occlusion::SectionVisibility;
use game::rendering::occlusion::VisibleSections;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::SECTIONS_PER_CHUNK;

// Faces in `geometry::FACE_*` order.
const UP: usize = 0;
const DOWN: usize = 1;
const EAST: usize = 2;
const WEST: usize = 3;
const SOUTH: usize = 4;
const NORTH: usize = 5;

fn filled_section(section: usize) -> Chunk {
    let mut chunk = Chunk::new();
    let base = section * 16;
    for y in base..base + 16 {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                chunk.set(x, y, z, Block::Stone);
            }
        }
    }
    chunk
}

#[test]
fn air_and_stone_sections_connect_everything_and_nothing() {
    assert_eq!(
        SectionVisibility::compute(&Chunk::new(), 2),
        SectionVisibility::ALL
    );
    assert_eq!(
        SectionVisibility::compute(&filled_section(2), 2),
        SectionVisibility::NONE
    );
}

#[test]
fn a_straight_tunnel_joins_only_its_two_ends() {
    let mut chunk = filled_section(3);
    for x in 0..CHUNK_SIZE {
        chunk.set(x, 3 * 16 + 8, 8, Block::Air);
    }
    let visibility = SectionVisibility::compute(&chunk, 3);
    assert!(visibility.connects(WEST, EAST));
    assert!(visibility.connects(EAST, WEST));
    for blocked in [UP, DOWN, NORTH, SOUTH] {
        assert!(!visibility.connects(WEST, blocked));
        assert!(!visibility.connects(blocked, EAST));
    }
    // The other sections are untouched air.
    assert_eq!(
        SectionVisibility::compute(&chunk, 2),
        SectionVisibility::ALL
    );
}

#[test]
fn an_l_shaped_tunnel_turns_a_corner() {
    let mut chunk = filled_section(1);
    for x in 0..=8 {
        chunk.set(x, 16 + 4, 8, Block::Air);
    }
    for z in 8..CHUNK_SIZE {
        chunk.set(8, 16 + 4, z, Block::Air);
    }
    let visibility = SectionVisibility::compute(&chunk, 1);
    assert!(visibility.connects(WEST, SOUTH));
    assert!(!visibility.connects(WEST, EAST));
    assert!(!visibility.connects(NORTH, SOUTH));
    assert_eq!(visibility.exits(1 << WEST), (1 << WEST) | (1 << SOUTH));
}

#[test]
fn leaves_and_glass_do_not_hide_anything() {
    let mut chunk = Chunk::new();
    for (y, block) in [(16, Block::Leaves), (17, Block::Glass)] {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                chunk.set(x, y, z, block);
            }
        }
    }
    assert_eq!(
        SectionVisibility::compute(&chunk, 1),
        SectionVisibility::ALL
    );
}

/// Seven columns along x at z = 0, each section open, except a wall of
/// closed sections in column 3.
fn wall_graph(
    open_at: Option<usize>,
) -> Vec<(ChunkPosition, [SectionVisibility; SECTIONS_PER_CHUNK])> {
    (0..7)
        .map(|x| {
            let mut sections = [SectionVisibility::ALL; SECTIONS_PER_CHUNK];
            if x == 3 {
                sections = [SectionVisibility::NONE; SECTIONS_PER_CHUNK];
                if let Some(section) = open_at {
                    sections[section] = SectionVisibility::ALL;
                }
            }
            (ChunkPosition { x, z: 0 }, sections)
        })
        .collect()
}

fn walk(
    camera: Vec3,
    graph: &[(ChunkPosition, [SectionVisibility; SECTIONS_PER_CHUNK])],
) -> VisibleSections {
    let mut visible = VisibleSections::new(true);
    visible.walk(
        camera,
        graph
            .iter()
            .map(|(position, sections)| (*position, sections)),
    );
    visible
}

#[test]
fn a_closed_wall_hides_the_sections_behind_it() {
    let camera = Vec3::new(8.0, 4.0 * 16.0 + 8.0, 8.0);
    let visible = walk(camera, &wall_graph(None));
    for x in 0..3 {
        assert_eq!(
            visible.mask(ChunkPosition { x, z: 0 }),
            u8::MAX,
            "column {x}"
        );
    }
    // The wall's own sections are drawn: their faces toward the camera show.
    assert_eq!(visible.mask(ChunkPosition { x: 3, z: 0 }), u8::MAX);
    for x in 4..7 {
        assert_eq!(visible.mask(ChunkPosition { x, z: 0 }), 0, "column {x}");
    }
}

#[test]
fn an_opening_in_the_wall_shows_what_is_behind_it() {
    let camera = Vec3::new(8.0, 4.0 * 16.0 + 8.0, 8.0);
    let visible = walk(camera, &wall_graph(Some(4)));
    for x in 4..7 {
        let mask = visible.mask(ChunkPosition { x, z: 0 });
        assert!(mask >> 4 & 1 != 0, "column {x} sees through the gap");
    }
    // Beyond the gap the walk may spread up and down, but never back.
    assert!(visible.is_visible(ChunkPosition { x: 6, z: 0 }, 7));
}

#[test]
fn a_camera_above_the_world_sees_every_top_section() {
    let visible = walk(Vec3::new(8.0, 300.0, 8.0), &wall_graph(None));
    for x in 0..7 {
        assert!(visible.is_visible(ChunkPosition { x, z: 0 }, SECTIONS_PER_CHUNK - 1));
    }
    // The wall column is closed, but its west faces show to the open column
    // beside it, which the walk reaches by going down.
    assert!(visible.is_visible(ChunkPosition { x: 3, z: 0 }, 0));
}

#[test]
fn culling_off_shows_everything() {
    let visible = VisibleSections::new(false);
    assert_eq!(visible.mask(ChunkPosition { x: 40, z: -3 }), u8::MAX);
}

/// Repeatable CPU measurement, independent of GPU presentation and frame caps.
#[test]
#[ignore = "manual performance measurement"]
fn benchmark_visibility_walk() {
    let radius = 24;
    let graph: Vec<_> = (-radius..=radius)
        .flat_map(|z| {
            (-radius..=radius).map(move |x| {
                (
                    ChunkPosition { x, z },
                    [SectionVisibility::ALL; SECTIONS_PER_CHUNK],
                )
            })
        })
        .collect();
    let mut visible = VisibleSections::new(true);
    let mut samples = Vec::new();
    for iteration in 0..220 {
        let start = std::time::Instant::now();
        visible.walk(
            Vec3::new((iteration % 2) as f32 * 16.0 + 8.0, 104.0, 8.0),
            graph
                .iter()
                .map(|(position, sections)| (*position, sections)),
        );
        std::hint::black_box(visible.reached());
        if iteration >= 20 {
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "visibility walk: {:.3} ms median, {:.3} ms p95 ({} sections)",
        samples[100],
        samples[190],
        visible.reached()
    );
}

#[test]
fn repeated_walks_clear_scratch_when_graph_bounds_and_walls_change() {
    let mut visible = VisibleSections::new(true);
    for open_at in [Some(4), None, Some(2), None] {
        let graph = wall_graph(open_at);
        visible.walk(
            Vec3::new(8.0, 72.0, 8.0),
            graph
                .iter()
                .map(|(position, sections)| (*position, sections)),
        );
        assert_eq!(
            visible.is_visible(ChunkPosition { x: 6, z: 0 }, 4),
            open_at == Some(4)
        );
    }
    // Missing columns within the bounds are open, including negative coordinates.
    let graph = [
        (
            ChunkPosition { x: -2, z: -3 },
            [SectionVisibility::ALL; SECTIONS_PER_CHUNK],
        ),
        (
            ChunkPosition { x: 2, z: -1 },
            [SectionVisibility::ALL; SECTIONS_PER_CHUNK],
        ),
    ];
    visible.walk(
        Vec3::new(-24.0, 72.0, -40.0),
        graph.iter().map(|(p, s)| (*p, s)),
    );
    for z in -3..=-1 {
        for x in -2..=2 {
            assert_eq!(visible.mask(ChunkPosition { x, z }), u8::MAX);
        }
    }
    let small = [(
        ChunkPosition { x: -1, z: -2 },
        [SectionVisibility::NONE; SECTIONS_PER_CHUNK],
    )];
    visible.walk(
        Vec3::new(-8.0, 300.0, -24.0),
        small.iter().map(|(p, s)| (*p, s)),
    );
    assert_eq!(visible.mask(small[0].0), 1 << (SECTIONS_PER_CHUNK - 1));
    assert_eq!(visible.mask(ChunkPosition { x: -2, z: -3 }), 0);
    visible.walk(Vec3::ZERO, std::iter::empty());
    assert_eq!(visible.reached(), 0);
    assert_eq!(visible.mask(small[0].0), 0);
}

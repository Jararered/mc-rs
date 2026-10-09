//! Beta's `Teleporter`: finding the exit portal, and building one.

use std::collections::HashMap;

use bevy::math::DVec3;
use game::block::blocks::Block;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::dimension::Dimension;
use game::world::lighting::LightCache;
use game::world::lighting::Skylight;
use game::world::portal::PortalArea;
use game::world::portal::create_portal;
use game::world::portal::find_exit;
use game::world::portal::place_in_portal;

use crate::world::block_ticks::generated;

/// Chunks within `radius` of the origin, each built by `fill`.
fn area(radius: i32, fill: impl Fn(&mut Chunk)) -> PortalArea {
    let mut chunks = HashMap::new();
    for x in -radius..=radius {
        for z in -radius..=radius {
            let mut chunk = Chunk::new();
            fill(&mut chunk);
            chunks.insert(
                ChunkPosition { x, z },
                generated(chunk, game::world::biome::Biome::Hell),
            );
        }
    }
    PortalArea::new(chunks)
}

fn floor(height: usize) -> impl Fn(&mut Chunk) {
    move |chunk| {
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..height {
                    chunk.set(x, y, z, Block::Netherrack);
                }
            }
        }
    }
}

/// Stand a bare 2×3 portal in the chunk at the origin.
fn stand_portal(area: &mut PortalArea, x: i32, y: i32, z: i32, along_x: bool) {
    let mut chunks = std::mem::take(area).into_parts().0;
    for width in 0..2 {
        for height in 0..3 {
            let (px, pz) = if along_x {
                (x + width, z)
            } else {
                (x, z + width)
            };
            let position = ChunkPosition::from_block(px, pz);
            chunks.get_mut(&position).unwrap().chunk.set(
                px.rem_euclid(16) as usize,
                (y + height) as usize,
                pz.rem_euclid(16) as usize,
                Block::NetherPortal,
            );
        }
    }
    *area = PortalArea::new(chunks);
}

#[test]
fn the_player_comes_out_centred_in_the_nearest_portal() {
    let mut area = area(3, floor(40));
    stand_portal(&mut area, 4, 40, 4, true);
    stand_portal(&mut area, -30, 40, 9, false);

    // Half a block above the floor, between the two columns.
    let near = find_exit(&area, DVec3::new(2.0, 42.0, 2.0)).unwrap();
    assert_eq!(near, DVec3::new(5.0, 40.5, 4.5));
    let far = find_exit(&area, DVec3::new(-25.0, 42.0, 2.0)).unwrap();
    assert_eq!(far, DVec3::new(-29.5, 40.5, 10.0));
}

#[test]
fn a_portal_more_than_128_blocks_away_is_not_found() {
    let mut area = area(10, floor(40));
    stand_portal(&mut area, 140, 40, 0, true);
    assert_eq!(find_exit(&area, DVec3::new(11.5, 42.0, 0.5)), None);
    assert!(find_exit(&area, DVec3::new(12.5, 42.0, 0.5)).is_some());
}

#[test]
fn a_new_portal_is_built_on_the_nearest_ledge_with_room() {
    for rotation in 0..4 {
        let mut area = area(2, floor(40));
        let entity = DVec3::new(8.5, 43.0, 8.5);
        let exit = place_in_portal(&mut area, entity, rotation).expect("a portal is built");
        // On the floor, right where the player is.
        assert_eq!(exit.y, 40.5);
        assert!((exit.x - 8.5).abs() <= 1.5 && (exit.z - 8.5).abs() <= 1.5);

        let (chunks, changes) = area.into_parts();
        let count = |block: Block| {
            chunks
                .values()
                .flat_map(|chunk| chunk.chunk.raw_blocks())
                .filter(|raw| **raw == block.as_u8())
                .count()
        };
        assert_eq!(count(Block::NetherPortal), 6);
        // The whole 4×5 frame, corners included, less the four floor cells
        // that replace netherrack one for one.
        assert_eq!(count(Block::Obsidian), 14);
        assert_eq!(changes.len(), 20);
    }
}

#[test]
fn with_nowhere_to_stand_a_platform_is_built_between_70_and_118() {
    // Open air over a floor far below, and solid rock: neither has a ledge.
    for (fill, y, expected) in [(0usize, 30.0, 70), (128, 125.0, 118), (0, 90.0, 90)] {
        let mut area = area(2, floor(fill));
        let entity = DVec3::new(8.5, y, 8.5);
        create_portal(&mut area, entity, 0);
        let exit = find_exit(&area, entity).expect("the platform's portal is found");
        assert_eq!(exit.y, f64::from(expected) + 0.5);
        // The portal stands along z; the platform reaches a block either
        // side of it, with the headroom above cleared even inside rock.
        for dx in [-1, 1] {
            for z in [8, 9] {
                assert_eq!(area.block(8 + dx, expected - 1, z), Block::Obsidian);
                for height in 0..3 {
                    assert_eq!(area.block(8 + dx, expected + height, z), Block::Air);
                }
            }
        }
        assert_eq!(area.block(8, expected, 8), Block::NetherPortal);
        assert_eq!(area.block(8, expected, 7), Block::Obsidian);
    }
}

#[test]
fn portal_coordinates_scale_by_eight() {
    let (x, z) = Dimension::Overworld.scale_position_to(Dimension::Nether, 800.0, -1600.0);
    assert_eq!((x, z), (100.0, -200.0));
    let (x, z) = Dimension::Nether.scale_position_to(Dimension::Overworld, 100.0, -200.0);
    assert_eq!((x, z), (800.0, -1600.0));
    assert_eq!(Dimension::from_id(-1), Dimension::Nether);
    assert_eq!(Dimension::from_id(0), Dimension::Overworld);
    assert_eq!(Dimension::Nether.other(), Dimension::Overworld);
}

#[test]
fn a_dimension_without_a_sky_has_no_sky_light() {
    // A chunk open to the sky, with one glowstone in it.
    let mut chunk = Chunk::new();
    floor(10)(&mut chunk);
    chunk.set(8, 10, 8, Block::Glowstone);

    let overworld = Skylight::from_chunk(&chunk);
    assert_eq!(overworld.sky(2, 20, 2), Some(15));

    let nether = Skylight::from_chunk(&chunk).without_sky();
    assert_eq!(nether.sky(2, 20, 2), Some(0));
    assert_eq!(
        nether.channels_at(2, 200, 2),
        0,
        "nothing shines in from above"
    );
    // Block light is untouched.
    assert_eq!(nether.block(8, 11, 8), overworld.block(8, 11, 8));
    assert!(nether.block(8, 11, 8).unwrap() >= 14);

    let mut cache = LightCache::default();
    cache.set_has_sky(false);
    assert_eq!(cache.channels(0, 300, 0), Some((0, 0)));
    assert_eq!(Dimension::Nether.skylight_subtracted(6000, 0.0), 15);
    assert_eq!(Dimension::Nether.ambient_light(), 0.1);
}

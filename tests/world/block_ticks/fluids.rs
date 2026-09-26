use game::block::fluids::Fluid;
use game::block::fluids::corner_height;
use game::block::fluids::percent_air;
use game::block::id::Id;
use game::world::block_ticks::BlockEvent;

use super::TestWorld;
use super::at;

/// A stone floor at y = 63 across chunk (0, 0) and its neighbors.
fn floored(radius: i32) -> TestWorld {
    let mut world = TestWorld::new(radius);
    let reach = 16 * radius + 15;
    world.fill(
        at(-16 * radius, 63, -16 * radius),
        at(reach, 63, reach),
        Id::Stone,
    );
    world
}

fn fluid_level(world: &TestWorld, x: i32, y: i32, z: i32, fluid: Fluid) -> Option<u8> {
    (Fluid::of(world.block(at(x, y, z))) == Some(fluid)).then(|| world.metadata(at(x, y, z)))
}

#[test]
fn a_water_source_spreads_seven_blocks_across_a_floor() {
    let mut world = floored(1);
    world.place(at(8, 64, 8), Id::FlowingWater);
    world.run(200);

    assert_eq!(fluid_level(&world, 8, 64, 8, Fluid::Water), Some(0));
    for distance in 1..=7 {
        assert_eq!(
            fluid_level(&world, 8 + distance, 64, 8, Fluid::Water),
            Some(distance as u8),
            "level {distance} blocks out"
        );
        assert_eq!(
            fluid_level(&world, 8, 64, 8 - distance, Fluid::Water),
            Some(distance as u8)
        );
    }
    assert_eq!(
        world.block(at(16, 64, 8)),
        Id::Air,
        "water stops after seven"
    );
    assert_eq!(
        fluid_level(&world, 10, 64, 11, Fluid::Water),
        Some(5),
        "the spread is a diamond"
    );
    // Once nothing changes, the flow settles into still water.
    assert_eq!(world.block(at(12, 64, 8)), Id::Water);
    assert!(
        world.ticks.scheduled_count() == 0,
        "a settled pool has nothing left to do"
    );
}

#[test]
fn water_falls_over_an_edge_as_falling_water() {
    let mut world = TestWorld::new(1);
    world.fill(at(4, 60, 4), at(8, 70, 8), Id::Stone);
    world.fill(at(4, 71, 4), at(8, 71, 8), Id::Air);
    world.place(at(8, 71, 6), Id::FlowingWater);
    // Each block of fall waits one five-tick water update.
    world.run(400);

    // Falling water carries metadata 8 and up, all the way to the ground.
    let column: Vec<_> = (0..71)
        .rev()
        .map(|y| fluid_level(&world, 9, y, 6, Fluid::Water))
        .collect();
    assert!(
        column
            .iter()
            .take(70)
            .all(|level| level.is_some_and(|level| level >= 8)),
        "{column:?}"
    );
    assert_eq!(fluid_level(&world, 9, 71, 6, Fluid::Water), Some(1));
}

#[test]
fn flow_heads_for_the_nearest_drop() {
    let mut world = floored(1);
    // A hole three blocks east of the source; nothing to the west.
    world.set(at(11, 63, 8), Id::Air);
    world.place(at(8, 64, 8), Id::FlowingWater);
    world.run(15);
    assert!(fluid_level(&world, 9, 64, 8, Fluid::Water).is_some());
    assert_eq!(
        world.block(at(7, 64, 8)),
        Id::Air,
        "a source only flows toward the cheapest drop"
    );
}

#[test]
fn two_sources_beside_water_on_a_floor_make_a_new_source() {
    let mut world = floored(1);
    world.place(at(6, 64, 8), Id::FlowingWater);
    world.place(at(8, 64, 8), Id::FlowingWater);
    world.run(100);
    assert_eq!(
        fluid_level(&world, 7, 64, 8, Fluid::Water),
        Some(0),
        "the gap between two sources fills in"
    );
}

#[test]
fn water_dries_up_once_its_source_is_gone() {
    let mut world = floored(1);
    world.place(at(8, 64, 8), Id::FlowingWater);
    world.run(200);
    world.place(at(8, 64, 8), Id::Air);
    world.run(400);
    for x in 0..16 {
        assert_eq!(world.block(at(x, 64, 8)), Id::Air, "x = {x}");
    }
}

#[test]
fn breaking_a_lakes_wall_lets_the_still_water_flow() {
    let mut world = floored(1);
    world.fill(at(4, 64, 4), at(8, 64, 8), Id::Water);
    world.fill(at(9, 64, 4), at(9, 64, 8), Id::Stone);
    world.run(20);
    assert_eq!(
        world.block(at(10, 64, 6)),
        Id::Air,
        "the wall holds the lake"
    );

    world.place(at(9, 64, 6), Id::Air);
    assert_eq!(
        world.block(at(8, 64, 6)),
        Id::FlowingWater,
        "the lake wakes up"
    );
    world.run(40);
    assert_eq!(fluid_level(&world, 9, 64, 6, Fluid::Water), Some(1));
    assert_eq!(fluid_level(&world, 10, 64, 6, Fluid::Water), Some(2));
}

#[test]
fn lava_spreads_three_blocks_on_a_floor() {
    let mut world = floored(1);
    world.place(at(8, 64, 8), Id::FlowingLava);
    world.run(1000);
    assert_eq!(fluid_level(&world, 9, 64, 8, Fluid::Lava), Some(2));
    assert_eq!(fluid_level(&world, 10, 64, 8, Fluid::Lava), Some(4));
    assert_eq!(fluid_level(&world, 11, 64, 8, Fluid::Lava), Some(6));
    assert_eq!(world.block(at(12, 64, 8)), Id::Air);
}

#[test]
fn lava_meeting_water_hardens_into_obsidian_or_cobblestone() {
    let mut world = floored(1);
    world.set(at(8, 64, 8), Id::Lava);
    world.set_with_metadata(at(8, 64, 12), Id::FlowingLava, 3);
    world.set_with_metadata(at(8, 64, 14), Id::FlowingLava, 6);
    world.place(at(9, 64, 8), Id::Water);
    world.place(at(9, 64, 12), Id::Water);
    world.place(at(9, 64, 14), Id::Water);
    assert_eq!(world.block(at(8, 64, 8)), Id::Obsidian, "a lava source");
    assert_eq!(
        world.block(at(8, 64, 12)),
        Id::Cobblestone,
        "lava that spread four or less"
    );
    assert!(
        Fluid::of(world.block(at(8, 64, 14))) == Some(Fluid::Lava),
        "thin lava does not harden"
    );
}

#[test]
fn flowing_water_washes_away_plants_and_torches_as_items() {
    let mut world = floored(1);
    world.set(at(9, 64, 8), Id::Torch);
    world.set(at(7, 63, 8), Id::Grass);
    world.set(at(7, 64, 8), Id::Dandelion);
    world.place(at(8, 64, 8), Id::FlowingWater);
    world.run(10);
    let dropped: Vec<_> = world
        .drops()
        .into_iter()
        .map(|(_, block, _)| block)
        .collect();
    assert!(dropped.contains(&Id::Torch), "{dropped:?}");
    assert!(dropped.contains(&Id::Dandelion), "{dropped:?}");
    assert!(Fluid::of(world.block(at(9, 64, 8))).is_some());
}

#[test]
fn fluid_levels_set_beta_surface_heights() {
    assert!((percent_air(0) - 1.0 / 9.0).abs() < 1e-6);
    assert!((percent_air(7) - 8.0 / 9.0).abs() < 1e-6);
    assert!(
        (percent_air(8) - 1.0 / 9.0).abs() < 1e-6,
        "falling fluid is full"
    );

    let source = |_: i32, y: i32, _: i32| {
        if y == 64 {
            (Id::Water, 0)
        } else {
            (Id::Air, 0)
        }
    };
    assert!((corner_height(Fluid::Water, 5, 64, 5, source) - 8.0 / 9.0).abs() < 1e-6);
    let covered = |_: i32, _: i32, _: i32| (Id::Water, 0);
    assert_eq!(corner_height(Fluid::Water, 5, 64, 5, covered), 1.0);
}

#[test]
fn walking_does_not_disturb_fluids() {
    let mut world = floored(1);
    world.set(at(8, 64, 8), Id::Water);
    world.event(BlockEvent::Walked {
        position: at(8, 64, 8),
    });
    assert_eq!(world.block(at(8, 64, 8)), Id::Water);
}

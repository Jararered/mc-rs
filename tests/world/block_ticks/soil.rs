use game::block::id::Id;
use game::world::block_ticks::BlockEvent;
use game::world::block_ticks::behaviors::soil::MAX_MOISTURE;

use super::TestWorld;
use super::at;

/// Noon, so sky light counts in full.
const NOON: u64 = 6000;

/// A 3×3 of chunks with a dirt floor at y = 63 across chunk (0, 0).
fn dirt_field() -> TestWorld {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 60, 0), at(15, 63, 15), Id::Dirt);
    world.time = NOON;
    world
}

#[test]
fn lit_grass_spreads_onto_nearby_dirt() {
    let mut world = dirt_field();
    world.set(at(8, 63, 8), Id::Grass);
    world.relight();
    world.random_ticks(at(8, 63, 8), 200);
    let grass = (7..=9)
        .flat_map(|x| (7..=9).map(move |z| (x, z)))
        .filter(|&(x, z)| world.block(at(x, 63, z)) == Id::Grass)
        .count();
    assert!(grass > 1, "grass spread to {} cells", grass - 1);
    for x in [5, 11] {
        assert_eq!(
            world.block(at(x, 63, 8)),
            Id::Dirt,
            "one random tick reaches one block"
        );
    }
}

#[test]
fn grass_does_not_spread_under_a_block() {
    let mut world = dirt_field();
    world.set(at(8, 63, 8), Id::Grass);
    world.set(at(9, 64, 8), Id::Stone);
    world.relight();
    world.random_ticks(at(8, 63, 8), 400);
    assert_eq!(
        world.block(at(9, 63, 8)),
        Id::Dirt,
        "dirt under stone never takes grass"
    );
}

#[test]
fn grass_covered_by_an_opaque_block_dies_back_to_dirt() {
    let mut world = dirt_field();
    world.fill(at(0, 64, 0), at(15, 64, 15), Id::Stone);
    world.set(at(8, 63, 8), Id::Grass);
    world.relight();
    world.random_ticks(at(8, 63, 8), 50);
    assert_eq!(world.block(at(8, 63, 8)), Id::Dirt);
}

/// The Java condition checks both light *at the water cell* and its opacity.
/// A water column with a stone roof is dark enough for grass to die back.
#[test]
fn grass_under_dark_still_or_flowing_water_dies_back_to_dirt() {
    let grass = at(8, 63, 8);
    let above = at(8, 64, 8);
    for water in [Id::Water, Id::FlowingWater] {
        let mut world = dirt_field();
        world.set(grass, Id::Grass);
        world.fill(at(0, 64, 0), at(15, 64, 15), water);
        world.fill(at(0, 65, 0), at(15, 65, 15), Id::Stone);
        world.relight();
        let light = world
            .ticks
            .world(&mut world.chunks, &mut world.light, NOON)
            .light(above);
        assert!(light < 4, "{water:?} above grass has light {light}");
        world.random_ticks(grass, 80);
        assert_eq!(world.block(grass), Id::Dirt, "under {water:?}");
    }
}

#[test]
fn sunlit_water_does_not_turn_grass_into_dirt() {
    let grass = at(8, 63, 8);
    let above = at(8, 64, 8);
    for water in [Id::Water, Id::FlowingWater] {
        let mut world = dirt_field();
        world.set(grass, Id::Grass);
        world.set(above, water);
        world.relight();
        let light = world
            .ticks
            .world(&mut world.chunks, &mut world.light, NOON)
            .light(above);
        assert!(light >= 4, "{water:?} above grass has light {light}");
        world.random_ticks(grass, 80);
        assert_eq!(world.block(grass), Id::Grass, "under sunlit {water:?}");
    }
}

#[test]
fn farmland_near_water_stays_moist_and_dries_out_without_it() {
    let mut world = dirt_field();
    world.set(at(4, 63, 8), Id::Farmland);
    world.set(at(8, 63, 8), Id::Water);
    world.set(at(14, 63, 8), Id::Farmland);
    world.random_ticks(at(4, 63, 8), 100);
    assert_eq!(world.block(at(4, 63, 8)), Id::Farmland);
    assert_eq!(
        world.metadata(at(4, 63, 8)),
        MAX_MOISTURE,
        "four blocks from water"
    );

    // Six blocks away, dry farmland with nothing planted reverts to dirt.
    world.random_ticks(at(14, 63, 8), 100);
    assert_eq!(world.block(at(14, 63, 8)), Id::Dirt);
}

#[test]
fn moist_farmland_counts_down_before_it_reverts() {
    let mut world = dirt_field();
    world.set_with_metadata(at(8, 63, 8), Id::Farmland, 3);
    world.set(at(8, 64, 8), Id::Crops);
    world.random_ticks(at(8, 63, 8), 200);
    assert_eq!(
        world.block(at(8, 63, 8)),
        Id::Farmland,
        "a crop keeps dry farmland"
    );
    assert_eq!(world.metadata(at(8, 63, 8)), 0);
}

#[test]
fn walking_on_farmland_can_trample_it() {
    let mut world = dirt_field();
    world.set(at(8, 63, 8), Id::Farmland);
    for _ in 0..40 {
        world.event(BlockEvent::Walked {
            position: at(8, 63, 8),
        });
    }
    assert_eq!(
        world.block(at(8, 63, 8)),
        Id::Dirt,
        "one step in four tramples it"
    );
}

#[test]
fn a_solid_block_on_farmland_turns_it_back_into_dirt() {
    let mut world = dirt_field();
    world.set(at(8, 63, 8), Id::Farmland);
    world.place(at(8, 64, 8), Id::Dandelion);
    assert_eq!(
        world.block(at(8, 63, 8)),
        Id::Farmland,
        "a flower is not solid"
    );
    world.place(at(8, 64, 8), Id::Cobblestone);
    assert_eq!(world.block(at(8, 63, 8)), Id::Dirt);
}

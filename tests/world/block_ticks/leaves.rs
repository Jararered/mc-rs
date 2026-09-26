use game::block::id::Id;
use game::world::block_ticks::behaviors::leaves::CHECK_DECAY;
use game::world::block_ticks::behaviors::leaves::supported;

use super::TestWorld;
use super::at;

/// A log at (8, 64, 8) and a line of leaves running east from it.
fn branch(length: i32) -> TestWorld {
    let mut world = TestWorld::new(1);
    world.set(at(8, 64, 8), Id::Wood);
    for x in 9..9 + length {
        world.set(at(x, 64, 8), Id::Leaves);
    }
    world
}

#[test]
fn leaves_within_four_steps_of_a_log_are_supported() {
    let mut world = branch(5);
    let view = world.ticks.world(&mut world.chunks, &mut world.light, 0);
    assert!(supported(&view, at(9, 64, 8)));
    assert!(supported(&view, at(12, 64, 8)), "four steps through leaves");
    assert!(!supported(&view, at(13, 64, 8)), "five steps is too far");
}

#[test]
fn removing_a_log_flags_nearby_leaves_which_then_decay() {
    let mut world = branch(3);
    for x in 9..12 {
        assert_eq!(
            world.metadata(at(x, 64, 8)) & CHECK_DECAY,
            0,
            "generated leaves are unflagged"
        );
    }
    world.place(at(8, 64, 8), Id::Air);
    for x in 9..12 {
        assert_ne!(world.metadata(at(x, 64, 8)) & CHECK_DECAY, 0);
    }
    for x in 9..12 {
        world.random_ticks(at(x, 64, 8), 1);
        assert_eq!(world.block(at(x, 64, 8)), Id::Air);
    }
    let dropped: Vec<_> = world
        .drops()
        .into_iter()
        .map(|(_, block, _)| block)
        .collect();
    assert_eq!(
        dropped,
        vec![Id::Leaves; 3],
        "decayed leaves roll their sapling drop"
    );
}

#[test]
fn a_flagged_leaf_that_finds_a_log_clears_its_flag() {
    let mut world = branch(2);
    world.set(at(10, 65, 8), Id::BirchWood);
    world.place(at(8, 64, 8), Id::Air);
    world.random_ticks(at(9, 64, 8), 1);
    world.random_ticks(at(10, 64, 8), 1);
    assert_eq!(
        world.block(at(9, 64, 8)),
        Id::Leaves,
        "another log still reaches it"
    );
    assert_eq!(world.metadata(at(9, 64, 8)) & CHECK_DECAY, 0);
    assert_eq!(world.block(at(10, 64, 8)), Id::Leaves);
}

#[test]
fn unflagged_leaves_never_check_for_logs() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 64, 8), Id::SpruceLeaves);
    world.random_ticks(at(8, 64, 8), 50);
    assert_eq!(world.block(at(8, 64, 8)), Id::SpruceLeaves);

    // Placed leaves carry the flag, as `ItemLeaves` sets it.
    world.set_with_metadata(at(8, 64, 8), Id::SpruceLeaves, CHECK_DECAY);
    world.random_ticks(at(8, 64, 8), 1);
    assert_eq!(world.block(at(8, 64, 8)), Id::Air);
}

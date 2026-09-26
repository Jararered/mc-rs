use bevy::math::Vec3;
use game::block::id::Id;
use game::entity::falling_block::FallingBlock;
use game::entity::falling_block::FallingStep;
use game::entity::falling_block::step_falling_block;
use game::world::block_ticks::TickEffect;
use game::world::block_ticks::behaviors::falling::can_fall_below;

use super::TestWorld;
use super::at;

#[test]
fn supported_sand_and_gravel_stay_put() {
    let mut world = TestWorld::new(2);
    world.set(at(8, 63, 8), Id::Stone);
    world.set(at(9, 63, 8), Id::Water);
    world.place(at(8, 64, 8), Id::Sand);
    world.place(at(10, 64, 8), Id::Gravel);
    world.set(at(10, 63, 8), Id::Dirt);
    world.run(10);
    assert!(world.effects().is_empty());

    // Gravel over water falls; water does not hold it up.
    world.place(at(9, 64, 8), Id::Gravel);
    world.run(3);
    assert_eq!(
        world.effects(),
        vec![TickEffect::FallingBlock {
            position: at(9, 64, 8),
            block: Id::Gravel,
        }]
    );
    assert!(can_fall_below(Id::Lava) && can_fall_below(Id::Air));
    assert!(!can_fall_below(Id::SnowLayer));
}

#[test]
fn removing_the_block_under_sand_makes_it_fall() {
    let mut world = TestWorld::new(2);
    world.set(at(8, 63, 8), Id::Dirt);
    world.set(at(8, 64, 8), Id::Sand);
    world.run(10);
    world.place(at(8, 63, 8), Id::Air);
    world.run(3);
    assert_eq!(
        world.effects(),
        vec![TickEffect::FallingBlock {
            position: at(8, 64, 8),
            block: Id::Sand,
        }]
    );
}

#[test]
fn a_falling_block_removes_its_source_and_lands_on_the_ground() {
    let mut world = TestWorld::new(2);
    world.set(at(8, 60, 8), Id::Stone);
    world.set(at(8, 70, 8), Id::Sand);
    let mut falling = FallingBlock {
        block: Id::Sand,
        fall_ticks: 0,
        motion: Vec3::ZERO,
        on_ground: false,
    };
    let mut center = Vec3::new(8.5, 70.5, 8.5);
    let mut edits = Vec::new();
    let mut outcome = FallingStep::Falling;
    for tick in 0..100 {
        outcome = step_falling_block(
            &mut falling,
            &mut center,
            &mut world.chunks,
            &mut world.ticks,
            &mut edits,
        );
        if tick == 0 {
            assert_eq!(
                world.block(at(8, 70, 8)),
                Id::Air,
                "the first tick lifts the block out"
            );
        }
        if outcome != FallingStep::Falling {
            break;
        }
    }
    assert_eq!(outcome, FallingStep::Placed(at(8, 61, 8)));
    assert_eq!(world.block(at(8, 61, 8)), Id::Sand);
    assert_eq!(edits, vec![at(8, 70, 8), at(8, 61, 8)]);
    // Both writes queue Beta's neighbor updates for the next tick pass.
    assert!(world.ticks.has_pending_events());
}

#[test]
fn a_falling_block_that_cannot_land_drops_as_an_item() {
    let mut world = TestWorld::new(2);
    world.set(at(8, 60, 8), Id::Stone);
    // A torch on the stone: the sand lands on it (no collision) into the
    // torch's cell, which it cannot replace.
    world.set(at(8, 61, 8), Id::Torch);
    world.set(at(8, 63, 8), Id::Sand);
    let mut falling = FallingBlock {
        block: Id::Sand,
        fall_ticks: 0,
        motion: Vec3::ZERO,
        on_ground: false,
    };
    let mut center = Vec3::new(8.5, 63.5, 8.5);
    let mut edits = Vec::new();
    let outcome = (0..100)
        .map(|_| {
            step_falling_block(
                &mut falling,
                &mut center,
                &mut world.chunks,
                &mut world.ticks,
                &mut edits,
            )
        })
        .find(|outcome| *outcome != FallingStep::Falling);
    assert_eq!(outcome, Some(FallingStep::Dropped(at(8, 61, 8))));
    assert_eq!(world.block(at(8, 61, 8)), Id::Torch);
}

#[test]
fn sand_drops_straight_down_when_its_surroundings_are_not_loaded() {
    // A 3×3 of chunks: enough for the scheduled tick, too little for
    // `EntityFallingSand`, which wants 32 blocks loaded all around.
    let mut world = TestWorld::new(1);
    world.set(at(8, 40, 8), Id::Stone);
    world.set(at(8, 41, 8), Id::Water);
    world.place(at(8, 50, 8), Id::Sand);
    world.run(3);
    assert!(world.effects().is_empty());
    assert_eq!(world.block(at(8, 50, 8)), Id::Air);
    assert_eq!(
        world.block(at(8, 41, 8)),
        Id::Sand,
        "it replaces the water it sinks into"
    );
}

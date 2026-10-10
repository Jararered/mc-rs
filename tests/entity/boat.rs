//! `EntityBoat`: floating, the rider's push, wrecks, turning, and damage.

use bevy::math::IVec3;
use bevy::math::Vec2;
use bevy::math::Vec3;
use game::block::blocks::Block;
use game::entity::EntitySize;
use game::entity::boat::BOAT_SIZE;
use game::entity::boat::Boat;
use game::entity::boat::BoatRules;
use game::entity::boat::rider_motion;
use game::entity::boat::step_boat;
use game::physics::move_entity_with_solids;

use crate::world::block_ticks::TestWorld;
use crate::world::block_ticks::at;

/// A stone floor at y = 60 across chunk (0, 0) under two blocks of still
/// water, whose surface is y = 63.
fn pool() -> TestWorld {
    let mut w = land();
    w.fill(at(0, 61, 0), at(15, 62, 15), Block::Water);
    w
}

/// A stone floor at y = 60 across chunk (0, 0).
fn land() -> TestWorld {
    let mut w = TestWorld::new(1);
    w.fill(at(0, 60, 0), at(15, 60, 15), Block::Stone);
    w
}

fn run(w: &TestWorld, boat: &mut Boat, center: &mut Vec3, ticks: usize) -> bool {
    (0..ticks).any(|_| step_boat(boat, center, None, BoatRules::BETA, &w.chunks).wrecked)
}

#[test]
fn a_boat_dropped_on_water_settles_at_the_surface() {
    let w = pool();
    let mut boat = Boat::default();
    let mut center = Vec3::new(8.5, 64.5, 8.5);
    assert!(!run(&w, &mut boat, &mut center, 300));
    assert!(
        (63.0..63.3).contains(&center.y),
        "floats with its hull half under at {center}"
    );
    assert!(boat.motion.y.abs() < 0.05, "still bobbing: {}", boat.motion);
    assert!(!boat.on_ground);
}

#[test]
fn a_boat_held_under_water_rises() {
    let w = pool();
    let mut boat = Boat {
        motion: Vec3::new(0.0, -0.2, 0.0),
        ..Boat::default()
    };
    let mut center = Vec3::new(8.5, 61.8, 8.5);
    step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    // Fully under: the fall is halved, then 0.007 of lift, then the drag.
    assert!((boat.motion.y - (-0.1 + 0.007) * 0.95).abs() < 1e-5);
}

#[test]
fn a_boat_on_land_falls_to_the_ground_and_grinds_to_a_halt() {
    let w = land();
    let mut boat = Boat {
        motion: Vec3::new(0.2, 0.0, 0.0),
        ..Boat::default()
    };
    let mut center = Vec3::new(4.5, 63.0, 8.5);
    assert!(!run(&w, &mut boat, &mut center, 40));
    assert!(boat.on_ground);
    assert!(
        (center.y - 61.3).abs() < 0.001,
        "rests on the floor: {center}"
    );
    assert!(boat.motion.x.abs() < 0.001, "{}", boat.motion);
}

#[test]
fn a_rider_pushes_the_boat_and_its_speed_is_capped() {
    let w = pool();
    let mut boat = Boat::default();
    let mut center = Vec3::new(4.5, 63.15, 8.5);
    let push = rider_motion(0.0, 1.0, -std::f32::consts::FRAC_PI_2);
    assert!(push.x > 0.017 && push.y.abs() < 1e-6, "{push}");
    for _ in 0..20 {
        step_boat(
            &mut boat,
            &mut center,
            Some(push),
            BoatRules::BETA,
            &w.chunks,
        );
    }
    assert!(boat.motion.x > 0.05, "{}", boat.motion);
    assert!(center.x > 4.9, "{center}");

    let before = center.x;
    step_boat(
        &mut boat,
        &mut center,
        Some(Vec2::new(50.0, 0.0)),
        BoatRules::BETA,
        &w.chunks,
    );
    assert!(
        (center.x - before - 0.4).abs() < 1e-4,
        "{}",
        center.x - before
    );
}

#[test]
fn the_rider_s_motion_is_one_airborne_step_of_walking() {
    // Beta's yaw 0 faces +Z.
    let forward = rider_motion(0.0, 1.0, 0.0);
    assert!(forward.x.abs() < 1e-6);
    assert!((forward.y - 0.98 * 0.02 * 0.91).abs() < 1e-6);
    // Sneaking arrives as 0.3 of the input.
    let sneaking = rider_motion(0.0, 0.3, 0.0);
    assert!((sneaking.y - 0.3 * forward.y).abs() < 1e-6);
    // A diagonal is scaled back to the full acceleration.
    let diagonal = rider_motion(1.0, 1.0, 0.0);
    assert!((diagonal.length() - 0.02 * 0.91).abs() < 1e-6);
    assert_eq!(rider_motion(0.0, 0.0, 1.0), Vec2::ZERO);
}

#[test]
fn a_glancing_hit_at_speed_wrecks_the_boat_but_a_square_or_slow_one_does_not() {
    let mut w = pool();
    w.fill(at(12, 61, 0), at(12, 66, 15), Block::Stone);
    let crash = |motion: Vec3| {
        let mut boat = Boat {
            motion,
            ..Boat::default()
        };
        let mut center = Vec3::new(10.5, 63.15, 6.5);
        let wrecked = run(&w, &mut boat, &mut center, 12);
        (wrecked, boat, center)
    };
    let (wrecked, ..) = crash(Vec3::new(0.3, 0.0, 0.3));
    assert!(wrecked, "sliding along the wall at 0.3 breaks it");
    // `moveEntity` stops the blocked axis before the speed is read.
    let (wrecked, boat, center) = crash(Vec3::new(0.3, 0.0, 0.0));
    assert!(!wrecked);
    assert_eq!(boat.motion.x, 0.0);
    assert!(
        (center.x - 11.25).abs() < 0.001,
        "against the wall: {center}"
    );
    let (wrecked, ..) = crash(Vec3::new(0.1, 0.0, 0.1));
    assert!(!wrecked);
}

#[test]
fn a_boat_turns_at_most_twenty_degrees_a_tick_toward_its_wake() {
    let w = pool();
    let mut boat = Boat {
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Boat::default()
    };
    let mut center = Vec3::new(4.5, 63.15, 8.5);
    step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    assert!((boat.yaw.abs() - 20.0).abs() < 1e-4, "{}", boat.yaw);
    assert_eq!(boat.prev_yaw, 0.0);
    for _ in 0..12 {
        boat.motion.x = 0.3;
        step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    }
    // Moving east, the yaw comes to point west, back along the travel.
    assert!(
        (boat.yaw.rem_euclid(360.0) - 180.0).abs() < 1e-3,
        "{}",
        boat.yaw
    );
}

#[test]
fn the_rider_sits_behind_the_centre_along_the_yaw() {
    let seat = Boat::default().seat();
    assert!((seat - Vec3::new(0.4, -0.3, 0.0)).length() < 1e-6);
    let turned = Boat {
        yaw: 90.0,
        ..Boat::default()
    }
    .seat();
    assert!((turned - Vec3::new(0.0, -0.3, 0.4)).length() < 1e-6);
}

#[test]
fn a_turning_boat_remembers_where_its_seat_was_a_tick_ago() {
    let w = pool();
    let mut boat = Boat {
        motion: Vec3::new(0.0, 0.0, 0.3),
        ..Boat::default()
    };
    let mut center = Vec3::new(8.5, 63.185, 4.5);
    let before = boat.seat();
    step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    assert!(boat.yaw != 0.0);
    assert_eq!(boat.previous_seat(), before);
    assert!((boat.seat() - before).length() > 0.1);
}

#[test]
fn a_boat_clears_the_snow_layers_under_its_corners() {
    let mut w = land();
    w.set(at(8, 61, 8), Block::SnowLayer);
    w.set(at(9, 61, 9), Block::SnowLayer);
    w.set(at(11, 61, 9), Block::SnowLayer);
    let mut boat = Boat::default();
    let mut center = Vec3::new(9.0, 61.3, 9.0);
    let step = step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    let mut snow = step.snow;
    snow.sort_by_key(|cell| (cell.x, cell.z));
    assert_eq!(snow, vec![IVec3::new(8, 61, 8), IVec3::new(9, 61, 9)]);
}

#[test]
fn five_quick_blows_break_a_boat_and_the_damage_wears_off() {
    let w = pool();
    let mut boat = Boat::default();
    for _ in 0..4 {
        assert!(!boat.hurt(1));
    }
    assert_eq!((boat.damage, boat.time_since_hit), (40, 10));
    assert_eq!(boat.rock_direction, 1);
    let mut rested = boat.clone();
    assert!(boat.hurt(1));
    let mut center = Vec3::new(8.5, 63.15, 8.5);
    run(&w, &mut rested, &mut center, 10);
    assert_eq!((rested.damage, rested.time_since_hit), (30, 0));
    assert!(!rested.hurt(1));
}

#[test]
fn a_steady_boat_comes_to_rest_at_beta_s_waterline_and_beta_s_keeps_bobbing() {
    let w = pool();
    let settle = |rules: BoatRules| {
        let mut boat = Boat::default();
        let mut center = Vec3::new(8.5, 64.5, 8.5);
        for _ in 0..60 {
            step_boat(&mut boat, &mut center, None, rules, &w.chunks);
        }
        // The swing over the next two seconds.
        let (mut low, mut high) = (center.y, center.y);
        for _ in 0..40 {
            step_boat(&mut boat, &mut center, None, rules, &w.chunks);
            low = low.min(center.y);
            high = high.max(center.y);
        }
        (low, high)
    };
    let (low, high) = settle(BoatRules::FEATURES);
    assert!(high - low < 0.002, "still moving between {low} and {high}");
    // Where Beta's flips between two and three wet fifths.
    assert!((low - 63.185).abs() < 0.01, "rests at {low}");
    let (low, high) = settle(BoatRules::BETA);
    assert!(high - low > 0.02, "Beta's hunts: {low} to {high}");
}

#[test]
fn with_boat_crashes_a_square_hit_at_speed_wrecks_the_boat_too() {
    let mut w = pool();
    w.fill(at(12, 61, 0), at(12, 66, 15), Block::Stone);
    let crash = |speed: f32| {
        let mut boat = Boat {
            motion: Vec3::new(speed, 0.0, 0.0),
            ..Boat::default()
        };
        let mut center = Vec3::new(11.0, 63.185, 6.5);
        (0..3).any(|_| {
            step_boat(&mut boat, &mut center, None, BoatRules::FEATURES, &w.chunks).wrecked
        })
    };
    assert!(crash(0.3));
    assert!(!crash(0.1), "a gentle bump only stops it");
}

#[test]
fn a_body_can_stand_on_a_boat() {
    let w = land();
    let boat_center = Vec3::new(8.5, 61.3, 8.5);
    let boat = BOAT_SIZE.aabb(boat_center);
    let size = EntitySize {
        width: 0.6,
        height: 1.8,
        y_offset: 0.0,
    };
    // Feet just above the deck, then fall onto it.
    let position = Vec3::new(8.5, boat.max.y + 0.25, 8.5);
    let movement = move_entity_with_solids(
        size.aabb(position),
        Vec3::new(0.0, -1.0, 0.0),
        0.0,
        false,
        &w.chunks,
        &[boat],
    );
    assert!(movement.collision.on_ground);
    assert!((movement.aabb.min.y - boat.max.y).abs() < 1e-4);
}

#[test]
fn a_fast_boat_reports_splash_speed() {
    let w = pool();
    let mut boat = Boat {
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Boat::default()
    };
    let mut center = Vec3::new(8.5, 63.185, 8.5);
    let step = step_boat(&mut boat, &mut center, None, BoatRules::BETA, &w.chunks);
    assert!(step.splash_speed > 0.15, "{}", step.splash_speed);
}

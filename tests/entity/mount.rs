//! `Entity.updateRidden`: a rider's view follows its vehicle's turns.

use bevy::math::Vec2;
use game::entity::mount::RiderDrift;

#[test]
fn a_turn_is_followed_by_half_of_what_is_owed_a_tick_at_most_ten_degrees() {
    let mut drift = RiderDrift::default();
    drift.follow(Vec2::ZERO);
    drift.follow(Vec2::new(30.0, 0.0));
    let turned: Vec<f32> = (0..5).map(|_| drift.step().x).collect();
    assert_eq!(turned, [10.0, 10.0, 5.0, 2.5, 1.25]);
    assert!((drift.yaw - 1.25).abs() < 1e-6);
}

#[test]
fn the_first_look_at_a_vehicle_only_remembers_it() {
    let mut drift = RiderDrift::default();
    drift.follow(Vec2::new(135.0, 20.0));
    assert_eq!(drift.step(), Vec2::ZERO);
}

#[test]
fn what_is_owed_wraps_the_short_way_round() {
    let mut drift = RiderDrift::default();
    drift.follow(Vec2::new(170.0, 0.0));
    // Past 180 to -170: twenty degrees on, not 340 back.
    drift.follow(Vec2::new(-170.0, 0.0));
    assert_eq!(drift.step().x, 10.0);
    assert!((drift.yaw - 10.0).abs() < 1e-4, "{}", drift.yaw);
}

#[test]
fn a_pitching_vehicle_tips_the_view_too() {
    let mut drift = RiderDrift::default();
    drift.follow(Vec2::ZERO);
    drift.follow(Vec2::new(0.0, -8.0));
    assert_eq!(drift.step(), Vec2::new(0.0, -4.0));
}

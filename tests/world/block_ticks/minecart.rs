//! `EntityMinecart`: its motion on rails and off them, bumps between carts,
//! and the damage a cart takes.

use bevy::math::Vec2;
use bevy::math::Vec3;
use game::block::blocks::Block;
use game::entity::minecart::CartBody;
use game::entity::minecart::CartKind;
use game::entity::minecart::Minecart;
use game::entity::minecart::boards;
use game::entity::minecart::collide_carts;
use game::entity::minecart::push_cart;
use game::entity::minecart::step_minecart;
use game::random::JavaRandom;

use super::TestWorld;
use super::at;

/// A straight east-west track at y = 64 from x = 2 to x = 30.
fn straight() -> TestWorld {
    let mut w = TestWorld::new(1);
    for x in 2..=30 {
        w.set_with_metadata(at(x, 64, 8), Block::Rail, 1);
    }
    w
}

fn run(w: &TestWorld, cart: &mut Minecart, center: &mut Vec3, ticks: usize) {
    let mut rng = JavaRandom::new(7);
    for _ in 0..ticks {
        step_minecart(cart, center, &w.chunks, &mut rng);
    }
}

#[test]
fn an_empty_cart_loses_four_percent_a_tick_and_a_rider_only_three_thousandths() {
    let w = straight();
    let mut empty = Minecart {
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut empty, &mut center, 10);
    assert!((empty.motion.x - 0.3 * 0.96f32.powi(10)).abs() < 0.002);
    assert!(
        (center.y - 64.5).abs() < 0.001,
        "rides the rail at {center}"
    );

    let mut ridden = Minecart {
        motion: Vec3::new(0.3, 0.0, 0.0),
        rider: Some(bevy::prelude::Entity::PLACEHOLDER),
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut ridden, &mut center, 10);
    assert!((ridden.motion.x - 0.3 * 0.997f32.powi(10)).abs() < 0.002);
    // A rider's weight shows in the distance: three quarters of the motion.
    assert!(
        (center.x - (4.5 + 0.75 * 0.3 * 10.0)).abs() < 0.2,
        "{center}"
    );
}

#[test]
fn a_cart_off_the_rails_falls_and_slows_on_the_ground() {
    let mut w = TestWorld::new(1);
    w.fill(at(2, 63, 2), at(14, 63, 14), Block::Stone);
    let mut cart = Minecart {
        motion: Vec3::new(0.2, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 68.0, 8.5);
    run(&w, &mut cart, &mut center, 60);
    assert!((center.y - 64.35).abs() < 0.02, "landed at {center}");
    assert!(cart.on_ground);
    assert!(cart.motion.x.abs() < 0.001, "friction stopped it");
}

#[test]
fn gravity_pulls_a_cart_down_a_slope() {
    let mut w = TestWorld::new(1);
    // Shape 2 rises to the east.
    w.set_with_metadata(at(8, 64, 8), Block::Rail, 2);
    let mut cart = Minecart::default();
    let mut center = Vec3::new(8.5, 65.0, 8.5);
    run(&w, &mut cart, &mut center, 1);
    assert!(cart.motion.x < 0.0, "rolled {}", cart.motion);
}

#[test]
fn an_unpowered_rail_brakes_and_a_powered_one_boosts() {
    let mut w = TestWorld::new(1);
    for x in 2..=30 {
        w.set_with_metadata(at(x, 64, 8), Block::PoweredRail, 1);
    }
    let mut braked = Minecart {
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut braked, &mut center, 5);
    assert!(braked.motion.x < 0.3 * 0.5f32.powi(4), "{}", braked.motion);

    for x in 2..=30 {
        w.set_with_metadata(at(x, 64, 8), Block::PoweredRail, 9);
    }
    let mut boosted = Minecart {
        motion: Vec3::new(0.1, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut boosted, &mut center, 5);
    assert!(boosted.motion.x > 0.2, "{}", boosted.motion);
}

#[test]
fn a_furnace_cart_follows_its_push_until_the_fuel_runs_out() {
    let w = straight();
    let mut cart = Minecart {
        kind: CartKind::Furnace,
        push: Vec2::new(1.0, 0.0),
        fuel: 5,
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut cart, &mut center, 30);
    assert!(cart.motion.x > 0.05, "pushed to {}", cart.motion);
    assert!(cart.fuel < 5, "it burns fuel while pushing");

    let mut spent = Minecart {
        kind: CartKind::Furnace,
        push: Vec2::new(1.0, 0.0),
        fuel: 0,
        ..Minecart::default()
    };
    let mut center = Vec3::new(4.5, 64.5, 8.5);
    run(&w, &mut spent, &mut center, 200);
    assert_eq!(spent.push, Vec2::ZERO, "no fuel, no push");
    assert_eq!(spent.fuel, -1);
}

#[test]
fn a_cart_that_hits_a_resting_one_hands_over_its_motion() {
    let mut mover = Minecart {
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut rested = Minecart::default();
    collide_carts(
        &mut CartBody {
            cart: &mut rested,
            center: Vec3::new(9.0, 64.5, 8.5),
            previous: Vec3::new(9.0, 64.5, 8.5),
        },
        &mut CartBody {
            cart: &mut mover,
            center: Vec3::new(8.5, 64.5, 8.5),
            previous: Vec3::new(8.2, 64.5, 8.5),
        },
    );
    assert!((rested.motion.x - 0.2).abs() < 0.001, "{}", rested.motion);
    assert!((mover.motion.x - 0.16).abs() < 0.001, "{}", mover.motion);
}

#[test]
fn a_furnace_cart_shoves_an_empty_one_and_keeps_most_of_its_own_motion() {
    let mut furnace = Minecart {
        kind: CartKind::Furnace,
        motion: Vec3::new(0.3, 0.0, 0.0),
        ..Minecart::default()
    };
    let mut empty = Minecart {
        motion: Vec3::new(0.05, 0.0, 0.0),
        ..Minecart::default()
    };
    collide_carts(
        &mut CartBody {
            cart: &mut empty,
            center: Vec3::new(9.0, 64.5, 8.5),
            previous: Vec3::new(9.0, 64.5, 8.5),
        },
        &mut CartBody {
            cart: &mut furnace,
            center: Vec3::new(8.5, 64.5, 8.5),
            previous: Vec3::new(8.2, 64.5, 8.5),
        },
    );
    assert!(empty.motion.x > 0.3, "shoved to {}", empty.motion);
    assert!(
        (furnace.motion.x - 0.21).abs() < 0.001,
        "{}",
        furnace.motion
    );
}

#[test]
fn a_walker_pushes_a_cart_away_and_is_pushed_back_a_quarter_as_hard() {
    let mut cart = Minecart::default();
    let back = push_cart(
        &mut cart,
        Vec3::new(8.5, 64.5, 8.5),
        Vec3::new(8.0, 64.5, 8.5),
    )
    .expect("they overlap");
    assert!((cart.motion.x - 0.05).abs() < 0.001, "{}", cart.motion);
    assert!((back.x + 0.0125).abs() < 0.0001, "{back}");
    assert!(
        push_cart(
            &mut cart,
            Vec3::new(8.5, 64.5, 8.5),
            Vec3::new(8.5, 70.0, 8.5)
        )
        .is_none(),
        "straight above, they are too close to tell apart"
    );
}

#[test]
fn only_a_free_moving_empty_cart_takes_on_a_creature() {
    let moving = Minecart {
        motion: Vec3::new(0.2, 0.0, 0.0),
        ..Minecart::default()
    };
    assert!(boards(&moving));
    assert!(!boards(&Minecart::default()), "standing still");
    assert!(!boards(&Minecart {
        rider: Some(bevy::prelude::Entity::PLACEHOLDER),
        ..moving.clone()
    }));
    assert!(!boards(&Minecart {
        kind: CartKind::Chest,
        ..moving
    }));
}

#[test]
fn a_cart_breaks_after_enough_damage_and_a_hard_hit_breaks_it_at_once() {
    let mut fist = Minecart::default();
    let hits: Vec<bool> = (0..5).map(|_| fist.hurt(1)).collect();
    assert_eq!(hits, [false, false, false, false, true]);
    assert_eq!(fist.time_since_hit, 10);
    assert_eq!(fist.rock_direction, -1);

    assert!(Minecart::default().hurt(5), "a sword's five points");
}

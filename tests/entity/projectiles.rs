//! Arrows the player owns, thrown snowballs and eggs, and the fishing bobber.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::DroppedItem;
use game::entity::Velocity;
use game::entity::fishing::BOBBER_SIZE;
use game::entity::fishing::Bobber;
use game::entity::fishing::Fishing;
use game::entity::fishing::cast_bobber;
use game::entity::fishing::reel_in;
use game::entity::fishing::reel_motion;
use game::entity::fishing::submerged_fraction;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::entity::projectiles::Arrow;
use game::entity::projectiles::ArrowDamage;
use game::entity::projectiles::Projectile;
use game::entity::projectiles::hand_origin;
use game::entity::projectiles::spawn_arrow_with;
use game::entity::projectiles::spawn_player_arrow;
use game::entity::thrown::Thrown;
use game::entity::thrown::ThrownKind;
use game::entity::thrown::egg_chickens;
use game::entity::thrown::spawn_thrown;
use game::entity::thrown::throw_from_player;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;
use game::item::ItemStack;
use game::player::Player;
use game::player::PlayerHealth;
use game::player::draw_fov_scale;
use game::player::draw_power;
use game::random::ItemRng;
use game::random::JavaRandom;
use game::world::chunk::WorldChunks;

use super::mobs::creature_app;
use super::mobs::feet_of;
use super::mobs::run_ticks;
use super::mobs::summon;
use super::pathfinding::field;

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut().query::<&T>().iter(app.world()).count()
}

fn player(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap()
}

/// Give the test player an empty inventory, with `held` in the first slot.
fn equip(app: &mut App, held: Option<Item>) -> Entity {
    let player = player(app);
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = held.map(|item| ItemStack::new(item, 1).unwrap());
    app.world_mut()
        .entity_mut(player)
        .insert((hotbar, Inventory::default()));
    player
}

fn arrows_held(app: &App, player: Entity) -> u32 {
    app.world()
        .get::<Hotbar>(player)
        .unwrap()
        .slots
        .iter()
        .flatten()
        .filter(|stack| stack.item() == Item::Arrow)
        .map(|stack| u32::from(stack.count()))
        .sum()
}

/// A field with a stone wall at x = 5.
fn walled() -> WorldChunks {
    let mut chunks = field(4);
    for y in 5..=8 {
        for z in -3..=3 {
            chunks.set_block(5, y, z, Block::Stone);
        }
    }
    chunks
}

fn shoot_at_wall(app: &mut App, pickup: bool) -> Entity {
    let arrow = spawn_arrow_with(
        &mut app.world_mut().commands(),
        Vec3::new(0.5, 6.0, 0.5),
        Vec3::X,
        1.5,
        0.0,
        None,
        pickup,
        ArrowDamage::Flat(4),
        &mut JavaRandom::new(1),
    );
    app.world_mut().flush();
    arrow
}

#[test]
fn the_hand_is_right_of_the_eyes_and_a_little_lower() {
    // Facing +Z (Beta yaw 0), the right hand is toward -X.
    let origin = hand_origin(Vec3::new(0.0, 10.0, 0.0), Vec3::Z);
    assert!(
        (origin - Vec3::new(-0.16, 9.9, 0.0)).length() < 1e-6,
        "{origin}"
    );
    // Pitch does not move it sideways.
    let up = hand_origin(Vec3::ZERO, Vec3::new(0.0, 0.8, 0.6));
    assert!((up - Vec3::new(-0.16, -0.1, 0.0)).length() < 1e-6, "{up}");
}

#[test]
fn a_stuck_arrow_the_player_owns_is_picked_up() {
    let mut app = creature_app(walled(), Vec3::new(-6.5, 5.0, 8.5));
    let player = equip(&mut app, None);
    let arrow = shoot_at_wall(&mut app, true);
    run_ticks(&mut app, 40);
    assert!(app.world().get::<Arrow>(arrow).unwrap().is_stuck());
    // Out of reach it stays where it is.
    assert_eq!(arrows_held(&app, player), 0);

    let lodged = feet_of(&app, arrow);
    app.world_mut()
        .get_mut::<Transform>(player)
        .unwrap()
        .translation = Vec3::new(lodged.x - 0.5, 5.0 + 1.62, lodged.z);
    run_ticks(&mut app, 1);
    // It is in the inventory at once and flies to the player for three ticks.
    assert_eq!(arrows_held(&app, player), 1);
    assert_eq!(app.world().get::<Arrow>(arrow).unwrap().taken, Some(0));
    assert_eq!(feet_of(&app, arrow), lodged);
    assert!(app.world().get::<Projectile>(arrow).is_none());
    run_ticks(&mut app, 3);
    assert!(app.world().get_entity(arrow).is_err());
    // One arrow, however long the player stood on it.
    assert_eq!(arrows_held(&app, player), 1);
}

#[test]
fn a_monster_s_arrow_is_left_in_the_wall() {
    let mut app = creature_app(walled(), Vec3::new(-6.5, 5.0, 8.5));
    let player = equip(&mut app, None);
    let arrow = shoot_at_wall(&mut app, false);
    run_ticks(&mut app, 40);
    let lodged = feet_of(&app, arrow);
    app.world_mut()
        .get_mut::<Transform>(player)
        .unwrap()
        .translation = Vec3::new(lodged.x - 0.5, 5.0 + 1.62, lodged.z);
    run_ticks(&mut app, 5);
    assert!(app.world().get_entity(arrow).is_ok());
    assert_eq!(arrows_held(&app, player), 0);
}

#[test]
fn a_quivering_arrow_cannot_be_picked_up_yet() {
    // Beside the arrow's path, within reach of where it lodges.
    let mut app = creature_app(walled(), Vec3::new(4.0, 5.0, 1.5));
    let player = equip(&mut app, None);
    let arrow = shoot_at_wall(&mut app, true);
    // It strikes within a few ticks and shakes for seven more.
    let mut struck_at = None;
    for tick in 0..30 {
        run_ticks(&mut app, 1);
        let Some(state) = app.world().get::<Arrow>(arrow) else {
            assert!(
                struck_at.is_some_and(|struck| tick >= struck + 6),
                "picked up while still shaking"
            );
            assert_eq!(arrows_held(&app, player), 1);
            return;
        };
        if state.is_stuck() && struck_at.is_none() {
            struck_at = Some(tick);
        }
    }
    panic!("the arrow was never picked up");
}

#[test]
fn the_player_s_own_arrow_leaves_without_hitting_them() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, None);
    let eye = app.world().get::<Transform>(player).unwrap().translation;
    let arrow = spawn_player_arrow(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::X,
        1.5,
        ArrowDamage::Flat(4),
        &mut JavaRandom::new(4),
    );
    app.world_mut().flush();
    assert!(app.world().get::<Arrow>(arrow).unwrap().pickup);
    run_ticks(&mut app, 4);
    assert_eq!(app.world().get::<PlayerHealth>(player).unwrap().current, 20);
    assert!(feet_of(&app, arrow).x > 4.0);
}

#[test]
fn a_player_s_arrow_wounds_a_mob_for_four() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, None);
    let pig = summon(
        &mut app,
        Mob::new(MobType::Pig, 5),
        Vec3::new(6.5, 5.0, 0.5),
    );
    let eye = Vec3::new(0.5, 5.6, 0.5);
    spawn_player_arrow(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::X,
        1.5,
        ArrowDamage::Flat(4),
        &mut JavaRandom::new(4),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 8);
    assert_eq!(app.world().get::<Mob>(pig).unwrap().health, 10 - 4);
    assert_eq!(count::<Arrow>(&mut app), 0);
}

#[test]
fn a_charged_arrow_strikes_by_its_speed() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, None);
    let cow = summon(
        &mut app,
        Mob::new(MobType::Cow, 5),
        Vec3::new(5.5, 5.0, 0.5),
    );
    spawn_player_arrow(
        &mut app.world_mut().commands(),
        player,
        Vec3::new(0.5, 5.9, 0.5),
        Vec3::X,
        3.0,
        ArrowDamage::Speed { critical: false },
        &mut JavaRandom::new(4),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 4);
    // Three blocks a tick, slowed by 1% a tick in flight: twice that, rounded up.
    assert_eq!(app.world().get::<Mob>(cow).unwrap().health, 10 - 6);
}

#[test]
fn the_draw_fills_over_a_second() {
    assert_eq!(draw_power(0.0), 0.0);
    // Three ticks is the first draw strong enough to shoot.
    assert!(draw_power(2.0) < 0.1 && draw_power(3.0) > 0.1);
    assert!((draw_power(10.0) - (0.25 + 1.0) / 3.0).abs() < 1e-6);
    assert_eq!(draw_power(20.0), 1.0);
    assert_eq!(draw_power(200.0), 1.0);
}

#[test]
fn a_snowball_knocks_a_mob_back_without_harming_it() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, None);
    let pig = summon(
        &mut app,
        Mob::new(MobType::Pig, 5),
        Vec3::new(5.5, 5.0, 0.5),
    );
    throw_from_player(
        &mut app.world_mut().commands(),
        ThrownKind::Snowball,
        player,
        Vec3::new(0.5, 5.6, 0.5),
        Vec3::X,
        &mut JavaRandom::new(2),
    );
    app.world_mut().flush();
    let mut pushed = false;
    for _ in 0..10 {
        run_ticks(&mut app, 1);
        pushed |= app.world().get::<Velocity>(pig).unwrap().0.x > 0.0;
    }
    assert!(pushed, "the pig was knocked away from the thrower");
    assert_eq!(app.world().get::<Mob>(pig).unwrap().health, 10);
    assert_eq!(count::<Thrown>(&mut app), 0);
}

#[test]
fn a_thrown_egg_bursts_on_the_ground() {
    let mut app = creature_app(field(4), Vec3::new(8.5, 5.0, 8.5));
    spawn_thrown(
        &mut app.world_mut().commands(),
        ThrownKind::Egg,
        Vec3::new(0.5, 7.0, 0.5),
        Vec3::new(1.0, 0.1, 0.0),
        1.1,
        6.0,
        None,
        &mut JavaRandom::new(2),
    );
    app.world_mut().flush();
    assert_eq!(count::<Thrown>(&mut app), 1);
    run_ticks(&mut app, 40);
    assert_eq!(count::<Thrown>(&mut app), 0);
}

#[test]
fn one_egg_in_eight_hatches_and_a_few_hatch_four() {
    let mut rng = JavaRandom::new(77);
    let rolls: Vec<u32> = (0..32_000).map(|_| egg_chickens(&mut rng)).collect();
    let hatched = rolls.iter().filter(|&&chickens| chickens > 0).count();
    let broods = rolls.iter().filter(|&&chickens| chickens == 4).count();
    assert!(rolls.iter().all(|chickens| matches!(chickens, 0 | 1 | 4)));
    assert!((3_600..4_400).contains(&hatched), "{hatched}");
    assert!((70..190).contains(&broods), "{broods}");
}

/// A field with a pool four deep under x in 2..=14 and z in 0..=8.
fn pond() -> WorldChunks {
    let mut chunks = field(4);
    for x in 2..=14 {
        for z in 0..=8 {
            for y in 1..=4 {
                chunks.set_block(x, y, z, Block::Water);
            }
        }
    }
    chunks
}

#[test]
fn the_bobber_s_slices_measure_how_deep_it_sits() {
    let chunks = pond();
    let at = |y: f32| submerged_fraction(BOBBER_SIZE.aabb(Vec3::new(4.5, y, 4.5)), &chunks);
    assert_eq!(at(4.0), 1.0);
    assert_eq!(at(6.0), 0.0);
    // The surface of a source block is its full height.
    let half = at(5.0 - 0.125);
    assert!(half > 0.0 && half < 1.0, "{half}");
}

#[test]
fn a_cast_bobber_floats_until_the_rod_is_put_away() {
    let mut app = creature_app(pond(), Vec3::new(0.5, 5.0, 4.5));
    let player = equip(&mut app, Some(Item::FishingRod));
    let eye = app.world().get::<Transform>(player).unwrap().translation;
    let bobber = cast_bobber(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::new(1.0, -0.4, 0.0).normalize(),
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    assert_eq!(app.world().get::<Fishing>(player).unwrap().0, bobber);
    run_ticks(&mut app, 120);
    let floating = feet_of(&app, bobber);
    assert!(
        (2.0..15.0).contains(&floating.x) && (0.0..9.0).contains(&floating.z),
        "{floating}"
    );
    // It rides at the surface, partly under.
    assert!((4.6..5.0).contains(&floating.y), "{floating}");
    assert!(!app.world().get::<Bobber>(bobber).unwrap().in_ground);

    app.world_mut().get_mut::<Hotbar>(player).unwrap().selected = 1;
    run_ticks(&mut app, 1);
    assert!(app.world().get_entity(bobber).is_err());
    assert!(app.world().get::<Fishing>(player).is_none());
}

#[test]
fn a_bobber_that_lands_on_the_ground_stays_there() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, Some(Item::FishingRod));
    let eye = app.world().get::<Transform>(player).unwrap().translation;
    let bobber = cast_bobber(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::X,
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 60);
    let state = app.world().get::<Bobber>(bobber).unwrap();
    assert!(state.in_ground);
    assert_eq!(state.reel().damage, 2);
    assert!(!state.reel().fish);
}

#[test]
fn the_line_snaps_past_thirty_two_blocks() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, Some(Item::FishingRod));
    let eye = app.world().get::<Transform>(player).unwrap().translation;
    let bobber = cast_bobber(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::X,
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 5);
    assert!(app.world().get_entity(bobber).is_ok());
    app.world_mut()
        .get_mut::<Transform>(player)
        .unwrap()
        .translation
        .z -= 40.0;
    run_ticks(&mut app, 1);
    assert!(app.world().get_entity(bobber).is_err());
}

#[test]
fn reeling_in_a_bite_flings_a_fish_at_the_angler() {
    let mut app = creature_app(pond(), Vec3::new(0.5, 5.0, 4.5));
    let player = equip(&mut app, Some(Item::FishingRod));
    let eye = app.world().get::<Transform>(player).unwrap().translation;
    let bobber = cast_bobber(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::new(1.0, -0.4, 0.0).normalize(),
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 60);
    // No bite yet: reeling in costs nothing and catches nothing.
    assert_eq!(app.world().get::<Bobber>(bobber).unwrap().reel().damage, 0);
    app.world_mut()
        .get_mut::<Bobber>(bobber)
        .unwrap()
        .ticks_catchable = 20;
    let state = app.world().get::<Bobber>(bobber).unwrap().clone();
    assert!(state.reel().fish);
    let at = feet_of(&app, bobber);
    let damage = app
        .world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut rng: Local<ItemRng>,
                  mut mobs: Query<&mut Velocity, With<Mob>>| {
                reel_in(&mut commands, &mut rng, bobber, &state, at, eye, &mut mobs)
            },
        )
        .unwrap();
    assert_eq!(damage, 1);
    assert!(app.world().get_entity(bobber).is_err());
    assert!(app.world().get::<Fishing>(player).is_none());
    let fish: Vec<Item> = app
        .world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .map(|dropped| dropped.0.item())
        .collect();
    assert_eq!(fish, [Item::RawFish]);
}

#[test]
fn a_catch_is_thrown_toward_the_angler_and_up() {
    let motion = reel_motion(Vec3::new(10.0, 4.0, 0.0), Vec3::new(0.0, 6.0, 0.0));
    assert!((motion.x + 1.0).abs() < 1e-6);
    let lift = 0.2 + (104.0_f32).sqrt().sqrt() * 0.08;
    assert!((motion.y - lift).abs() < 1e-5, "{}", motion.y);
    assert_eq!(motion.z, 0.0);
}

#[test]
fn a_hooked_mob_is_dragged_toward_the_angler() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let player = equip(&mut app, Some(Item::FishingRod));
    let pig = summon(
        &mut app,
        Mob::new(MobType::Pig, 5),
        Vec3::new(5.5, 5.0, 0.5),
    );
    let eye = Vec3::new(0.5, 5.6, 0.5);
    let bobber = cast_bobber(
        &mut app.world_mut().commands(),
        player,
        eye,
        Vec3::X,
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 12);
    let state = app.world().get::<Bobber>(bobber).unwrap().clone();
    assert_eq!(state.hooked, Some(pig));
    assert_eq!(state.reel().damage, 3);
    // The bobber rides on the pig.
    let pig_at = feet_of(&app, pig);
    let at = feet_of(&app, bobber);
    assert!((at.x - pig_at.x).abs() < 1e-4 && at.y > pig_at.y);

    app.world_mut().get_mut::<Velocity>(pig).unwrap().0 = Vec3::ZERO;
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  mut rng: Local<ItemRng>,
                  mut mobs: Query<&mut Velocity, With<Mob>>| {
                reel_in(&mut commands, &mut rng, bobber, &state, at, eye, &mut mobs)
            },
        )
        .unwrap();
    let pull = app.world().get::<Velocity>(pig).unwrap().0;
    assert!(pull.x < 0.0 && pull.y > 0.0, "{pull}");
}

#[test]
fn drawing_the_bow_narrows_the_view_by_up_to_fifteen_percent() {
    assert_eq!(draw_fov_scale(0.0), 1.0);
    // It starts slowly: a quarter of the way in at half a second.
    assert!((draw_fov_scale(10.0) - (1.0 - 0.25 * 0.15)).abs() < 1e-6);
    assert!((draw_fov_scale(20.0) - 0.85).abs() < 1e-6);
    assert!((draw_fov_scale(400.0) - 0.85).abs() < 1e-6);
}

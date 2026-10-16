//! Targets other than the player: `EntityMob.attackEntityFrom` turning a
//! monster on whatever hit it, the wolf pack (`EntityWolf.attackEntityFrom`),
//! the tamed wolves `EntityPlayer.alertWolves` calls, a wild wolf's sheep
//! hunt, cactus against mobs, and the giant zombie.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::Velocity;
use game::entity::combat::Assailant;
use game::entity::combat::Attacker;
use game::entity::combat::Hit;
use game::entity::combat::Source;
use game::entity::combat::hurt_creature;
use game::entity::creature::Living;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::player::Player;
use game::player::PlayerHealth;
use game::world::tick::WorldTick;

use super::mobs::creature_app;
use super::mobs::run_ticks;
use super::mobs::summon;
use super::pathfinding::field;

const MIDNIGHT: u64 = 18_000;
/// More than 16 blocks from where the mobs below stand, so none of them
/// picks the player out for itself.
const FAR: Vec3 = Vec3::new(-20.5, 5.0, -20.5);

fn at_night(app: &mut App) {
    app.world_mut()
        .resource_mut::<WorldTick>()
        .set_world_time(MIDNIGHT);
}

fn living(app: &App, entity: Entity) -> &Living {
    app.world().get::<Living>(entity).unwrap()
}

fn health(app: &App, entity: Entity) -> i16 {
    app.world().get::<Mob>(entity).unwrap().health
}

fn set_target(app: &mut App, entity: Entity, target: Entity) {
    app.world_mut()
        .get_mut::<Living>(entity)
        .unwrap()
        .set_target(Some(target));
}

fn player(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap()
}

fn hit(attacker: Option<Attacker>) -> Hit {
    Hit {
        amount: 2,
        from: None,
        source: Source::Creature,
        attacker,
    }
}

#[test]
fn a_monster_turns_on_the_creature_that_hit_it_and_keeps_its_target_otherwise() {
    let mut zombie = Mob::new(MobType::Zombie, 1);
    let mut living = Living::facing(0.0);
    let mut velocity = Velocity::default();
    let wolf = World::new().spawn_empty().id();
    let blow = hit(Some(Attacker::creature(wolf, MobType::Wolf)));
    assert!(hurt_creature(&mut zombie, &mut living, &mut velocity, Vec3::ZERO, blow).landed);
    assert_eq!(living.target(), Some(wolf));

    // Cactus, fire and falling name no attacker: the target stays.
    let mut fresh = Living::facing(0.0);
    fresh.set_target(Some(wolf));
    hurt_creature(
        &mut zombie,
        &mut fresh,
        &mut velocity,
        Vec3::ZERO,
        hit(None),
    );
    assert_eq!(fresh.target(), Some(wolf));

    // An arrow is the attacker Beta names, and it is gone a tick later.
    let mut shot = Living::facing(0.0);
    shot.set_target(Some(wolf));
    let arrow = hit(Some(Attacker::Arrow {
        shooter: Some(Assailant::Player(wolf)),
    }));
    hurt_creature(&mut zombie, &mut shot, &mut velocity, Vec3::ZERO, arrow);
    assert_eq!(shot.target(), None);
    assert!(!shot.chasing());
}

#[test]
fn an_arrow_into_a_wild_wolf_sends_the_pack_after_the_archer() {
    let mut wolf = Mob::new(MobType::Wolf, 2);
    let mut living = Living::facing(0.0);
    let mut velocity = Velocity::default();
    let archer = World::new().spawn_empty().id();
    let arrow = Hit {
        amount: 2,
        from: None,
        source: Source::Player,
        attacker: Some(Attacker::Arrow {
            shooter: Some(Assailant::Player(archer)),
        }),
    };
    let wound = hurt_creature(&mut wolf, &mut living, &mut velocity, Vec3::ZERO, arrow);
    // Not halved: Beta halves blows from anything but a player or an arrow.
    assert_eq!(wolf.health, 6);
    // The arrow is no player, so the wolf itself is not angered; its pack
    // call, which includes it, is.
    assert!(!wolf.angry);
    assert_eq!(wound.rally, Some(Assailant::Player(archer)));
}

#[test]
fn a_zombie_bitten_by_a_wolf_fights_back() {
    let mut app = creature_app(field(4), FAR);
    at_night(&mut app);
    let mut angry = Mob::new(MobType::Wolf, 4);
    angry.angry = true;
    let wolf = summon(&mut app, angry, Vec3::new(0.5, 5.0, 0.5));
    let zombie = summon(
        &mut app,
        Mob::new(MobType::Zombie, 5),
        Vec3::new(1.5, 5.0, 0.5),
    );
    set_target(&mut app, wolf, zombie);
    for _ in 0..200 {
        run_ticks(&mut app, 1);
        if health(&app, zombie) < 20 {
            break;
        }
    }
    assert!(health(&app, zombie) < 20, "the wolf never bit");
    assert_eq!(living(&app, zombie).target(), Some(wolf));
}

#[test]
fn a_wild_wolf_hunts_a_sheep() {
    let mut app = creature_app(field(4), FAR);
    let wolf = summon(
        &mut app,
        Mob::new(MobType::Wolf, 6),
        Vec3::new(0.5, 5.0, 0.5),
    );
    let sheep = summon(
        &mut app,
        Mob::new(MobType::Sheep, 7),
        Vec3::new(3.5, 5.0, 0.5),
    );
    let mut hunted = false;
    for _ in 0..4000 {
        run_ticks(&mut app, 1);
        hunted |= living(&app, wolf).target() == Some(sheep);
        if health(&app, sheep) < 10 {
            break;
        }
    }
    assert!(hunted, "the wolf never picked the sheep out");
    assert!(health(&app, sheep) < 10, "the wolf never bit the sheep");
    // A wolf on the hunt is not angry: only a player angers one.
    assert!(!app.world().get::<Mob>(wolf).unwrap().angry);
}

#[test]
fn a_tamed_wolf_goes_for_what_hurts_its_owner() {
    let owner_feet = Vec3::new(0.5, 5.0, 0.5);
    let mut app = creature_app(field(4), owner_feet);
    at_night(&mut app);
    let owner = player(&mut app);
    let mut pet = Mob::new(MobType::Wolf, 8);
    pet.tamed = true;
    pet.owner = Some("Player".to_owned());
    let wolf = summon(&mut app, pet, Vec3::new(-2.5, 5.0, 0.5));
    let zombie = summon(
        &mut app,
        Mob::new(MobType::Zombie, 9),
        Vec3::new(1.5, 5.0, 0.5),
    );
    set_target(&mut app, zombie, owner);
    for _ in 0..200 {
        run_ticks(&mut app, 1);
        if app.world().get::<PlayerHealth>(owner).unwrap().current < 20 {
            break;
        }
    }
    assert!(app.world().get::<PlayerHealth>(owner).unwrap().current < 20);
    assert_eq!(living(&app, wolf).target(), Some(zombie));
}

#[test]
fn cactus_pricks_a_mob_pressed_against_it_without_changing_its_target() {
    let mut chunks = field(4);
    chunks.set_block(1, 5, 0, Block::Cactus);
    let mut app = creature_app(chunks, FAR);
    at_night(&mut app);
    let owner = player(&mut app);
    // A zombie's box reaches past x = 1, short of the cactus's inset side.
    let zombie = summon(
        &mut app,
        Mob::new(MobType::Zombie, 10),
        Vec3::new(0.75, 5.0, 0.5),
    );
    set_target(&mut app, zombie, owner);
    run_ticks(&mut app, 1);
    assert_eq!(health(&app, zombie), 19);
    assert_eq!(living(&app, zombie).target(), Some(owner));

    let mut clear = creature_app(field(4), FAR);
    at_night(&mut clear);
    let far_zombie = summon(
        &mut clear,
        Mob::new(MobType::Zombie, 10),
        Vec3::new(0.5, 5.0, 0.5),
    );
    run_ticks(&mut clear, 5);
    assert_eq!(health(&clear, far_zombie), 20);
}

#[test]
fn a_giant_is_a_zombie_six_times_over() {
    let size = MobType::Giant.size(0);
    assert_eq!((size.width, size.height), (3.6, 10.8));
    assert_eq!(MobType::Giant.health(0), 200);
    assert!(MobType::Giant.hostile() && MobType::Giant.is_monster());
    assert_eq!(MobType::parse("giant"), Some(MobType::Giant));

    // It strikes for 50, which no player survives unarmored. It stands in
    // the open sun all the while, which would set a zombie alight.
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    let owner = player(&mut app);
    let giant = summon(
        &mut app,
        Mob::new(MobType::Giant, 11),
        Vec3::new(1.5, 5.0, 0.5),
    );
    set_target(&mut app, giant, owner);
    for _ in 0..100 {
        run_ticks(&mut app, 1);
        if app.world().get::<PlayerHealth>(owner).unwrap().current < 20 {
            break;
        }
    }
    assert_eq!(app.world().get::<PlayerHealth>(owner).unwrap().current, 0);
    assert!(app.world().get::<Mob>(giant).unwrap().fire_ticks <= 0);
}

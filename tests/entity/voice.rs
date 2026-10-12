//! Mob voices: the table, the cry a hit draws, and idle talk.

use bevy::prelude::*;
use game::audio::world::voices;
use game::entity::combat::PlayerCombat;
use game::entity::creature::Living;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::entity::voice::PLAYER_HURT;
use game::entity::voice::voice;
use game::player::Player;
use game::player::PlayerHealth;
use game::world::sound::SoundRequest;
use game::world::sound::Sounds;

use super::mobs::creature_app;
use super::mobs::run_ticks;
use super::mobs::summon;
use super::pathfinding::field;

#[test]
fn every_mob_has_betas_voice() {
    for kind in MobType::ALL {
        let voice = voice(kind);
        assert!(voice.volume > 0.0, "{kind:?}");
        assert!(matches!(voice.talk_interval, 80 | 120), "{kind:?}");
    }
    let pig = voice(MobType::Pig);
    assert_eq!(pig.living, Some("mob.pig"));
    assert_eq!(pig.hurt, Some("mob.pig"));
    assert_eq!(pig.death, Some("mob.pigdeath"));
    assert_eq!((pig.volume, pig.talk_interval), (1.0, 120));
    // A cow is quiet, and animals wait longer between sounds than monsters.
    assert_eq!(voice(MobType::Cow).volume, 0.4);
    assert_eq!(voice(MobType::Zombie).talk_interval, 80);
    assert_eq!(voice(MobType::Zombie).hurt, Some("mob.zombiehurt"));
    assert_eq!(voice(MobType::Skeleton).death, Some("mob.skeletonhurt"));
    // A creeper and a slime are silent until something happens to them.
    assert_eq!(voice(MobType::Creeper).living, None);
    assert_eq!(voice(MobType::Creeper).death, Some("mob.creeperdeath"));
    assert_eq!(voice(MobType::Slime).living, None);
    assert_eq!(voice(MobType::Slime).volume, 0.6);
    // A squid makes no sound at all, and a ghast is heard from far away.
    let squid = voice(MobType::Squid);
    assert_eq!((squid.living, squid.hurt, squid.death), (None, None, None));
    assert_eq!(voice(MobType::Ghast).volume, 10.0);
    assert_eq!(voice(MobType::Ghast).living, Some("mob.ghast.moan"));
    assert_eq!(
        voice(MobType::PigZombie).hurt,
        Some("mob.zombiepig.zpighurt")
    );
}

/// A field with a far-off player, where cries are heard but nothing ticks
/// until [`run_ticks`] is called.
fn listening_app() -> App {
    let mut app = creature_app(field(4), Vec3::new(40.5, 5.0, 40.5));
    app.init_resource::<Sounds>()
        .add_systems(PostUpdate, voices);
    app
}

fn hear(app: &mut App) -> Vec<SoundRequest> {
    app.world_mut().run_schedule(PostUpdate);
    app.world_mut().resource_mut::<Sounds>().drain().collect()
}

fn strike(app: &mut App, mob: Entity, health: i16) {
    app.world_mut().get_mut::<Mob>(mob).unwrap().health = health;
    app.world_mut().get_mut::<Living>(mob).unwrap().hurt_time = 10;
}

#[test]
fn a_struck_mob_cries_out_once_and_dies_with_another_sound() {
    let mut app = listening_app();
    let feet = Vec3::new(8.5, 5.0, 8.5);
    let cow = summon(&mut app, Mob::new(MobType::Cow, 3), feet);
    assert!(hear(&mut app).is_empty(), "an unhurt mob is not heard");

    strike(&mut app, cow, 6);
    let cries = hear(&mut app);
    assert_eq!(cries.len(), 1);
    assert_eq!(cries[0].name, "mob.cowhurt");
    assert_eq!(cries[0].position, Some(feet));
    assert_eq!(cries[0].volume, 0.4);
    assert!((0.8..=1.2).contains(&cries[0].pitch));
    // The same hit is not heard twice, nor as it wears off.
    assert!(hear(&mut app).is_empty());
    app.world_mut().get_mut::<Living>(cow).unwrap().hurt_time = 4;
    assert!(hear(&mut app).is_empty());
    app.world_mut().get_mut::<Living>(cow).unwrap().hurt_time = 0;
    assert!(hear(&mut app).is_empty());

    // The blow that kills a pig is the one with its death sound.
    let pig = summon(&mut app, Mob::new(MobType::Pig, 3), feet);
    hear(&mut app);
    strike(&mut app, pig, 0);
    let cries = hear(&mut app);
    assert_eq!(cries.len(), 1);
    assert_eq!(cries[0].name, "mob.pigdeath");

    // A squid has nothing to say about it.
    let squid = summon(&mut app, Mob::new(MobType::Squid, 3), feet);
    hear(&mut app);
    strike(&mut app, squid, 2);
    assert!(hear(&mut app).is_empty());
}

#[test]
fn a_hurt_player_is_heard_at_their_feet() {
    let mut app = listening_app();
    let player = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(player)
        .insert(PlayerCombat::default());
    assert!(hear(&mut app).is_empty());
    app.world_mut()
        .get_mut::<PlayerCombat>(player)
        .unwrap()
        .hurt_time = 10;
    app.world_mut()
        .get_mut::<PlayerHealth>(player)
        .unwrap()
        .current = 12;
    let cries = hear(&mut app);
    assert_eq!(cries.len(), 1);
    assert_eq!(cries[0].name, PLAYER_HURT);
    assert_eq!(cries[0].position, Some(Vec3::new(40.5, 5.0, 40.5)));
    assert_eq!(cries[0].volume, 1.0);
    assert!(hear(&mut app).is_empty());
}

#[test]
fn a_cow_lows_now_and_then_and_never_twice_in_a_row() {
    let mut app = listening_app();
    summon(
        &mut app,
        Mob::new(MobType::Cow, 11),
        Vec3::new(8.5, 5.0, 8.5),
    );
    let mut lowed = Vec::new();
    for tick in 0..2400 {
        run_ticks(&mut app, 1);
        for sound in app.world_mut().resource_mut::<Sounds>().drain() {
            if sound.name == "mob.cow" {
                assert_eq!(sound.volume, 0.4);
                lowed.push(tick);
            }
        }
    }
    // The odds climb by one in a thousand a tick, so the first comes soon,
    // and each is followed by the animals' six seconds of quiet.
    assert!(lowed.len() >= 5, "{lowed:?}");
    assert!(lowed[0] < 400, "{lowed:?}");
    assert!(
        lowed.windows(2).all(|pair| pair[1] - pair[0] > 120),
        "{lowed:?}"
    );
}

#[test]
fn a_world_with_nobody_listening_rolls_for_no_voices() {
    // Without `Sounds` the same cow lives the same life: the idle roll is
    // not drawn from the mob's own `Random`.
    let walk = |listening: bool| {
        let mut app = creature_app(field(4), Vec3::new(40.5, 5.0, 40.5));
        if listening {
            app.init_resource::<Sounds>();
        }
        let cow = summon(
            &mut app,
            Mob::new(MobType::Cow, 11),
            Vec3::new(8.5, 5.0, 8.5),
        );
        for _ in 0..600 {
            run_ticks(&mut app, 1);
            if listening {
                app.world_mut().resource_mut::<Sounds>().clear();
            }
        }
        app.world().get::<Transform>(cow).unwrap().translation
    };
    assert_eq!(walk(true), walk(false));
}

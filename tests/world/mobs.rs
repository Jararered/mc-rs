use bevy::prelude::*;
use game::app::settings::Difficulty;
use game::entity::EntitySize;
use game::entity::StepDistance;
use game::entity::creature::Bounce;
use game::entity::creature::Fuse;
use game::entity::creature::Hover;
use game::entity::creature::Living;
use game::entity::creature::Swim;
use game::entity::creature::Wings;
use game::entity::explosion::PrimedTnt;
use game::entity::explosion::prime_tnt;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::entity::mobs::SpawnCategory;
use game::entity::mobs::spawn;
use game::entity::mobs::spawn_table;
use game::player::Player;
use game::player::PlayerHealth;
use game::world::biome::Biome;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::plugin::WorldPlugin;
use game::world::tick::WorldTick;

#[test]
fn only_overworld_creatures_appear_in_natural_tables() {
    for biome in [Biome::Forest, Biome::Taiga, Biome::Plains, Biome::Desert] {
        for category in [
            SpawnCategory::Monster,
            SpawnCategory::Creature,
            SpawnCategory::Water,
        ] {
            let list = spawn_table(biome, category);
            assert!(!list.is_empty());
            assert!(
                !list
                    .iter()
                    .any(|(kind, _)| matches!(kind, MobType::Ghast | MobType::PigZombie))
            );
        }
    }
    assert!(
        spawn_table(Biome::Forest, SpawnCategory::Creature)
            .iter()
            .any(|(mob, _)| *mob == MobType::Wolf)
    );
    assert!(
        !spawn_table(Biome::Desert, SpawnCategory::Creature)
            .iter()
            .any(|(mob, _)| *mob == MobType::Wolf)
    );
}

#[test]
fn difficulty_scales_hostile_damage() {
    assert_eq!(Difficulty::Peaceful.mob_damage(5), 0);
    assert_eq!(Difficulty::Easy.mob_damage(5), 2);
    assert_eq!(Difficulty::Normal.mob_damage(5), 5);
    assert_eq!(Difficulty::Hard.mob_damage(5), 7);
}

#[test]
fn beta_health_values_distinguish_passive_and_hostile_mobs() {
    assert_eq!(MobType::Sheep.health(0), 10);
    assert_eq!(MobType::Ghast.health(0), 10);
    assert_eq!(MobType::Chicken.health(0), 4);
    assert_eq!(MobType::Wolf.health(0), 8);
    assert_eq!(MobType::Zombie.health(0), 20);
    assert_eq!(MobType::Slime.health(4), 16);
}

fn mob_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
    app.world_mut().resource_mut::<WorldChunks>().insert(
        ChunkPosition::ZERO,
        super::block_ticks::generated(Chunk::new(), Biome::Plains),
    );
    app.world_mut().spawn((
        Player,
        PlayerHealth::default(),
        Transform::from_xyz(6., 12., 6.),
    ));
    app
}

#[test]
fn mob_roots_support_inherited_visibility_for_rendered_children() {
    let mut app = mob_app();
    for kind in MobType::ALL {
        let entity = spawn(
            &mut app.world_mut().commands(),
            Mob::new(kind, 42),
            Vec3::new(7., 10., 7.),
        );
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Inherited)
        );
        assert!(app.world().get::<InheritedVisibility>(entity).is_some());
    }
}

#[test]
fn large_slimes_split_into_four_smaller_slimes_on_death() {
    let mut app = mob_app();
    let mut slime = Mob::new(MobType::Slime, 99);
    slime.variant = 4;
    slime.health = 0;
    spawn(
        &mut app.world_mut().commands(),
        slime,
        Vec3::new(7., 10., 7.),
    );
    // The body tips over for 20 ticks before it splits.
    for _ in 0..21 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.world_mut().run_schedule(Update);
    }
    let sizes: Vec<_> = app
        .world_mut()
        .query::<&Mob>()
        .iter(app.world())
        .filter(|mob| mob.kind == MobType::Slime)
        .map(|mob| mob.variant)
        .collect();
    assert_eq!(sizes, vec![2; 4]);
}

#[test]
fn primed_tnt_damages_nearby_players_and_removes_weak_blocks() {
    let mut app = mob_app();
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .get_mut(ChunkPosition::ZERO)
        .unwrap()
        .chunk
        .set(7, 12, 6, game::block::blocks::Block::Dirt);
    prime_tnt(&mut app.world_mut().commands(), Vec3::new(6., 12., 6.), 1);
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.world_mut().run_schedule(Update);
    let health = app
        .world_mut()
        .query::<&PlayerHealth>()
        .single(app.world())
        .unwrap()
        .current;
    assert!(health < 20);
    assert_eq!(
        app.world().resource::<WorldChunks>().block_at(7, 12, 6),
        Some(game::block::blocks::Block::Air)
    );
    assert_eq!(
        app.world_mut()
            .query::<&PrimedTnt>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn fire_contact_hurts_once_per_invulnerability_window() {
    let mut app = mob_app();
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let chunk = &mut chunks.get_mut(ChunkPosition::ZERO).unwrap().chunk;
        // A one-block pit with fire at the bottom.
        for x in 6..=8 {
            for z in 6..=8 {
                chunk.set(x, 9, z, game::block::blocks::Block::Stone);
                for y in 10..=12 {
                    chunk.set(x, y, z, game::block::blocks::Block::Stone);
                }
            }
        }
        for y in 10..=12 {
            chunk.set(7, y, 7, game::block::blocks::Block::Air);
        }
        chunk.set(7, 10, 7, game::block::blocks::Block::Fire);
    }
    spawn(
        &mut app.world_mut().commands(),
        Mob::new(MobType::Pig, 42),
        Vec3::new(7.5, 10., 7.5),
    );
    for _ in 0..20 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.world_mut().run_schedule(Update);
    }
    let (mob, living) = app
        .world_mut()
        .query::<(&Mob, &Living)>()
        .single(app.world())
        .unwrap();
    // Touching fire deals 1 a tick, but after each hit the pig is
    // invulnerable for 10 ticks: hits land on ticks 1 and 12.
    assert_eq!(mob.health, 8);
    assert!(mob.fire_ticks > 0);
    assert!(living.invulnerable());
}

/// A grass field with the player standing at `player_feet`.
pub fn creature_app(chunks: WorldChunks, player_feet: Vec3) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugins(WorldPlugin);
    *app.world_mut().resource_mut::<WorldChunks>() = chunks;
    app.world_mut().spawn((
        Player,
        PlayerHealth::default(),
        Transform::from_translation(player_feet + Vec3::Y * EntitySize::PLAYER.y_offset),
    ));
    app
}

pub fn run_ticks(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.world_mut().run_schedule(Update);
    }
}

pub fn summon(app: &mut App, mob: Mob, feet: Vec3) -> Entity {
    let entity = spawn(&mut app.world_mut().commands(), mob, feet);
    app.world_mut().flush();
    entity
}

pub fn feet_of(app: &App, entity: Entity) -> Vec3 {
    app.world().get::<Transform>(entity).unwrap().translation
}

#[test]
fn every_mob_runs_beta_living_with_its_own_parts() {
    let mut app = creature_app(super::pathfinding::field(4), Vec3::new(0.5, 5.0, 8.5));
    for kind in MobType::ALL {
        let entity = summon(&mut app, Mob::new(kind, 7), Vec3::new(0.5, 5.0, 0.5));
        let world = app.world();
        assert!(world.get::<Living>(entity).is_some(), "{kind:?}");
        assert_eq!(
            world.get::<Wings>(entity).is_some(),
            kind == MobType::Chicken
        );
        assert_eq!(world.get::<Swim>(entity).is_some(), kind == MobType::Squid);
        assert_eq!(
            world.get::<Fuse>(entity).is_some(),
            kind == MobType::Creeper
        );
        assert_eq!(
            world.get::<Bounce>(entity).is_some(),
            kind == MobType::Slime
        );
        assert_eq!(world.get::<Hover>(entity).is_some(), kind == MobType::Ghast);
        // `canTriggerWalking` keeps wolves and spiders off farmland.
        assert_eq!(
            world.get::<StepDistance>(entity).is_some(),
            !matches!(kind, MobType::Wolf | MobType::Spider)
        );
    }
}

#[test]
fn pigs_wander_along_paths_over_grass() {
    let mut app = creature_app(super::pathfinding::field(4), Vec3::new(0.5, 5.0, 12.5));
    let start = Vec3::new(0.5, 5.0, 0.5);
    let pig = summon(&mut app, Mob::new(MobType::Pig, 42), start);
    let mut walked_a_path = false;
    let mut swung_legs = false;
    for _ in 0..600 {
        run_ticks(&mut app, 1);
        let living = app.world().get::<Living>(pig).expect("a nearby pig stays");
        walked_a_path |= living.path().is_some();
        swung_legs |= living.limb_amount > 0.2;
        let feet = feet_of(&app, pig);
        assert!((feet.y - 5.0).abs() < 1e-3, "pig left the ground at {feet}");
    }
    assert!(walked_a_path);
    assert!(swung_legs);
    let moved = feet_of(&app, pig) - start;
    assert!(moved.x.hypot(moved.z) > 1.0, "pig stayed put: {moved}");
}

#[test]
fn chickens_flap_and_fall_slowly() {
    let mut app = creature_app(super::pathfinding::field(4), Vec3::new(0.5, 5.0, 12.5));
    let chicken = summon(
        &mut app,
        Mob::new(MobType::Chicken, 3),
        Vec3::new(0.5, 30.0, 0.5),
    );
    let pig = summon(
        &mut app,
        Mob::new(MobType::Pig, 3),
        Vec3::new(4.5, 30.0, 0.5),
    );
    run_ticks(&mut app, 20);
    let chicken_drop = 30.0 - feet_of(&app, chicken).y;
    let pig_drop = 30.0 - feet_of(&app, pig).y;
    assert!(
        chicken_drop < 3.0 && pig_drop > 10.0,
        "{chicken_drop} vs {pig_drop}"
    );
    let wings = app.world().get::<Wings>(chicken).unwrap();
    assert_eq!(wings.flap_speed, 1.0);
    assert!(wings.angle(0.0) > 0.0);

    run_ticks(&mut app, 400);
    assert!((feet_of(&app, chicken).y - 5.0).abs() < 1e-3);
    assert_eq!(app.world().get::<Wings>(chicken).unwrap().flap_speed, 0.0);
}

#[test]
fn squid_swim_in_pulses_and_stay_in_the_water() {
    let mut chunks = super::pathfinding::field(4);
    for x in -14..=14 {
        for z in -14..=14 {
            for y in 5..=14 {
                chunks.set_block(x, y, z, game::block::blocks::Block::Water);
            }
        }
    }
    let mut app = creature_app(chunks, Vec3::new(0.5, 15.0, 15.5));
    let start = Vec3::new(0.5, 9.0, 0.5);
    let squid = summon(&mut app, Mob::new(MobType::Squid, 11), start);
    let mut tentacles = Vec::new();
    for _ in 0..300 {
        run_ticks(&mut app, 1);
        tentacles.push(app.world().get::<Swim>(squid).unwrap().tentacle);
        let feet = feet_of(&app, squid);
        assert!(
            feet.y >= 5.0 && feet.y < 15.0,
            "squid left the water at {feet}"
        );
    }
    assert!(tentacles.iter().any(|angle| *angle > 0.5));
    assert!(tentacles.contains(&0.0));
    let moved = feet_of(&app, squid) - start;
    assert!(moved.length() > 1.0, "squid stayed put: {moved}");
}

#[test]
fn tamed_wolves_follow_their_owner_until_told_to_sit() {
    let owner = Vec3::new(10.5, 5.0, 0.5);
    let mut app = creature_app(super::pathfinding::field(4), owner);
    let mut tamed = Mob::new(MobType::Wolf, 5);
    tamed.tamed = true;
    let follower = summon(&mut app, tamed.clone(), Vec3::new(0.5, 5.0, 0.5));
    tamed.sitting = true;
    let sitter = summon(&mut app, tamed, Vec3::new(0.5, 5.0, -4.5));
    // Like Beta, a tamed wolf still takes the odd wander path near its
    // owner, and paths back once it strays more than five blocks.
    let mut closest = f32::INFINITY;
    for tick in 0..400 {
        run_ticks(&mut app, 1);
        let distance = feet_of(&app, follower).distance(owner);
        closest = closest.min(distance);
        if tick > 200 {
            assert!(distance < 10.0, "the wolf wandered off to {distance}");
        }
    }
    assert!(closest < 3.0, "the wolf never reached its owner: {closest}");
    let sat = feet_of(&app, sitter);
    assert!(
        (sat.x - 0.5).abs() < 1e-3 && (sat.z + 4.5).abs() < 1e-3,
        "{sat}"
    );
}

#[test]
fn creatures_far_from_the_player_despawn_but_tamed_wolves_stay() {
    let mut app = creature_app(super::pathfinding::field(4), Vec3::new(140.5, 5.0, 0.5));
    let pig = summon(
        &mut app,
        Mob::new(MobType::Pig, 1),
        Vec3::new(0.5, 5.0, 0.5),
    );
    let mut wolf = Mob::new(MobType::Wolf, 1);
    wolf.tamed = true;
    wolf.sitting = true;
    let wolf = summon(&mut app, wolf, Vec3::new(4.5, 5.0, 0.5));
    run_ticks(&mut app, 40);
    assert!(app.world().get_entity(pig).is_err());
    assert!(app.world().get_entity(wolf).is_ok());
}

#[test]
fn the_player_shoves_creatures_aside() {
    let mut app = creature_app(super::pathfinding::field(4), Vec3::new(0.8, 5.0, 0.5));
    let mut sitter = Mob::new(MobType::Wolf, 2);
    sitter.tamed = true;
    sitter.sitting = true;
    let wolf = summon(&mut app, sitter, Vec3::new(0.5, 5.0, 0.5));
    run_ticks(&mut app, 10);
    assert!(feet_of(&app, wolf).x < 0.2, "{}", feet_of(&app, wolf));
}

use bevy::prelude::*;
use game::app::settings::Difficulty;
use game::entity::EntitySize;
use game::entity::mobs::Mob;
use game::entity::mobs::MobKind;
use game::entity::mobs::PrimedTnt;
use game::entity::mobs::SpawnCategory;
use game::entity::mobs::prime_tnt;
use game::entity::mobs::ray_hit;
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
                    .any(|(kind, _)| matches!(kind, MobKind::Ghast | MobKind::PigZombie))
            );
        }
    }
    assert!(
        spawn_table(Biome::Forest, SpawnCategory::Creature)
            .iter()
            .any(|(mob, _)| *mob == MobKind::Wolf)
    );
    assert!(
        !spawn_table(Biome::Desert, SpawnCategory::Creature)
            .iter()
            .any(|(mob, _)| *mob == MobKind::Wolf)
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
    assert_eq!(MobKind::Sheep.health(0), 10);
    assert_eq!(MobKind::Ghast.health(0), 10);
    assert_eq!(MobKind::Chicken.health(0), 4);
    assert_eq!(MobKind::Wolf.health(0), 8);
    assert_eq!(MobKind::Zombie.health(0), 20);
    assert_eq!(MobKind::Slime.health(4), 16);
}

#[test]
fn mob_ray_reaches_boxes_without_targeting_beyond_reach() {
    let origin = Vec3::new(0.5, 1.6, 0.5);
    let size = EntitySize::PLAYER;
    let feet = Vec3::new(0.5, 0.0, -2.5);
    assert!(
        (ray_hit(
            origin,
            Vec3::NEG_Z,
            feet,
            EntitySize {
                y_offset: 0.0,
                ..size
            },
            3.0
        )
        .unwrap()
            - 2.7)
            .abs()
            < 0.01
    );
    assert!(
        ray_hit(
            origin,
            Vec3::NEG_Z,
            feet,
            EntitySize {
                y_offset: 0.0,
                ..size
            },
            2.0
        )
        .is_none()
    );
    assert!(
        ray_hit(
            origin,
            Vec3::X,
            feet,
            EntitySize {
                y_offset: 0.0,
                ..size
            },
            3.0
        )
        .is_none()
    );
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
    for kind in MobKind::ALL {
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
    let mut slime = Mob::new(MobKind::Slime, 99);
    slime.variant = 4;
    slime.health = 0;
    spawn(
        &mut app.world_mut().commands(),
        slime,
        Vec3::new(7., 10., 7.),
    );
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.world_mut().run_schedule(Update);
    let sizes: Vec<_> = app
        .world_mut()
        .query::<&Mob>()
        .iter(app.world())
        .filter(|mob| mob.kind == MobKind::Slime)
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
        .set(7, 12, 6, game::block::id::Id::Dirt);
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
        Some(game::block::id::Id::Air)
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
fn standing_in_fire_keeps_hurting_mobs_after_reignition() {
    let mut app = mob_app();
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .get_mut(ChunkPosition::ZERO)
        .unwrap()
        .chunk
        .set(7, 10, 7, game::block::id::Id::Fire);
    spawn(
        &mut app.world_mut().commands(),
        Mob::new(MobKind::Zombie, 42),
        Vec3::new(7., 10., 7.),
    );
    for _ in 0..20 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.world_mut().run_schedule(Update);
    }
    let health = app
        .world_mut()
        .query::<&Mob>()
        .single(app.world())
        .unwrap()
        .health;
    assert_eq!(health, 19);
}

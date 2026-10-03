//! Beta damage rules, monsters attacking the player, projectiles, and blasts.

use bevy::prelude::*;
use game::app::settings::Difficulty;
use game::block::id::Id;
use game::entity::DroppedItem;
use game::entity::Velocity;
use game::entity::combat::HURT_TICKS;
use game::entity::combat::Hit;
use game::entity::combat::PlayerCombat;
use game::entity::combat::Source;
use game::entity::combat::Victim;
use game::entity::combat::armor_value;
use game::entity::combat::bordered;
use game::entity::combat::hurt_creature;
use game::entity::combat::hurt_player;
use game::entity::combat::pick;
use game::entity::creature::Living;
use game::entity::explosion::blast_cells;
use game::entity::explosion::exposure;
use game::entity::mobs::Mob;
use game::entity::mobs::MobKind;
use game::entity::projectiles::Arrow;
use game::entity::projectiles::Fireball;
use game::entity::projectiles::spawn_arrow;
use game::item::ItemId;
use game::item::ItemStack;
use game::item::tools::damage_vs_entity;
use game::item::tools::hit_durability;
use game::physics::Aabb;
use game::player::PlayerHealth;
use game::random::ItemRng;
use game::random::JavaRandom;

use super::mobs::creature_app;
use super::mobs::feet_of;
use super::mobs::run_ticks;
use super::mobs::summon;
use super::pathfinding::field;

fn player_health(app: &mut App) -> u8 {
    app.world_mut()
        .query::<&PlayerHealth>()
        .single(app.world())
        .unwrap()
        .current
}

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut().query::<&T>().iter(app.world()).count()
}

#[test]
fn a_second_hit_lands_only_for_what_exceeds_the_first() {
    let mut mob = Mob::new(MobKind::Zombie, 1);
    let mut living = Living::facing(0.0);
    let mut velocity = Velocity::default();
    let hit = |amount| Hit {
        amount,
        from: Some(Vec3::new(-2.0, 0.0, 0.0)),
        source: Source::Player,
    };
    let first = hurt_creature(&mut mob, &mut living, &mut velocity, Vec3::ZERO, hit(4));
    assert!(first.landed && !first.died);
    assert_eq!(mob.health, 16);
    assert_eq!(living.hurt_time, HURT_TICKS);
    // Knocked away from the attacker and up.
    assert!(velocity.0.x > 0.0 && velocity.0.y > 0.0, "{}", velocity.0);
    // A zombie struck by the player turns on them.
    assert!(living.chasing());

    // Inside the window a blow no harder than the first is absorbed...
    assert!(!hurt_creature(&mut mob, &mut living, &mut velocity, Vec3::ZERO, hit(4)).landed);
    assert_eq!(mob.health, 16);
    // ...and a harder one lands only for the difference, without knockback.
    let before = velocity.0;
    assert!(hurt_creature(&mut mob, &mut living, &mut velocity, Vec3::ZERO, hit(7)).landed);
    assert_eq!(mob.health, 13);
    assert_eq!(velocity.0, before);
}

#[test]
fn wolves_halve_blows_from_anything_but_the_player() {
    let mut wolf = Mob::new(MobKind::Wolf, 2);
    wolf.sitting = true;
    let mut living = Living::facing(0.0);
    let mut velocity = Velocity::default();
    let bite = Hit {
        amount: 5,
        from: None,
        source: Source::Creature,
    };
    hurt_creature(&mut wolf, &mut living, &mut velocity, Vec3::ZERO, bite);
    assert_eq!(wolf.health, 8 - 3);
    assert!(!wolf.sitting);
}

#[test]
fn difficulty_and_armor_soften_monster_blows() {
    let mut health = PlayerHealth::default();
    let mut combat = PlayerCombat::default();
    let mut velocity = Velocity::default();
    let mut armor = [None; 4];
    let mut rng = ItemRng::default();
    let monster = Hit {
        amount: 5,
        from: Some(Vec3::new(1.0, 1.62, 0.0)),
        source: Source::Monster,
    };
    let mut strike = |health: &mut PlayerHealth,
                      combat: &mut PlayerCombat,
                      armor: &mut [Option<ItemStack>; 4],
                      difficulty| {
        *combat = PlayerCombat::default();
        let mut victim = Victim {
            health,
            combat,
            velocity: &mut velocity,
            armor,
            eye: Vec3::new(0.0, 1.62, 0.0),
            yaw: 0.0,
        };
        hurt_player(&mut victim, monster, difficulty, &mut rng)
    };
    assert!(!strike(
        &mut health,
        &mut combat,
        &mut armor,
        Difficulty::Peaceful
    ));
    assert_eq!(health.current, 20);
    assert!(strike(
        &mut health,
        &mut combat,
        &mut armor,
        Difficulty::Easy
    ));
    assert_eq!(health.current, 18);
    assert_eq!(combat.hurt_time, HURT_TICKS);

    // Unworn iron armor is worth 20 of 25: a 5 point blow takes 1.
    armor = [
        ItemId::IronHelmet,
        ItemId::IronChestplate,
        ItemId::IronLeggings,
        ItemId::IronBoots,
    ]
    .map(|item| Some(ItemStack::new(item, 1).unwrap()));
    assert_eq!(armor_value(&armor), 20);
    assert!(strike(
        &mut health,
        &mut combat,
        &mut armor,
        Difficulty::Normal
    ));
    assert_eq!(health.current, 17);
    assert!(armor.iter().flatten().all(|piece| piece.data() == 5));
}

#[test]
fn weapons_deal_beta_damage_and_wear() {
    let stack = |item| Some(ItemStack::new(item, 1).unwrap());
    assert_eq!(damage_vs_entity(None), 1);
    assert_eq!(damage_vs_entity(stack(ItemId::WoodenSword)), 4);
    assert_eq!(damage_vs_entity(stack(ItemId::StoneSword)), 6);
    assert_eq!(damage_vs_entity(stack(ItemId::IronSword)), 8);
    assert_eq!(damage_vs_entity(stack(ItemId::DiamondSword)), 10);
    assert_eq!(damage_vs_entity(stack(ItemId::GoldSword)), 4);
    assert_eq!(damage_vs_entity(stack(ItemId::DiamondAxe)), 6);
    assert_eq!(damage_vs_entity(stack(ItemId::IronPickaxe)), 4);
    assert_eq!(damage_vs_entity(stack(ItemId::StoneShovel)), 2);
    assert_eq!(damage_vs_entity(stack(ItemId::IronHoe)), 1);
    assert_eq!(damage_vs_entity(stack(ItemId::Stick)), 1);
    assert_eq!(hit_durability(stack(ItemId::IronSword).unwrap()), 1);
    assert_eq!(hit_durability(stack(ItemId::IronPickaxe).unwrap()), 2);
    assert_eq!(hit_durability(stack(ItemId::IronHoe).unwrap()), 0);
}

#[test]
fn the_crosshair_picks_the_nearest_box_in_reach() {
    let cube = |z: f32| Aabb::new(Vec3::new(-0.3, -0.3, z - 0.3), Vec3::new(0.3, 0.3, z + 0.3));
    let look = Vec3::NEG_Z;
    let all = [
        ("near", cube(-1.5)),
        ("behind", cube(-2.4)),
        ("far", cube(-3.8)),
    ];
    assert_eq!(pick(Vec3::ZERO, look, 3.0, all), Some("near"));
    assert_eq!(
        pick(Vec3::ZERO, look, 3.0, [all[1], all[2]]),
        Some("behind")
    );
    assert_eq!(pick(Vec3::ZERO, look, 3.0, [all[2]]), None);
    // A block in view cuts the reach short.
    assert_eq!(pick(Vec3::ZERO, look, 1.0, all), None);
    // A box just beside the ray is hit only with its collision border.
    let beside = Aabb::new(Vec3::new(0.05, -0.3, -2.0), Vec3::new(0.6, 0.3, -1.4));
    assert_eq!(pick(Vec3::ZERO, look, 3.0, [((), beside)]), None);
    assert_eq!(
        pick(Vec3::ZERO, look, 3.0, [((), bordered(beside, 0.1))]),
        Some(())
    );
}

#[test]
fn blasts_eat_through_soil_but_stop_at_obsidian() {
    let mut rng = JavaRandom::new(5);
    let center = Vec3::new(0.5, 5.5, 0.5);
    let soil = field(4);
    let solid = |chunks: &game::world::chunk::WorldChunks, cells: &[IVec3]| {
        cells
            .iter()
            .filter(|cell| chunks.block_at(cell.x, cell.y, cell.z) != Some(Id::Air))
            .count()
    };
    let cells = blast_cells(&soil, center, 3.0, &mut rng);
    assert!(solid(&soil, &cells) > 10);

    let mut sealed = field(4);
    for x in -4..=4 {
        for z in -4..=4 {
            for y in 3..=8 {
                if x == -4 || x == 4 || z == -4 || z == 4 || y == 3 || y == 8 {
                    sealed.set_block(x, y, z, Id::Obsidian);
                }
            }
            if x.abs() < 4 && z.abs() < 4 {
                sealed.set_block(x, 4, z, Id::Air);
            }
        }
    }
    let cells = blast_cells(&sealed, center, 3.0, &mut rng);
    assert_eq!(solid(&sealed, &cells), 0);
    assert!(
        cells
            .iter()
            .all(|cell| cell.x.abs() < 4 && cell.z.abs() < 4)
    );
}

#[test]
fn walls_shield_bodies_from_a_blast() {
    let mut chunks = field(4);
    let center = Vec3::new(0.5, 5.5, 0.5);
    let body = Aabb::new(Vec3::new(3.2, 5.0, 0.2), Vec3::new(3.8, 6.8, 0.8));
    assert_eq!(exposure(&chunks, center, body), 1.0);
    for y in 5..=8 {
        for z in -2..=2 {
            chunks.set_block(2, y, z, Id::Stone);
        }
    }
    assert_eq!(exposure(&chunks, center, body), 0.0);
}

#[test]
fn zombies_chase_and_strike_the_player() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    let zombie = summon(
        &mut app,
        Mob::new(MobKind::Zombie, 3),
        Vec3::new(0.5, 5.0, 1.5),
    );
    run_ticks(&mut app, 200);
    assert!(app.world().get::<Living>(zombie).unwrap().chasing());
    assert!(player_health(&mut app) < 20);
}

#[test]
fn creepers_hiss_and_blow_a_crater_beside_the_player() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    summon(
        &mut app,
        Mob::new(MobKind::Creeper, 4),
        Vec3::new(0.5, 5.0, 6.5),
    );
    run_ticks(&mut app, 80);
    let creepers = app
        .world_mut()
        .query::<&Mob>()
        .iter(app.world())
        .filter(|mob| mob.kind == MobKind::Creeper)
        .count();
    assert_eq!(creepers, 0, "the creeper should have exploded");
    assert!(player_health(&mut app) < 20);
    let chunks = app.world().resource::<game::world::chunk::WorldChunks>();
    let cratered = (-2..=3).any(|x| (4..=9).any(|z| chunks.block_at(x, 4, z) == Some(Id::Air)));
    assert!(cratered);
}

#[test]
fn skeletons_shoot_arrows_at_the_player() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    summon(
        &mut app,
        Mob::new(MobKind::Skeleton, 6),
        Vec3::new(0.5, 5.0, 2.5),
    );
    let mut shot = false;
    for _ in 0..300 {
        run_ticks(&mut app, 1);
        shot |= count::<Arrow>(&mut app) > 0;
    }
    assert!(shot);
    assert!(player_health(&mut app) < 20);
}

#[test]
fn big_slimes_hurt_on_contact() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    let mut slime = Mob::new(MobKind::Slime, 9);
    slime.variant = 4;
    slime.health = MobKind::Slime.health(4);
    summon(&mut app, slime, Vec3::new(0.5, 5.0, 7.0));
    run_ticks(&mut app, 3);
    assert_eq!(player_health(&mut app), 16);
}

#[test]
fn a_fatal_fall_drops_loot_then_the_body_vanishes() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    let sheep = summon(
        &mut app,
        Mob::new(MobKind::Sheep, 11),
        Vec3::new(0.5, 40.0, 0.5),
    );
    let mut dying = 0;
    while app.world().get_entity(sheep).is_ok() {
        run_ticks(&mut app, 1);
        if let Some(mob) = app.world().get::<Mob>(sheep)
            && mob.health <= 0
        {
            // `dropFewItems` runs the moment health runs out: one fleece.
            assert_eq!(count::<DroppedItem>(&mut app), 1);
            dying += 1;
        }
        assert!(dying <= 21, "the body lingered");
    }
    // The killing tick, then the 20-tick death animation.
    assert_eq!(dying, 21);
}

#[test]
fn arrows_stick_in_walls() {
    let mut chunks = field(4);
    for y in 5..=8 {
        for z in -3..=3 {
            chunks.set_block(5, y, z, Id::Stone);
        }
    }
    let mut app = creature_app(chunks, Vec3::new(-6.5, 5.0, 8.5));
    let arrow = spawn_arrow(
        &mut app.world_mut().commands(),
        Vec3::new(0.5, 6.0, 0.5),
        Vec3::X,
        1.5,
        0.0,
        None,
        &mut JavaRandom::new(1),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 40);
    let lodged = feet_of(&app, arrow);
    run_ticks(&mut app, 20);
    assert_eq!(feet_of(&app, arrow), lodged);
    assert!(lodged.x > 4.5 && lodged.x < 5.0, "{lodged}");
}

#[test]
fn a_struck_fireball_flies_the_way_the_player_looks() {
    let mut fireball = fireball_template();
    assert!(fireball.acceleration.y < 0.0);
    fireball.deflect(Vec3::Z);
    assert_eq!(fireball.motion, Vec3::Z);
    assert_eq!(fireball.acceleration, Vec3::Z * 0.1);
}

fn fireball_template() -> Fireball {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 8.5));
    let entity = game::entity::projectiles::spawn_fireball(
        &mut app.world_mut().commands(),
        Vec3::new(0.5, 20.0, 0.5),
        Vec3::NEG_Y,
        None,
        &mut JavaRandom::new(1),
    );
    app.world_mut().flush();
    app.world().get::<Fireball>(entity).unwrap().clone()
}

#[test]
fn fireballs_explode_on_impact() {
    let mut app = creature_app(field(4), Vec3::new(30.5, 5.0, 30.5));
    let entity = game::entity::projectiles::spawn_fireball(
        &mut app.world_mut().commands(),
        Vec3::new(0.5, 9.0, 0.5),
        Vec3::NEG_Y,
        None,
        &mut JavaRandom::new(3),
    );
    app.world_mut().flush();
    run_ticks(&mut app, 60);
    assert!(app.world().get_entity(entity).is_err());
}

#[test]
fn slimes_hop_toward_a_nearby_player() {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 12.5));
    let mut slime = Mob::new(MobKind::Slime, 21);
    slime.variant = 2;
    slime.health = MobKind::Slime.health(2);
    let start = Vec3::new(0.5, 5.0, 0.5);
    let entity = summon(&mut app, slime, start);
    let mut airborne = false;
    for _ in 0..80 {
        run_ticks(&mut app, 1);
        airborne |= feet_of(&app, entity).y > 5.2;
    }
    assert!(airborne, "the slime never hopped");
    assert!(feet_of(&app, entity).distance(start) > 1.0);
}

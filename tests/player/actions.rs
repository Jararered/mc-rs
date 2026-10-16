//! `PlayerAction`s applied headless: what a player's hands do to the world,
//! for a player nobody is holding a mouse for.

use bevy::prelude::*;
use game::app::settings::GameSettings;
use game::block::blocks::Block;
use game::entity::DroppedItem;
use game::entity::creature::Living;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;
use game::item::ItemStack;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::Player;
use game::player::PlayerHealth;
use game::player::actions::Action;
use game::player::actions::PlayerAction;
use game::player::actions::PlayerActionsPlugin;
use game::player::actions::Pointed;
use game::player::actions::Window;
use game::player::actions::WindowOpen;
use game::world::chunk::WorldChunks;

use crate::entity::mobs::creature_app;
use crate::entity::mobs::run_ticks;
use crate::entity::mobs::second_player;
use crate::entity::mobs::summon;
use crate::entity::pathfinding::field;

/// Two players on a grass field at `y = 4`, each with stone in hand. Returns
/// the one standing at the origin and the one eight blocks east.
fn two_players() -> (App, Entity, Entity) {
    let mut app = creature_app(field(4), Vec3::new(0.5, 5.0, 0.5));
    app.add_plugins(PlayerActionsPlugin);
    let first = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    let second = second_player(&mut app, Vec3::new(8.5, 5.0, 0.5));
    for player in [first, second] {
        let mut hotbar = Hotbar::default();
        hotbar.slots[0] = ItemStack::from_block(Block::Stone, 2).ok();
        app.world_mut()
            .entity_mut(player)
            .insert((hotbar, Inventory::default()));
    }
    (app, first, second)
}

fn act(app: &mut App, player: Entity, action: Action) {
    app.world_mut()
        .resource_mut::<Messages<PlayerAction>>()
        .write(PlayerAction { player, action });
    run_ticks(app, 1);
}

fn top_of(app: &App, x: i32, y: i32, z: i32) -> BlockHit {
    BlockHit {
        x,
        y,
        z,
        face: BlockFace::Up,
        block: block(app, x, y, z),
    }
}

fn block(app: &App, x: i32, y: i32, z: i32) -> Block {
    app.world()
        .resource::<WorldChunks>()
        .block_at(x, y, z)
        .unwrap()
}

fn held(app: &App, player: Entity) -> u8 {
    app.world().get::<Hotbar>(player).unwrap().slots[0].map_or(0, |stack| stack.count())
}

fn dropped(app: &mut App) -> usize {
    app.world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .count()
}

#[test]
fn punching_each_face_extinguishes_only_the_adjacent_fire() {
    use game::world::block_ticks::AuxEffect;

    for face in [
        BlockFace::Down,
        BlockFace::Up,
        BlockFace::North,
        BlockFace::South,
        BlockFace::West,
        BlockFace::East,
    ] {
        let (mut app, player, _) = two_players();
        app.world_mut().get_mut::<Hotbar>(player).unwrap().slots[0] =
            ItemStack::with_data(Item::FlintAndSteel, 1, 12).ok();
        let hit = BlockHit {
            x: 2,
            y: 8,
            z: 2,
            face,
            block: Block::Stone,
        };
        let (x, y, z) = face.neighbor(hit.x, hit.y, hit.z);
        {
            let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
            chunks.set_block(hit.x, hit.y, hit.z, hit.block);
            chunks.set_block_with_metadata(x, y, z, Block::Fire, 7);
        }
        let (dx, dy, dz) = face.offset();
        let outward = Vec3::new(dx as f32, dy as f32, dz as f32);
        let center = Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + Vec3::splat(0.5);
        let pointed = game::physics::raycast_blocks(
            app.world().resource::<WorldChunks>(),
            center + outward * 2.0,
            -outward,
            5.0,
        )
        .expect("the ray passes through fire to its supporting face");
        assert_eq!(
            (pointed.x, pointed.y, pointed.z, pointed.face),
            (hit.x, hit.y, hit.z, face)
        );
        let before = app.world().get::<Hotbar>(player).unwrap().slots;
        act(&mut app, player, Action::Mine { hit });
        assert_eq!(block(&app, x, y, z), Block::Air, "{face:?}");
        assert_eq!(block(&app, hit.x, hit.y, hit.z), Block::Stone);
        assert_eq!(app.world().get::<Hotbar>(player).unwrap().slots, before);
        assert_eq!(dropped(&mut app), 0);
        let effects: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<AuxEffect>>()
            .drain()
            .collect();
        assert_eq!(
            effects,
            [AuxEffect {
                position: IVec3::new(x, y, z),
                effect: 1004,
                data: 0,
            }]
        );

        // Clicking again, or against another block, must not hiss.
        act(&mut app, player, Action::Mine { hit });
        app.world_mut()
            .resource_mut::<WorldChunks>()
            .set_block(x, y, z, Block::Stone);
        act(&mut app, player, Action::Mine { hit });
        assert_eq!(block(&app, x, y, z), Block::Stone);
        assert!(app.world().resource::<Messages<AuxEffect>>().is_empty());
    }
}

#[test]
fn a_player_digs_and_builds_with_their_own_hands() {
    let (mut app, first, second) = two_players();
    let hit = top_of(&app, 9, 4, 0);
    act(&mut app, second, Action::Mine { hit });
    act(&mut app, second, Action::Break { hit });
    assert_eq!(block(&app, 9, 4, 0), Block::Air);
    assert_eq!(dropped(&mut app), 1, "the grass block dropped its dirt");
    // A second claim on a block that is already gone breaks nothing else.
    act(&mut app, first, Action::Break { hit });
    assert_eq!(dropped(&mut app), 1);

    let hit = top_of(&app, 10, 4, 2);
    act(
        &mut app,
        second,
        Action::Use {
            hit: Some(hit),
            origin: Vec3::new(8.5, 6.62, 0.5),
            look: Vec3::NEG_Y,
            click: true,
        },
    );
    assert_eq!(block(&app, 10, 5, 2), Block::Stone);
    assert_eq!(held(&app, second), 1);
    assert_eq!(
        held(&app, first),
        2,
        "the other player's stack is untouched"
    );
}

#[test]
fn a_dropped_item_leaves_the_hand_of_the_player_who_dropped_it() {
    let (mut app, first, second) = two_players();
    act(&mut app, second, Action::DropItem);
    assert_eq!(held(&app, second), 1);
    assert_eq!(held(&app, first), 2);
    assert_eq!(dropped(&mut app), 1);
}

#[test]
fn a_struck_zombie_turns_on_the_player_who_hit_it() {
    let (mut app, first, second) = two_players();
    // Beside the first player, out of sight range of neither.
    let zombie = summon(
        &mut app,
        Mob::new(MobType::Zombie, 5),
        Vec3::new(2.5, 5.0, 0.5),
    );
    run_ticks(&mut app, 2);
    assert_eq!(
        app.world().get::<Living>(zombie).unwrap().target(),
        Some(first)
    );
    act(
        &mut app,
        second,
        Action::UseEntity {
            target: Pointed::Mob(zombie),
            attack: true,
            look: Vec3::NEG_X,
        },
    );
    assert!(app.world().get::<Mob>(zombie).unwrap().health < 20);
    assert_eq!(
        app.world().get::<Living>(zombie).unwrap().target(),
        Some(second)
    );
}

#[test]
fn zombie_pigmen_turn_on_the_player_who_struck_one_of_them() {
    let (mut app, first, second) = two_players();
    // Both beside the first player, who did nothing.
    let struck = summon(
        &mut app,
        Mob::new(MobType::PigZombie, 6),
        Vec3::new(2.5, 5.0, 0.5),
    );
    let bystander = summon(
        &mut app,
        Mob::new(MobType::PigZombie, 7),
        Vec3::new(1.5, 5.0, 2.5),
    );
    act(
        &mut app,
        second,
        Action::UseEntity {
            target: Pointed::Mob(struck),
            attack: true,
            look: Vec3::NEG_X,
        },
    );
    for pigman in [struck, bystander] {
        assert!(app.world().get::<Mob>(pigman).unwrap().angry);
        assert_eq!(
            app.world().get::<Living>(pigman).unwrap().target(),
            Some(second),
            "not the nearer {first}"
        );
    }
}

#[test]
fn a_tamed_wolf_joins_its_owner_s_attack() {
    let (mut app, _, second) = two_players();
    let cow = summon(
        &mut app,
        Mob::new(MobType::Cow, 8),
        Vec3::new(6.5, 5.0, 0.5),
    );
    let mut pet = Mob::new(MobType::Wolf, 9);
    pet.tamed = true;
    pet.owner = Some("Player".to_owned());
    let standing = summon(&mut app, pet.clone(), Vec3::new(10.5, 5.0, 0.5));
    pet.sitting = true;
    let sitting = summon(&mut app, pet, Vec3::new(10.5, 5.0, 2.5));
    act(
        &mut app,
        second,
        Action::UseEntity {
            target: Pointed::Mob(cow),
            attack: true,
            look: Vec3::NEG_X,
        },
    );
    assert_eq!(
        app.world().get::<Living>(standing).unwrap().target(),
        Some(cow)
    );
    // `alertWolves(target, true)` leaves a sitting wolf where it sits.
    assert_eq!(app.world().get::<Living>(sitting).unwrap().target(), None);
}

#[test]
fn a_chest_opens_for_the_player_who_clicked_it() {
    let (mut app, _, second) = two_players();
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(10, 5, 0, Block::Chest);
    let hit = top_of(&app, 10, 5, 0);
    act(
        &mut app,
        second,
        Action::Use {
            hit: Some(hit),
            origin: Vec3::new(8.5, 6.62, 0.5),
            look: Vec3::X,
            click: true,
        },
    );
    let opened: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<WindowOpen>>()
        .drain()
        .collect();
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].player, second);
    assert!(matches!(
        opened[0].window,
        Window::Chest { position, .. } if position == IVec3::new(10, 5, 0)
    ));
    // The stone in hand was not placed against it.
    assert_eq!(held(&app, second), 2);
}

fn use_block(hit: BlockHit) -> Action {
    Action::Use {
        hit: Some(hit),
        origin: Vec3::new(8.5, 6.62, 0.5),
        look: Vec3::X,
        click: true,
    }
}

fn health_of(app: &App, player: Entity) -> u8 {
    app.world().get::<PlayerHealth>(player).unwrap().current
}

fn set_health(app: &mut App, player: Entity, current: u8) {
    app.world_mut()
        .get_mut::<PlayerHealth>(player)
        .unwrap()
        .current = current;
}

#[test]
fn a_cake_is_placed_from_the_hand_and_eaten_in_slices() {
    let (mut app, _, player) = two_players();
    {
        let mut hotbar = app.world_mut().get_mut::<Hotbar>(player).unwrap();
        hotbar.slots[0] = ItemStack::new(Item::Cake, 1).ok();
    }
    let ground = top_of(&app, 10, 4, 2);
    act(&mut app, player, use_block(ground));
    assert_eq!(block(&app, 10, 5, 2), Block::Cake);
    assert_eq!(held(&app, player), 0);
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(10, 5, 2),
        0
    );

    // Full health: the click is consumed, so the stone in hand is not placed
    // on the cake, and neither click takes a bite.
    {
        let mut hotbar = app.world_mut().get_mut::<Hotbar>(player).unwrap();
        hotbar.slots[0] = ItemStack::from_block(Block::Stone, 2).ok();
    }
    set_health(&mut app, player, 20);
    let cake = top_of(&app, 10, 5, 2);
    act(&mut app, player, use_block(cake));
    act(&mut app, player, Action::Mine { hit: cake });
    assert_eq!(block(&app, 10, 5, 2), Block::Cake);
    assert_eq!(block(&app, 10, 6, 2), Block::Air);
    assert_eq!(held(&app, player), 2);
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(10, 5, 2),
        0
    );
    assert_eq!(health_of(&app, player), 20);

    set_health(&mut app, player, 10);
    act(&mut app, player, use_block(cake));
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(10, 5, 2),
        1
    );
    assert_eq!(health_of(&app, player), 13);
    assert_eq!(held(&app, player), 2);
    assert_eq!(block(&app, 10, 6, 2), Block::Air);

    // By default only a right-click eats: a punch digs and takes no bite.
    act(&mut app, player, Action::Mine { hit: cake });
    assert_eq!(block(&app, 10, 5, 2), Block::Cake);
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(10, 5, 2),
        1
    );
    assert_eq!(health_of(&app, player), 13);

    // With the feature off, a punch takes a bite as in Beta, and the cake is
    // still there to keep digging.
    app.insert_resource(GameSettings {
        cake_right_click_only: false,
        ..default()
    });
    act(&mut app, player, Action::Mine { hit: cake });
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(10, 5, 2),
        2
    );
    assert_eq!(health_of(&app, player), 16);

    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(10, 5, 2, 5);
    set_health(&mut app, player, 10);
    let last = top_of(&app, 10, 5, 2);
    act(&mut app, player, use_block(last));
    assert_eq!(block(&app, 10, 5, 2), Block::Air);
    assert_eq!(health_of(&app, player), 13);
    assert_eq!(dropped(&mut app), 0);

    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(10, 5, 2, Block::Cake);
    let broken = top_of(&app, 10, 5, 2);
    act(&mut app, player, Action::Break { hit: broken });
    assert_eq!(block(&app, 10, 5, 2), Block::Air);
    assert_eq!(dropped(&mut app), 0);

    // Taking the block under a cake removes it and still drops nothing.
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(4, 5, 4, Block::Cake);
    let support = top_of(&app, 4, 4, 4);
    act(&mut app, player, Action::Break { hit: support });
    assert_eq!(block(&app, 4, 4, 4), Block::Air);
    assert_eq!(block(&app, 4, 5, 4), Block::Air);
    // The grass block drops its dirt. The cake does not.
    assert_eq!(dropped(&mut app), 1);
}

#[test]
fn breaking_and_placing_a_block_sound_like_the_block() {
    use game::world::sound::Sounds;
    let (mut app, first, _) = two_players();
    app.init_resource::<Sounds>();
    let hit = top_of(&app, 1, 4, 0);
    act(&mut app, first, Action::Break { hit });
    assert_eq!(block(&app, 1, 4, 0), Block::Air);
    let sounds: Vec<_> = app.world_mut().resource_mut::<Sounds>().drain().collect();
    assert_eq!(sounds.len(), 1);
    // Aux effect 2001: the broken block's sound from the middle of its
    // cell, at `(volume + 1) / 2` and four fifths of its pitch.
    assert_eq!(sounds[0].name, "step.grass");
    assert_eq!(sounds[0].position, Some(Vec3::new(1.5, 4.5, 0.5)));
    assert_eq!(sounds[0].volume, 1.0);
    assert!((sounds[0].pitch - 0.8).abs() < 1e-6);

    // A claim on a block that has gone makes no sound.
    act(&mut app, first, Action::Break { hit });
    assert!(app.world().resource::<Sounds>().queued().is_empty());

    let hit = top_of(&app, 2, 4, 0);
    act(
        &mut app,
        first,
        Action::Use {
            hit: Some(hit),
            origin: Vec3::new(0.5, 6.62, 0.5),
            look: Vec3::NEG_Y,
            click: true,
        },
    );
    assert_eq!(block(&app, 2, 5, 0), Block::Stone);
    let sounds: Vec<_> = app.world_mut().resource_mut::<Sounds>().drain().collect();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name, "step.stone");
    assert_eq!(sounds[0].position, Some(Vec3::new(2.5, 5.5, 0.5)));
}

#[test]
fn a_disc_goes_into_a_jukebox_and_a_click_gets_it_back() {
    use game::item::Item;
    use game::world::block_ticks::AuxEffect;
    let (mut app, first, _) = two_players();
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(2, 5, 0, Block::Jukebox);
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = ItemStack::new(Item::RecordCat, 1).ok();
    app.world_mut().entity_mut(first).insert(hotbar);
    let click = |app: &mut App| {
        let hit = top_of(app, 2, 5, 0);
        act(
            app,
            first,
            Action::Use {
                hit: Some(hit),
                origin: Vec3::new(0.5, 6.62, 0.5),
                look: Vec3::X,
                click: true,
            },
        );
    };
    let effects = |app: &mut App| -> Vec<(i32, i32)> {
        app.world_mut()
            .resource_mut::<Messages<AuxEffect>>()
            .drain()
            .map(|effect| (effect.effect, effect.data))
            .collect()
    };
    effects(&mut app);

    click(&mut app);
    let chunks = app.world().resource::<WorldChunks>();
    assert_eq!(chunks.record_at(2, 5, 0), Some(Item::RecordCat));
    assert_eq!(chunks.metadata_at(2, 5, 0), 1);
    assert_eq!(held(&app, first), 0, "the disc left the hand");
    // Aux effect 1005 names the disc that started.
    assert_eq!(effects(&mut app), [(1005, 2257)]);
    assert_eq!(dropped(&mut app), 0, "and it stays in");

    // The next click throws it out, whatever is held.
    click(&mut app);
    run_ticks(&mut app, 1);
    let chunks = app.world().resource::<WorldChunks>();
    assert_eq!(chunks.record_at(2, 5, 0), None);
    assert_eq!(chunks.metadata_at(2, 5, 0), 0);
    assert_eq!(effects(&mut app), [(1005, 0)]);
    let discs: Vec<Item> = app
        .world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .map(|dropped| dropped.0.item())
        .collect();
    assert_eq!(discs, [Item::RecordCat]);
}

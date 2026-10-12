//! `PlayerAction`s applied headless: what a player's hands do to the world,
//! for a player nobody is holding a mouse for.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::DroppedItem;
use game::entity::creature::Living;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::ItemStack;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::Player;
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
fn a_player_digs_and_builds_with_their_own_hands() {
    let (mut app, first, second) = two_players();
    let hit = top_of(&app, 9, 4, 0);
    act(&mut app, second, Action::StartDig { hit });
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

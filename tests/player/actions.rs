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

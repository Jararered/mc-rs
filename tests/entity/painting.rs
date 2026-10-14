//! `EntityPainting`, `EnumArt` and `ItemPainting`: where a painting hangs,
//! which motives fit, and what knocks it down.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::DroppedItem;
use game::entity::combat::Source;
use game::entity::explosion::Explosion;
use game::entity::painting::Art;
use game::entity::painting::Painting;
use game::entity::painting::motives;
use game::entity::painting::spawn_painting;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;
use game::item::ItemStack;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::Player;
use game::player::actions::Action;
use game::player::actions::PlayerAction;
use game::player::actions::PlayerActionsPlugin;
use game::player::actions::Pointed;
use game::random::ItemRng;
use game::world::chunk::WorldChunks;

use crate::entity::mobs::creature_app;
use crate::entity::mobs::run_ticks;
use crate::entity::pathfinding::field;
use crate::world::block_ticks::TestWorld;
use crate::world::block_ticks::at;

fn art(title: &str) -> Art {
    Art::find(title).unwrap_or_else(|| panic!("no motive {title}"))
}

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).abs().max_element() < 1e-4
}

#[test]
fn the_motive_table_is_beta_s() {
    assert_eq!(motives().len(), 25);
    assert_eq!(Art::default().title(), "Kebab");
    let donkey = art("DonkeyKong").motive();
    assert_eq!((donkey.size, donkey.offset), ((64, 48), (192, 112)));
    assert_eq!(Art::named("NotAPainting"), Art::default());
}

#[test]
fn a_small_painting_sits_a_sixteenth_out_from_its_wall() {
    let painting = Painting::new(IVec3::new(5, 10, 7), 0, art("Kebab"));
    assert!(close(painting.center(), Vec3::new(5.5, 10.5, 6.9375)));
    let aabb = painting.aabb();
    assert!(close(aabb.min, Vec3::new(5.00625, 10.00625, 6.928125)));
    assert!(close(aabb.max, Vec3::new(5.99375, 10.99375, 6.946875)));
    assert_eq!(painting.wall().collect::<Vec<_>>(), [IVec3::new(5, 10, 7)]);
}

#[test]
fn a_wide_painting_spreads_from_the_clicked_block() {
    // Facing west, so it runs along Z. 64 across shifts it half a block, and
    // Beta's `getArtSize` leaves the 48-pixel height unshifted.
    let painting = Painting::new(IVec3::new(0, 64, 0), 1, art("Skeleton"));
    assert!(close(painting.center(), Vec3::new(-0.0625, 64.5, 1.0)));
    assert_eq!(painting.yaw(), 90.0);
    let wall: Vec<_> = painting.wall().collect();
    assert_eq!(wall.len(), 12);
    assert!(wall.iter().all(|cell| cell.x == 0));
    assert_eq!(
        wall.iter().map(|cell| cell.z).min()..=wall.iter().map(|cell| cell.z).max(),
        Some(-1)..=Some(2)
    );
    assert_eq!(
        wall.iter().map(|cell| cell.y).min()..=wall.iter().map(|cell| cell.y).max(),
        Some(63)..=Some(65)
    );
}

/// A single stone block in open air at (8, 70, 8).
fn lone_block() -> TestWorld {
    let mut w = TestWorld::new(1);
    w.set(at(8, 70, 8), Block::Stone);
    w
}

#[test]
fn a_lone_block_only_takes_one_block_motives() {
    let w = lone_block();
    let mut rng = ItemRng::default();
    let mut seen = Vec::new();
    for _ in 0..200 {
        let painting = Painting::hang(&w.chunks, at(8, 70, 8), BlockFace::South, &[], &mut rng)
            .expect("a 16×16 motive fits");
        assert_eq!(painting.direction, 2);
        assert_eq!(painting.art.motive().size, (16, 16));
        if !seen.contains(&painting.art) {
            seen.push(painting.art);
        }
    }
    assert_eq!(seen.len(), 7, "every one-block motive comes up");
}

#[test]
fn nothing_hangs_on_a_top_or_bottom() {
    let w = lone_block();
    let mut rng = ItemRng::default();
    for face in [BlockFace::Up, BlockFace::Down] {
        assert!(Painting::hang(&w.chunks, at(8, 70, 8), face, &[], &mut rng).is_none());
    }
}

#[test]
fn a_painting_needs_a_solid_wall_and_room_in_front() {
    let mut w = lone_block();
    let mut rng = ItemRng::default();
    // A torch is `Material.circuits`, which is not solid.
    w.set(at(8, 70, 8), Block::Torch);
    assert!(Painting::hang(&w.chunks, at(8, 70, 8), BlockFace::East, &[], &mut rng).is_none());
    w.set(at(8, 70, 8), Block::Stone);
    w.set(at(9, 70, 8), Block::Cobblestone);
    assert!(
        Painting::hang(&w.chunks, at(8, 70, 8), BlockFace::East, &[], &mut rng).is_none(),
        "a block in front"
    );
    assert!(Painting::hang(&w.chunks, at(8, 70, 8), BlockFace::West, &[], &mut rng).is_some());
}

#[test]
fn two_paintings_do_not_overlap() {
    let w = lone_block();
    let mut rng = ItemRng::default();
    let first = Painting::hang(&w.chunks, at(8, 70, 8), BlockFace::North, &[], &mut rng).unwrap();
    assert!(
        Painting::hang(
            &w.chunks,
            at(8, 70, 8),
            BlockFace::North,
            &[first.aabb()],
            &mut rng
        )
        .is_none()
    );
}

#[test]
fn a_big_wall_takes_the_big_motives_too() {
    let mut w = TestWorld::new(1);
    w.fill(at(0, 60, 8), at(15, 70, 8), Block::Stone);
    let mut rng = ItemRng::default();
    let big = (0..400)
        .filter_map(|_| Painting::hang(&w.chunks, at(8, 65, 8), BlockFace::North, &[], &mut rng))
        .any(|painting| painting.art.motive().size == (64, 64));
    assert!(big);
}

/// A player at the origin on a grass field (y = 4) facing a stone wall along
/// z = 3, two paintings in hand.
fn gallery() -> (App, Entity) {
    let mut chunks: WorldChunks = field(4);
    for x in -2..=6 {
        for y in 5..=9 {
            chunks.set_block(x, y, 3, Block::Stone);
        }
    }
    let mut app = creature_app(chunks, Vec3::new(0.5, 5.0, 0.5));
    app.add_plugins(PlayerActionsPlugin);
    let player = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = ItemStack::new(Item::Painting, 2).ok();
    app.world_mut()
        .entity_mut(player)
        .insert((hotbar, Inventory::default()));
    (app, player)
}

fn act(app: &mut App, player: Entity, action: Action) {
    app.world_mut()
        .resource_mut::<Messages<PlayerAction>>()
        .write(PlayerAction { player, action });
    run_ticks(app, 1);
}

fn hang_one(app: &mut App, player: Entity) -> Entity {
    let hit = BlockHit {
        x: 1,
        y: 6,
        z: 3,
        face: BlockFace::North,
        block: Block::Stone,
    };
    act(
        app,
        player,
        Action::Use {
            hit: Some(hit),
            origin: Vec3::new(0.5, 6.62, 0.5),
            look: Vec3::Z,
            click: true,
        },
    );
    let mut paintings = app.world_mut().query_filtered::<Entity, With<Painting>>();
    let found: Vec<_> = paintings.iter(app.world()).collect();
    assert_eq!(found.len(), 1, "one painting hangs");
    found[0]
}

fn paintings_dropped(app: &mut App) -> usize {
    app.world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .filter(|item| item.0.item() == Item::Painting)
        .count()
}

fn held(app: &App, player: Entity) -> u8 {
    app.world().get::<Hotbar>(player).unwrap().slots[0].map_or(0, |stack| stack.count())
}

#[test]
fn a_painting_is_hung_from_the_hand() {
    let (mut app, player) = gallery();
    let entity = hang_one(&mut app, player);
    assert_eq!(held(&app, player), 1);
    let painting = app.world().get::<Painting>(entity).unwrap();
    assert_eq!(painting.tile, IVec3::new(1, 6, 3));
    assert_eq!(painting.direction, 0);
    assert!(
        painting
            .wall()
            .all(|cell| cell.z == 3 && (-2..=6).contains(&cell.x) && (5..=9).contains(&cell.y))
    );
}

#[test]
fn a_painting_falls_off_a_wall_that_is_gone() {
    let (mut app, player) = gallery();
    let entity = hang_one(&mut app, player);
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        for x in -2..=6 {
            for y in 5..=9 {
                chunks.set_block(x, y, 3, Block::Air);
            }
        }
    }
    // It looks again only every 101 ticks.
    run_ticks(&mut app, 95);
    assert!(app.world().get_entity(entity).is_ok());
    assert_eq!(paintings_dropped(&mut app), 0);
    run_ticks(&mut app, 8);
    assert!(app.world().get_entity(entity).is_err(), "fell off");
    assert_eq!(paintings_dropped(&mut app), 1);
}

#[test]
fn any_hit_knocks_a_painting_down() {
    let (mut app, player) = gallery();
    let entity = hang_one(&mut app, player);
    act(
        &mut app,
        player,
        Action::UseEntity {
            target: Pointed::Painting(entity),
            attack: false,
            look: Vec3::Z,
        },
    );
    assert!(
        app.world().get_entity(entity).is_ok(),
        "a right-click leaves it"
    );
    act(
        &mut app,
        player,
        Action::UseEntity {
            target: Pointed::Painting(entity),
            attack: true,
            look: Vec3::Z,
        },
    );
    assert!(app.world().get_entity(entity).is_err());
    assert_eq!(paintings_dropped(&mut app), 1);
}

#[test]
fn an_explosion_knocks_a_painting_down() {
    let (mut app, _) = gallery();
    let entity = spawn_painting(
        &mut app.world_mut().commands(),
        Painting::new(IVec3::new(4, 6, 3), 0, art("Kebab")),
    );
    app.world_mut().flush();
    app.world_mut()
        .resource_mut::<Messages<Explosion>>()
        .write(Explosion {
            center: Vec3::new(4.5, 6.5, 1.5),
            strength: 1.0,
            flaming: false,
            source: Source::Environment,
        });
    run_ticks(&mut app, 1);
    assert!(app.world().get_entity(entity).is_err());
    assert_eq!(paintings_dropped(&mut app), 1);
}

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::block::blocks::Block;
use game::entity::DroppedItem;
use game::entity::EntitySize;
use game::entity::explosion::PrimedTnt;
use game::entity::falling_block::FallingBlock;
use game::entity::minecart::Cargo;
use game::entity::minecart::CartKind;
use game::entity::minecart::Minecart;
use game::entity::minecart::break_cart;
use game::entity::minecart::spawn_cart;
use game::entity::minecart::spawn_minecart;
use game::entity::mobs::Mob;
use game::entity::mobs::MobType;
use game::entity::mobs::spawn as spawn_mob;
use game::entity::mount::Mounted;
use game::entity::mount::dismount;
use game::entity::mount::mount;
use game::entity::projectiles::ARROW_SIZE;
use game::entity::projectiles::Arrow;
use game::entity::projectiles::Projectile;
use game::entity::thrown::Thrown;
use game::entity::thrown::ThrownKind;
use game::item::Item;
use game::item::ItemStack;
use game::player::Player;
use game::random::ItemRng;
use game::world::biome::Biome;
use game::world::block_ticks::BlockEvent;
use game::world::block_ticks::BlockTicks;
use game::world::block_ticks::BlockTicksPlugin;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::tick::TICK_SECONDS;
use game::world::tick::WorldTick;

use super::at;
use super::generated;

/// A headless app with the block tick systems over a 5×5 of chunks with a
/// stone floor at y = 60 across chunk (0, 0).
fn app() -> App {
    let mut chunks = WorldChunks::default();
    for x in -2..=2 {
        for z in -2..=2 {
            let mut chunk = Chunk::new();
            if x == 0 && z == 0 {
                for cx in 0..16 {
                    for cz in 0..16 {
                        chunk.set(cx, 60, cz, Block::Stone);
                    }
                }
            }
            chunks.insert(ChunkPosition { x, z }, generated(chunk, Biome::Plains));
        }
    }
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .insert_resource(chunks)
        .add_plugins(BlockTicksPlugin);
    app.update();
    app
}

/// One frame carrying one world tick.
fn step(app: &mut App) {
    app.world_mut()
        .resource_mut::<WorldTick>()
        .advance(TICK_SECONDS * 1.001);
    app.update();
}

fn block(app: &App, x: i32, y: i32, z: i32) -> Option<Block> {
    app.world().resource::<WorldChunks>().block_at(x, y, z)
}

/// Write a block and report it, as block editing does.
fn place(app: &mut App, x: i32, y: i32, z: i32, placed: Block) {
    let previous = app
        .world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(x, y, z, placed)
        .unwrap();
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .block_changed(at(x, y, z), previous, 0);
}

#[test]
fn unsupported_sand_falls_as_an_entity_and_lands_as_a_block() {
    let mut app = app();
    place(&mut app, 8, 70, 8, Block::Sand);
    let mut spawned = false;
    for _ in 0..80 {
        step(&mut app);
        let falling = app
            .world_mut()
            .query::<&FallingBlock>()
            .iter(app.world())
            .count();
        spawned |= falling > 0;
        if spawned && falling == 0 {
            break;
        }
    }
    assert!(spawned, "the sand became a falling entity");
    assert_eq!(block(&app, 8, 70, 8), Some(Block::Air));
    assert_eq!(
        block(&app, 8, 61, 8),
        Some(Block::Sand),
        "it lands on the floor"
    );
    assert_eq!(
        app.world_mut()
            .query::<&FallingBlock>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn a_torch_that_loses_its_floor_drops_an_item_entity() {
    let mut app = app();
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(8, 61, 8, Block::Stone);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(8, 62, 8, Block::Torch);
    place(&mut app, 8, 61, 8, Block::Air);
    step(&mut app);
    assert_eq!(block(&app, 8, 62, 8), Some(Block::Air));
    let items: Vec<_> = app
        .world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .map(|item| item.0.item())
        .collect();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        game::item::Item::from_block(Block::Torch).unwrap()
    );
}

/// Activate the lever at `(x, y, z)` as a player click would.
fn flick(app: &mut App, x: i32, y: i32, z: i32) {
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .push_event(BlockEvent::Activated {
            position: at(x, y, z),
        });
}

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut().query::<&T>().iter(app.world()).count()
}

/// Load a dispenser facing east at (8, 61, 8) with `stack` and pulse it.
fn fire_dispenser(app: &mut App, stack: ItemStack) {
    place(app, 8, 61, 8, Block::Dispenser);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(8, 61, 8, 5);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .dispenser_at_mut(8, 61, 8)
        .unwrap()
        .slots[0] = Some(stack);
    place(app, 7, 61, 8, Block::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    flick(app, 7, 61, 8);
    for _ in 0..5 {
        step(app);
    }
}

#[test]
fn a_dispensed_arrow_can_be_picked_up() {
    let mut app = app();
    fire_dispenser(&mut app, ItemStack::new(Item::Arrow, 1).unwrap());
    let arrows: Vec<bool> = app
        .world_mut()
        .query::<&Arrow>()
        .iter(app.world())
        .map(|arrow| arrow.pickup)
        .collect();
    assert_eq!(arrows, [true]);
}

#[test]
fn dispensers_launch_eggs_and_snowballs() {
    for (item, kind) in [
        (Item::Egg, ThrownKind::Egg),
        (Item::Snowball, ThrownKind::Snowball),
    ] {
        let mut app = app();
        fire_dispenser(&mut app, ItemStack::new(item, 1).unwrap());
        let thrown: Vec<(ThrownKind, Vec3)> = app
            .world_mut()
            .query::<&Thrown>()
            .iter(app.world())
            .map(|thrown| (thrown.kind, thrown.motion))
            .collect();
        assert_eq!(thrown.len(), 1, "{item:?}");
        assert_eq!(thrown[0].0, kind);
        // East, at about 1.1 blocks per tick.
        assert!(thrown[0].1.x > 0.9, "{}", thrown[0].1);
        assert_eq!(count::<DroppedItem>(&mut app), 0);
    }
}

#[test]
fn an_arrow_holds_down_a_wooden_plate_but_not_a_stone_one() {
    for (plate, pressed) in [
        (Block::WoodenPressurePlate, 1),
        (Block::StonePressurePlate, 0),
    ] {
        let mut app = app();
        place(&mut app, 8, 61, 8, plate);
        step(&mut app);
        app.world_mut()
            .spawn((Projectile, ARROW_SIZE, Transform::from_xyz(8.5, 61.0, 8.5)));
        for _ in 0..3 {
            step(&mut app);
        }
        assert_eq!(
            app.world().resource::<WorldChunks>().metadata_at(8, 61, 8),
            pressed,
            "{plate:?}"
        );
    }
}

#[test]
fn placed_cart_activates_detector_rail() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Block::DetectorRail);
    step(&mut app);
    spawn_minecart(&mut app.world_mut().commands(), at(8, 61, 8));
    app.world_mut().flush();
    step(&mut app);
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(8, 61, 8) & 8,
        8
    );
    assert_eq!(count::<Minecart>(&mut app), 1);
}

#[test]
fn powered_dispenser_fires_an_arrow_entity() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Block::Dispenser);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(8, 61, 8, 5);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .dispenser_at_mut(8, 61, 8)
        .unwrap()
        .slots[0] = Some(ItemStack::new(Item::Arrow, 2).unwrap());
    place(&mut app, 7, 61, 8, Block::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    flick(&mut app, 7, 61, 8);
    for _ in 0..5 {
        step(&mut app);
    }
    assert_eq!(count::<Arrow>(&mut app), 1);
    assert_eq!(
        app.world()
            .resource::<WorldChunks>()
            .dispenser_at(8, 61, 8)
            .unwrap()
            .slots[0]
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn powered_tnt_becomes_a_primed_entity() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Block::Tnt);
    place(&mut app, 7, 61, 8, Block::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    flick(&mut app, 7, 61, 8);
    step(&mut app);
    assert_eq!(block(&app, 8, 61, 8), Some(Block::Air));
    assert_eq!(count::<PrimedTnt>(&mut app), 1);
}

#[test]
fn extending_piston_pushes_a_player_out_of_the_new_head() {
    let mut app = app();
    let player = app
        .world_mut()
        .spawn((
            Player,
            EntitySize::PLAYER,
            Transform::from_xyz(9.5, 61.0 + EntitySize::PLAYER.y_offset, 8.5),
        ))
        .id();
    place(&mut app, 8, 61, 8, Block::Piston);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(8, 61, 8, 5);
    place(&mut app, 7, 61, 8, Block::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    flick(&mut app, 7, 61, 8);
    step(&mut app);
    assert_eq!(block(&app, 9, 61, 8), Some(Block::PistonHead));
    assert!(app.world().get::<Transform>(player).unwrap().translation.x > 10.3);
}

/// A straight track along z = 8 over the floor, and a cart on it.
fn laid_cart(app: &mut App, kind: CartKind) -> Entity {
    for x in 4..=30 {
        app.world_mut()
            .resource_mut::<WorldChunks>()
            .set_block_with_metadata(x, 61, 8, Block::Rail, 1);
    }
    let cart = spawn_cart(&mut app.world_mut().commands(), at(8, 61, 8), kind);
    app.world_mut().flush();
    cart
}

fn cart_of(app: &App, cart: Entity) -> Minecart {
    app.world().get::<Minecart>(cart).unwrap().clone()
}

fn player(app: &mut App, feet: Vec3) -> Entity {
    app.world_mut()
        .spawn((
            Player,
            Transform::from_translation(feet + Vec3::Y * EntitySize::PLAYER.y_offset),
        ))
        .id()
}

#[test]
fn a_rider_goes_where_the_cart_goes_and_steps_off_onto_it() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Empty);
    let rider = player(&mut app, Vec3::new(20.5, 61.0, 20.5));
    mount(&mut app.world_mut().commands(), rider, cart);
    app.world_mut().flush();
    assert_eq!(cart_of(&app, cart).rider, Some(rider));
    app.world_mut().get_mut::<Minecart>(cart).unwrap().motion.x = 0.3;
    for _ in 0..10 {
        step(&mut app);
    }
    let at_cart = app.world().get::<Transform>(cart).unwrap().translation;
    let at_rider = app.world().get::<Transform>(rider).unwrap().translation;
    assert!(at_cart.x > 10.0, "the cart rolled to {at_cart}");
    // Sitting 0.3 below the cart's centre, at the player's eye height less
    // half a block.
    let lift = Vec3::Y * (-0.3 + 1.62 - 0.5);
    assert!((at_rider - (at_cart + lift)).length() < 0.001, "{at_rider}");

    dismount(&mut app.world_mut().commands(), rider);
    app.world_mut().flush();
    assert!(app.world().get::<Mounted>(rider).is_none());
    assert_eq!(cart_of(&app, cart).rider, None);
    let at_rider = app.world().get::<Transform>(rider).unwrap().translation;
    assert!((at_rider.y - (at_cart.y + 0.35 + 1.62)).abs() < 0.001);
}

#[test]
fn a_rider_is_let_go_when_its_cart_is_gone() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Empty);
    let rider = player(&mut app, Vec3::new(20.5, 61.0, 20.5));
    mount(&mut app.world_mut().commands(), rider, cart);
    app.world_mut().flush();
    app.world_mut().despawn(cart);
    step(&mut app);
    step(&mut app);
    assert!(app.world().get::<Mounted>(rider).is_none());
}

#[test]
fn a_new_rider_takes_the_cart_from_the_old_one() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Empty);
    let first = player(&mut app, Vec3::new(20.5, 61.0, 20.5));
    let second = player(&mut app, Vec3::new(22.5, 61.0, 20.5));
    mount(&mut app.world_mut().commands(), first, cart);
    app.world_mut().flush();
    mount(&mut app.world_mut().commands(), second, cart);
    app.world_mut().flush();
    assert_eq!(cart_of(&app, cart).rider, Some(second));
    assert!(app.world().get::<Mounted>(first).is_none());
    assert!(app.world().get::<Mounted>(second).is_some());
}

#[test]
fn a_moving_empty_cart_scoops_up_a_creature_in_its_way_but_a_resting_one_does_not() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Empty);
    let zombie = spawn_mob(
        &mut app.world_mut().commands(),
        Mob::new(MobType::Zombie, 1),
        Vec3::new(9.0, 61.0, 8.5),
    );
    app.world_mut().flush();
    step(&mut app);
    assert!(
        app.world().get::<Mounted>(zombie).is_none(),
        "a cart at rest only pushes"
    );
    app.world_mut().get_mut::<Minecart>(cart).unwrap().motion.x = 0.2;
    step(&mut app);
    step(&mut app);
    assert_eq!(cart_of(&app, cart).rider, Some(zombie));
    assert_eq!(
        app.world()
            .get::<Mounted>(zombie)
            .map(|mounted| mounted.vehicle),
        Some(cart)
    );
}

#[test]
fn a_player_walking_into_a_cart_shoves_it_along() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Empty);
    // Overlapping the cart's western edge.
    player(&mut app, Vec3::new(7.9, 61.0, 8.5));
    step(&mut app);
    assert!(cart_of(&app, cart).motion.x > 0.0);
    assert!(app.world().get::<Mounted>(cart).is_none());
}

#[test]
fn a_broken_chest_cart_leaves_its_parts_and_spills_its_cargo() {
    let mut app = app();
    let cart = laid_cart(&mut app, CartKind::Chest);
    let mut cargo = app
        .world()
        .get::<Cargo>(cart)
        .expect("a chest cart")
        .clone();
    cargo.0[3] = Some(ItemStack::new(Item::Stick, 5).unwrap());
    app.world_mut().entity_mut(cart).insert(cargo.clone());
    let rider = player(&mut app, Vec3::new(20.5, 61.0, 20.5));
    let broken = cart_of(&app, cart);
    break_cart(
        &mut app.world_mut().commands(),
        &mut ItemRng::default(),
        cart,
        &broken,
        Vec3::new(8.5, 61.5, 8.5),
        Some(&cargo),
    );
    app.world_mut().flush();
    assert!(app.world().get_entity(cart).is_err(), "the cart is gone");
    assert!(app.world().get::<Mounted>(rider).is_none());
    let mut found: Vec<_> = app
        .world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .map(|item| (item.0.item(), item.0.count()))
        .collect();
    found.sort_by_key(|(item, _)| item.as_u16());
    let total = |wanted: Item| -> u32 {
        found
            .iter()
            .filter(|(item, _)| *item == wanted)
            .map(|(_, count)| u32::from(*count))
            .sum()
    };
    assert_eq!(total(Item::Minecart), 1);
    assert_eq!(total(Item::from_block(Block::Chest).unwrap()), 1);
    assert_eq!(total(Item::Stick), 5);
}

#[test]
fn a_chest_cart_has_a_cargo_and_the_others_do_not() {
    let mut app = app();
    let chest = laid_cart(&mut app, CartKind::Chest);
    let furnace = spawn_cart(
        &mut app.world_mut().commands(),
        at(12, 61, 8),
        CartKind::Furnace,
    );
    app.world_mut().flush();
    assert_eq!(app.world().get::<Cargo>(chest), Some(&Cargo::default()),);
    assert!(app.world().get::<Cargo>(furnace).is_none());
    assert_eq!(cart_of(&app, furnace).kind, CartKind::Furnace);
}

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::block::id::Id;
use game::entity::DroppedItem;
use game::entity::EntitySize;
use game::entity::falling_block::FallingBlock;
use game::entity::minecart::Minecart;
use game::entity::minecart::spawn_minecart;
use game::entity::projectile::Projectile;
use game::entity::tnt::PrimedTnt;
use game::item::ItemId;
use game::item::ItemStack;
use game::player::Player;
use game::world::block_ticks::BlockEvent;
use game::world::block_ticks::BlockTicks;
use game::world::block_ticks::BlockTicksPlugin;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
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
                        chunk.set(cx, 60, cz, Id::Stone);
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

fn block(app: &App, x: i32, y: i32, z: i32) -> Option<Id> {
    app.world().resource::<WorldChunks>().block_at(x, y, z)
}

/// Write a block and report it, as block editing does.
fn place(app: &mut App, x: i32, y: i32, z: i32, placed: Id) {
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
    place(&mut app, 8, 70, 8, Id::Sand);
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
    assert_eq!(block(&app, 8, 70, 8), Some(Id::Air));
    assert_eq!(
        block(&app, 8, 61, 8),
        Some(Id::Sand),
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
        .set_block(8, 61, 8, Id::Stone);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_block(8, 62, 8, Id::Torch);
    place(&mut app, 8, 61, 8, Id::Air);
    step(&mut app);
    assert_eq!(block(&app, 8, 62, 8), Some(Id::Air));
    let items: Vec<_> = app
        .world_mut()
        .query::<&DroppedItem>()
        .iter(app.world())
        .map(|item| item.0.item())
        .collect();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0], game::item::ItemId::from_block(Id::Torch).unwrap());
}

#[test]
fn placed_cart_activates_detector_rail() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Id::DetectorRail);
    step(&mut app);
    spawn_minecart(&mut app.world_mut().commands(), at(8, 61, 8));
    app.world_mut().flush();
    step(&mut app);
    assert_eq!(
        app.world().resource::<WorldChunks>().metadata_at(8, 61, 8) & 8,
        8
    );
    assert_eq!(
        app.world_mut()
            .query::<&Minecart>()
            .iter(app.world())
            .count(),
        1
    );
}

#[test]
fn powered_dispenser_fires_an_arrow_entity() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Id::Dispenser);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(8, 61, 8, 5);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .dispenser_at_mut(8, 61, 8)
        .unwrap()
        .slots[0] = Some(ItemStack::new(ItemId::Arrow, 2).unwrap());
    place(&mut app, 7, 61, 8, Id::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .push_event(BlockEvent::Activated {
            position: at(7, 61, 8),
        });
    for _ in 0..5 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut()
            .query::<&Projectile>()
            .iter(app.world())
            .count(),
        1
    );
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
fn powered_tnt_explodes_after_eighty_fuse_ticks() {
    let mut app = app();
    place(&mut app, 8, 61, 8, Id::Tnt);
    place(&mut app, 8, 61, 9, Id::Stone);
    place(&mut app, 7, 61, 8, Id::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .push_event(BlockEvent::Activated {
            position: at(7, 61, 8),
        });
    step(&mut app);
    assert_eq!(block(&app, 8, 61, 8), Some(Id::Air));
    assert_eq!(
        app.world_mut()
            .query::<&PrimedTnt>()
            .iter(app.world())
            .count(),
        1
    );
    for _ in 0..81 {
        step(&mut app);
    }
    assert_eq!(
        app.world_mut()
            .query::<&PrimedTnt>()
            .iter(app.world())
            .count(),
        0
    );
    assert_eq!(block(&app, 8, 61, 9), Some(Id::Air));
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
    place(&mut app, 8, 61, 8, Id::Piston);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(8, 61, 8, 5);
    place(&mut app, 7, 61, 8, Id::Lever);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .set_metadata(7, 61, 8, 5);
    app.world_mut()
        .resource_mut::<BlockTicks>()
        .push_event(BlockEvent::Activated {
            position: at(7, 61, 8),
        });
    step(&mut app);
    assert_eq!(block(&app, 9, 61, 8), Some(Id::PistonHead));
    assert!(app.world().get::<Transform>(player).unwrap().translation.x > 10.3);
}

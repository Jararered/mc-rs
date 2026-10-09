//! Two dimensions in one save, and travelling between them.

use std::time::Duration;

use bevy::prelude::*;
use game::app::session::Travel;
use game::app::session::WorldChoice;
use game::app::session::WorldSession;
use game::block::blocks::Block;
use game::inventory::Hotbar;
use game::item::ItemStack;
use game::player::Player;
use game::player::PlayerHealth;
use game::player::portal::PortalTravel;
use game::world::biome::Biome;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::difficulty::Difficulty;
use game::world::dimension::ActiveDimension;
use game::world::dimension::Dimension;
use game::world::generation::ChunkGenerator;
use game::world::generation::nether::NetherGenerator;
use game::world::lighting::LightCache;
use game::world::persistence::SaveFormat;
use game::world::persistence::StoredPlayer;
use game::world::persistence::WorldPersistence;
use game::world::persistence::WorldStorage;

use super::load_world;
use super::run_until;
use super::session_app;
use super::temp_saves;

#[test]
fn each_format_keeps_the_nether_apart_from_the_overworld() {
    for format in [SaveFormat::Binary, SaveFormat::Original] {
        let saves = temp_saves("dimensions");
        let storage = WorldStorage::create_in_format(&saves, 11, "Two", None, format).unwrap();
        let position = ChunkPosition { x: 3, z: -2 };

        let mut overworld = Chunk::new();
        overworld.set(1, 1, 1, Block::GoldBlock);
        let overworld = crate::world::block_ticks::generated(overworld, Biome::Plains);
        storage.save_chunk(position, &overworld).unwrap();

        let nether = NetherGenerator::new(11).generate_base(position);
        storage.set_dimension(Dimension::Nether);
        assert!(
            storage.load_chunk(position).is_none(),
            "{format:?}: the Nether starts with no chunks"
        );
        storage.save_chunk(position, &nether).unwrap();
        let folder = match format {
            SaveFormat::Binary => "DIM-1/regions",
            SaveFormat::Original => "DIM-1/region",
        };
        assert!(storage.root().join(folder).is_dir(), "{format:?}");

        let root = storage.root().to_path_buf();
        drop(storage);
        let storage = WorldStorage::open(root).unwrap();
        // A reopened world addresses the Overworld until told otherwise.
        assert_eq!(storage.dimension(), Dimension::Overworld);
        let loaded = storage.load_chunk(position).unwrap();
        assert_eq!(loaded.chunk.get(1, 1, 1), Some(Block::GoldBlock));
        let loaded = storage
            .load_chunk_in(Dimension::Nether, position)
            .unwrap_or_else(|| panic!("{format:?}: the Nether chunk loads"));
        assert_eq!(loaded.chunk.raw_blocks(), nether.chunk.raw_blocks());
        assert_eq!(loaded.biomes.get(5, 5).biome, Biome::Hell);
    }
}

#[test]
fn the_player_record_remembers_its_dimension() {
    for format in [SaveFormat::Binary, SaveFormat::Original] {
        let saves = temp_saves("player-dimension");
        let storage = WorldStorage::create_in_format(&saves, 5, "Where", None, format).unwrap();
        let player = StoredPlayer::from_transform(&Transform::from_xyz(4.5, 70.0, -9.5))
            .with_dimension(Dimension::Nether);
        storage.save_player(&player).unwrap();
        let root = storage.root().to_path_buf();
        drop(storage);
        let loaded = WorldStorage::open(root).unwrap().load_player().unwrap();
        assert_eq!(loaded.dimension, Dimension::Nether, "{format:?}");
    }
    // A record from before dimensions existed is in the Overworld.
    let old: StoredPlayer = serde_json::from_str(&format!(
        r#"{{"format_version":{},"x":0.0,"y":70.0,"z":0.0,"yaw":0.0,"pitch":0.0}}"#,
        game::world::persistence::FORMAT_VERSION
    ))
    .unwrap();
    assert_eq!(old.dimension, Dimension::Overworld);
}

fn travel(app: &mut App, travel: Travel) {
    app.world_mut()
        .resource_mut::<WorldSession>()
        .request_travel(travel);
    app.update();
    assert!(
        app.world().resource::<WorldSession>().is_busy(),
        "travel should start"
    );
    assert!(
        run_until(app, Duration::from_secs(120), |app| {
            app.world().resource::<WorldSession>().is_active()
        }),
        "the other dimension never loaded"
    );
}

fn player_position(app: &mut App) -> Vec3 {
    app.world_mut()
        .query_filtered::<&Transform, With<Player>>()
        .single(app.world())
        .unwrap()
        .translation
}

/// The portal blocks in the loaded chunks, as world cells.
fn portal_blocks(app: &App) -> Vec<IVec3> {
    let chunks = app.world().resource::<WorldChunks>();
    let mut cells = Vec::new();
    for position in chunks.positions() {
        let chunk = &chunks.get(position).unwrap().chunk;
        for (index, raw) in chunk.raw_blocks().iter().enumerate() {
            if *raw == Block::NetherPortal.as_u8() {
                cells.push(IVec3::new(
                    position.x * 16 + (index % 16) as i32,
                    (index / 256) as i32,
                    position.z * 16 + (index / 16 % 16) as i32,
                ));
            }
        }
    }
    cells
}

#[test]
fn travelling_through_a_portal_swaps_the_dimension_and_keeps_the_player() {
    let saves = temp_saves("travel");
    let mut app = session_app(&saves);
    load_world(
        &mut app,
        WorldChoice::New {
            name: "Portal".to_owned(),
            seed: 21,
            difficulty: Difficulty::Peaceful,
            format: SaveFormat::Binary,
        },
    );
    let player = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    // Far enough out that the two dimensions' coordinates differ.
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(400.5, 80.0, -240.5);
    let diamond = ItemStack::new(game::item::Item::Diamond, 3).unwrap();
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Hotbar>()
        .unwrap()
        .slots[2] = Some(diamond);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<PlayerHealth>()
        .unwrap()
        .current = 7;

    travel(&mut app, Travel::Portal);

    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Nether
    );
    assert_eq!(
        app.world().resource::<WorldPersistence>().dimension(),
        Dimension::Nether
    );
    assert!(!app.world().resource::<LightCache>().has_sky());
    assert!(
        app.world()
            .resource::<WorldSession>()
            .travelling_to()
            .is_none()
    );
    // The same player entity, with what it carried.
    let now = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    assert_eq!(now, player);
    assert_eq!(
        app.world().get::<Hotbar>(player).unwrap().slots[2],
        Some(diamond)
    );
    assert_eq!(app.world().get::<PlayerHealth>(player).unwrap().current, 7);

    // A portal was built within reach of an eighth of the way out, and the
    // player stands in it.
    let nether_position = player_position(&mut app);
    assert!(
        (nether_position.x - 50.0).abs() <= 18.0,
        "{nether_position}"
    );
    assert!(
        (nether_position.z + 30.0).abs() <= 18.0,
        "{nether_position}"
    );
    let portal = portal_blocks(&app);
    assert_eq!(portal.len(), 6, "one new portal");
    let feet = nether_position - Vec3::Y * 1.62;
    assert!(
        portal.iter().any(|cell| *cell == feet.floor().as_ivec3()),
        "feet {feet} are in the portal {portal:?}"
    );
    // The terrain around is the Nether's.
    let chunks = app.world().resource::<WorldChunks>();
    let here = ChunkPosition::from_world(nether_position.x, nether_position.z);
    assert_eq!(
        chunks.get(here).unwrap().biomes.get(0, 0).biome,
        Biome::Hell
    );
    assert_eq!(
        chunks.get(here).unwrap().chunk.get(0, 127, 0),
        Some(Block::Bedrock)
    );

    // Back out: no portal waits in the Overworld, so one is built there.
    travel(&mut app, Travel::Portal);
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Overworld
    );
    let overworld_position = player_position(&mut app);
    assert!((overworld_position.x - nether_position.x * 8.0).abs() <= 18.0);
    assert!((overworld_position.z - nether_position.z * 8.0).abs() <= 18.0);
    assert_eq!(portal_blocks(&app).len(), 6);

    // And in again: this time the first Nether portal is found, not a new one.
    travel(&mut app, Travel::Portal);
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Nether
    );
    let back = player_position(&mut app);
    assert!(
        back.distance(nether_position) < 0.01,
        "{back} is not the first portal at {nether_position}"
    );
    assert!(
        run_until(&mut app, Duration::from_secs(60), |app| {
            portal_blocks(app).len() == 6
        }),
        "the saved portal's chunk loads with exactly the one portal"
    );
    // A fresh arrival may not leave again until the player steps out.
    assert!(app.world().get::<PortalTravel>(player).unwrap().cooldown() > 0);

    // Dying in the Nether comes back to the Overworld's spawn.
    travel(&mut app, Travel::Respawn);
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Overworld
    );
    let spawn = player_position(&mut app);
    assert_eq!((spawn.x, spawn.z), (8.5, 8.5));
    assert!(portal_blocks(&app).is_empty());
}

#[test]
fn a_world_saved_in_the_nether_reopens_there() {
    let saves = temp_saves("reopen-nether");
    let mut app = session_app(&saves);
    load_world(
        &mut app,
        WorldChoice::New {
            name: "Stay".to_owned(),
            seed: 3,
            difficulty: Difficulty::Peaceful,
            format: SaveFormat::Original,
        },
    );
    let root = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .unwrap()
        .root()
        .to_path_buf();
    travel(&mut app, Travel::Portal);
    let position = player_position(&mut app);
    super::leave_world(&mut app);
    app.update();
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Overworld,
        "no world, no dimension"
    );

    load_world(&mut app, WorldChoice::Existing(root));
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Nether
    );
    let reopened = player_position(&mut app);
    assert!(
        reopened.distance(position) < 0.01,
        "{reopened} vs {position}"
    );
    let here = ChunkPosition::from_world(reopened.x, reopened.z);
    let chunks = app.world().resource::<WorldChunks>();
    assert_eq!(
        chunks.get(here).unwrap().biomes.get(0, 0).biome,
        Biome::Hell
    );
}

#[test]
fn a_respawn_reloads_the_world_around_the_players_bed() {
    use game::chat::ChatHistory;
    use game::player::sleep::BED_MISSING_MESSAGE;
    use game::player::sleep::PlayerSleep;

    let saves = temp_saves("bed-respawn");
    let mut app = session_app(&saves);
    app.init_resource::<ChatHistory>();
    load_world(
        &mut app,
        WorldChoice::New {
            name: "Bed".to_owned(),
            seed: 21,
            difficulty: Difficulty::Peaceful,
            format: SaveFormat::Binary,
        },
    );
    // A bed on a platform above the terrain, so the seed does not matter.
    let foot = IVec3::new(5, 121, 6);
    let edit = |app: &mut App, bed: bool| {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        for x in 3..=7 {
            for z in 3..=8 {
                chunks.set_block(x, 120, z, Block::Stone);
            }
        }
        if bed {
            chunks.set_block_with_metadata(5, 121, 5, Block::Bed, 0);
            chunks.set_block_with_metadata(5, 121, 6, Block::Bed, 8);
        } else {
            chunks.set_block(5, 121, 5, Block::Air);
            chunks.set_block(5, 121, 6, Block::Air);
        }
        app.world_mut()
            .resource_mut::<WorldPersistence>()
            .mark_dirty(ChunkPosition::ZERO);
    };
    edit(&mut app, true);
    let player = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(player)
        .get_mut::<PlayerSleep>()
        .unwrap()
        .spawn = Some(foot);
    // Far from the bed, as a player who died away from home.
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Transform>()
        .unwrap()
        .translation = Vec3::new(400.5, 80.0, -240.5);

    travel(&mut app, Travel::Respawn);
    assert_eq!(
        app.world().resource::<ActiveDimension>().0,
        Dimension::Overworld
    );
    let stood = player_position(&mut app);
    assert!(
        (stood - Vec3::new(4.5, 121.1 + 1.62, 5.5)).length() < 0.01,
        "{stood}"
    );
    assert_eq!(
        app.world().get::<PlayerSleep>(player).unwrap().spawn,
        Some(foot)
    );
    assert_eq!(
        app.world()
            .resource::<WorldChunks>()
            .block_at(foot.x, foot.y, foot.z),
        Some(Block::Bed)
    );

    // Without the bed the world spawn takes over and the player is told.
    edit(&mut app, false);
    travel(&mut app, Travel::Respawn);
    let stood = player_position(&mut app);
    assert!(
        (stood.x - 8.5).abs() < 0.01 && (stood.z - 8.5).abs() < 0.01,
        "{stood}"
    );
    assert_eq!(app.world().get::<PlayerSleep>(player).unwrap().spawn, None);
    assert!(
        app.world()
            .resource::<ChatHistory>()
            .messages()
            .any(|message| message.text == BED_MISSING_MESSAGE)
    );
}

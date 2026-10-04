use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::block::blocks::Block;
use game::block::direction::HorizontalFacing;
use game::entity::mobs::Mob;
use game::entity::mobs::MobRecord;
use game::entity::mobs::MobSpawner;
use game::entity::mobs::MobType;
use game::item::ItemStack;
use game::player::Player;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkDroppedItem;
use game::world::chunk::ChunkPosition;
use game::world::chunk::PendingTick;
use game::world::chunk::WorldChunks;
use game::world::generation::overworld::OverworldGenerator;
use game::world::persistence::CHUNK_FORMAT_VERSION;
use game::world::persistence::FORMAT_VERSION;
use game::world::persistence::PersistencePlugin;
use game::world::persistence::REGION_SIZE;
use game::world::persistence::StoredPlayer;
use game::world::persistence::WorldPersistence;
use game::world::persistence::WorldStorage;
use game::world::persistence::chunk_file_name;
use game::world::persistence::region_dir_name;
use game::world::persistence::region_of;
use game::world::plugin::WorldPlugin;
use game::world::weather::WorldWeather;

/// A unique, empty directory under the system temp directory.
fn temp_saves(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("game-persistence-{label}-{unique}"));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn assert_same_blocks(left: &Chunk, right: &Chunk) {
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert_eq!(
                    left.get(x, y, z),
                    right.get(x, y, z),
                    "block at {x},{y},{z}"
                );
            }
        }
    }
}

/// A headless app with the world and persistence plugins, matching how
/// `GamePlugin` wires them together.
fn persistence_app(saves: &Path) -> App {
    app_with(PersistencePlugin::new(saves.to_path_buf()))
}

fn app_with(persistence: PersistencePlugin) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin))
        .add_plugins(persistence);
    app
}

fn run_until(
    app: &mut App,
    timeout: Duration,
    mut condition: impl FnMut(&mut App) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition(app) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

#[test]
fn world_folders_are_unique_and_named_from_the_creation_time() {
    let saves = temp_saves("unique");
    let first = WorldStorage::create(&saves, 0, "First").unwrap();
    let second = WorldStorage::create(&saves, 0, "Second").unwrap();
    assert_ne!(first.root(), second.root());

    let name = first.root().file_name().unwrap().to_str().unwrap();
    let parts: Vec<_> = name.split('-').collect();
    assert_eq!(parts.len(), 4, "unexpected world folder name {name}");
    assert_eq!(parts[0], "world");
    assert_eq!(parts[1].len(), 8, "date part of {name}");
    assert_eq!(parts[2].len(), 6, "time part of {name}");
    assert_eq!(parts[3].len(), 8, "hash part of {name}");
    assert!(first.root().join("level.json").is_file());
}

#[test]
fn manifest_records_the_seed_and_name() {
    let saves = temp_saves("manifest");
    let storage = WorldStorage::create(&saves, 12345, "Test World").unwrap();
    let manifest = storage.manifest();
    assert_eq!(manifest.seed, 12345);
    assert_eq!(manifest.name, "Test World");
    assert_eq!(manifest.format_version, FORMAT_VERSION);
    assert!(manifest.created_unix_millis > 0);
}

#[test]
fn weather_and_spawner_mobs_round_trip_without_changing_older_saves() {
    let saves = temp_saves("creatures-weather");
    let storage = WorldStorage::create(&saves, 9, "Creatures").unwrap();
    let mut weather = WorldWeather {
        raining: true,
        thundering: true,
        rain_time: 200,
        thunder_time: 100,
        ..default()
    };
    weather.rain_strength = 1.0;
    storage.set_weather(&weather);
    storage
        .save_player(&StoredPlayer::from_transform(&Transform::default()))
        .unwrap();
    let reopened = WorldStorage::open(storage.root().to_path_buf()).unwrap();
    assert!(reopened.manifest().weather.raining);
    assert_eq!(reopened.manifest().weather.rain_time, 200);
    let player = StoredPlayer::from_transform(&Transform::default()).with_health(7);
    storage.save_player(&player).unwrap();
    assert_eq!(reopened.load_player().unwrap().health, 7);

    let pos = ChunkPosition::ZERO;
    let mut generated = OverworldGenerator::new(9).generate(pos);
    generated.chunk.set(2, 40, 3, Block::MobSpawner);
    let index = Chunk::index(2, 40, 3);
    let spawner = MobSpawner {
        kind: MobType::Skeleton,
        delay: 407,
        rng_state: 42,
    };
    generated.chunk.insert_spawner(index, spawner);
    let mut sheep = Mob::new(MobType::Sheep, 123);
    sheep.sheared = true;
    sheep.health = 7;
    generated.chunk.set_mob_records(vec![MobRecord {
        mob: sheep,
        feet: [2.5, 42.0, 3.5],
        velocity: [0.0, 0.1, 0.0],
        yaw: 135.0,
    }]);
    storage.save_chunk(pos, &generated).unwrap();
    let loaded = reopened.load_chunk(pos).unwrap();
    assert_eq!(*loaded.chunk.spawners().next().unwrap().1, spawner);
    let loaded_mob = &loaded.chunk.mob_records()[0];
    assert_eq!(loaded_mob.mob.kind, MobType::Sheep);
    assert!(loaded_mob.mob.sheared);
    assert_eq!(loaded_mob.mob.health, 7);
    assert_eq!(loaded_mob.feet, [2.5, 42.0, 3.5]);
    assert_eq!(loaded_mob.yaw, 135.0);
}

#[test]
fn chunk_round_trips_through_a_chunk_file() {
    let saves = temp_saves("roundtrip");
    let storage = WorldStorage::create(&saves, 0, "Roundtrip").unwrap();
    let position = ChunkPosition { x: -1, z: 2 };
    let mut generated = OverworldGenerator::new(0).generate(position);
    for (x, facing) in [
        (1, HorizontalFacing::North),
        (2, HorizontalFacing::East),
        (3, HorizontalFacing::South),
        (4, HorizontalFacing::West),
    ] {
        generated.chunk.set_with_metadata(
            x,
            70,
            1,
            Block::Pumpkin,
            Block::Pumpkin.facing_metadata(facing),
        );
    }
    storage.save_chunk(position, &generated).unwrap();

    let loaded = storage.load_chunk(position).expect("chunk should load");
    assert_same_blocks(&loaded.chunk, &generated.chunk);
    for (x, facing) in [
        (1, HorizontalFacing::North),
        (2, HorizontalFacing::East),
        (3, HorizontalFacing::South),
        (4, HorizontalFacing::West),
    ] {
        assert_eq!(loaded.chunk.get(x, 70, 1), Some(Block::Pumpkin));
        assert_eq!(
            Block::Pumpkin.facing(loaded.chunk.metadata(x, 70, 1)),
            Some(facing)
        );
    }
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            assert_eq!(loaded.heightmap.get(x, z), generated.heightmap.get(x, z));
            let before = generated.biomes.get(x, z);
            let after = loaded.biomes.get(x, z);
            assert_eq!(after.biome, before.biome);
            // Climate is stored quantized to a byte, which is lossless for the
            // 256x256 grass and foliage palette lookups.
            assert!((after.temperature - before.temperature).abs() < 0.01);
            assert!((after.humidity - before.humidity).abs() < 0.01);
        }
    }
}

#[test]
fn block_metadata_and_pending_ticks_round_trip_through_a_chunk_file() {
    let saves = temp_saves("metadata");
    let storage = WorldStorage::create(&saves, 0, "Metadata").unwrap();
    let position = ChunkPosition { x: 3, z: -2 };
    let mut generated = OverworldGenerator::new(0).generate(position);
    generated
        .chunk
        .set_with_metadata(4, 90, 5, Block::FlowingWater, 3);
    generated.chunk.set_with_metadata(5, 90, 5, Block::Crops, 7);
    generated
        .chunk
        .set_with_metadata(6, 90, 5, Block::Farmland, 6);
    let tick = PendingTick {
        index: Chunk::index(4, 90, 5) as u16,
        block: Block::FlowingWater,
        delay: 4,
    };
    generated.chunk.set_pending_ticks(vec![tick]);
    storage.save_chunk(position, &generated).unwrap();

    let loaded = storage.load_chunk(position).expect("chunk should load");
    assert_eq!(loaded.chunk.metadata(4, 90, 5), 3);
    assert_eq!(loaded.chunk.metadata(5, 90, 5), 7);
    assert_eq!(loaded.chunk.metadata(6, 90, 5), 6);
    assert_eq!(loaded.chunk.metadata(7, 90, 5), 0);
    assert_eq!(loaded.chunk.pending_ticks(), &[tick]);

    // A chunk without metadata stores none and loads all zeroes.
    let plain = OverworldGenerator::new(0).generate_base(ChunkPosition { x: 4, z: -2 });
    assert!(plain.chunk.raw_metadata().is_none());
    storage
        .save_chunk(ChunkPosition { x: 4, z: -2 }, &plain)
        .unwrap();
    let loaded = storage.load_chunk(ChunkPosition { x: 4, z: -2 }).unwrap();
    assert!(loaded.chunk.raw_metadata().is_none());
    fs::remove_dir_all(saves).unwrap();
}

#[test]
fn chunks_saved_with_block_ids_for_species_and_facing_are_rejected() {
    assert_eq!(Block::Cake.as_u8(), 92);
    assert!(!Block::Cake.in_world());
    let saves = temp_saves("old-format");
    let storage = WorldStorage::create(&saves, 0, "Old").unwrap();
    let position = ChunkPosition::ZERO;
    let generated = OverworldGenerator::new(0).generate(position);
    storage.save_chunk(position, &generated).unwrap();

    let path = storage
        .root()
        .join(region_dir_name(region_of(position)))
        .join(chunk_file_name(position));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["format_version"], CHUNK_FORMAT_VERSION);
    // A version 1 chunk could hold ids 200..=230, which are not blocks now.
    value["format_version"] = serde_json::json!(1);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(storage.load_chunk(position).is_none());

    // Ids outside Beta's range are rejected even under the current version.
    let blocks = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
    value["format_version"] = serde_json::json!(CHUNK_FORMAT_VERSION);
    value["runs"] = serde_json::json!([[200, blocks as u16]]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(storage.load_chunk(position).is_none());

    value["runs"] = serde_json::json!([[20, blocks as u16]]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(storage.load_chunk(position).is_none());
}

#[test]
fn dropped_items_round_trip_inside_their_chunk() {
    let saves = temp_saves("items");
    let storage = WorldStorage::create(&saves, 0, "Items").unwrap();
    let position = ChunkPosition { x: 1, z: -1 };
    let mut generated = OverworldGenerator::new(0).generate(position);
    generated.items.push(ChunkDroppedItem {
        stack: ItemStack::from_block(Block::Cobblestone, 3).unwrap(),
        position: [20.25, 70.0, -8.5],
        motion: [0.05, 0.2, -0.08],
        age_ticks: 12,
        pickup_delay_ticks: 10,
        hover_start: 1.25,
        rng_state: 99,
    });
    storage.save_chunk(position, &generated).unwrap();
    let loaded = storage.load_chunk(position).unwrap();
    assert_eq!(loaded.items.len(), 1);
    let item = &loaded.items[0];
    assert_eq!(item.stack.count(), 3);
    assert_eq!(item.position, [20.25, 70.0, -8.5]);
    assert_eq!(item.motion, [0.05, 0.2, -0.08]);
    assert_eq!(item.age_ticks, 12);
    assert_eq!(item.pickup_delay_ticks, 10);
    assert!((item.hover_start - 1.25).abs() < 1e-5);
    assert_eq!(item.rng_state, 99);

    let path = storage
        .root()
        .join(region_dir_name(region_of(position)))
        .join(chunk_file_name(position));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("items");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let legacy = storage
        .load_chunk(position)
        .expect("chunks without items still load");
    assert!(legacy.items.is_empty());
}

#[test]
fn region_folders_group_sixteen_by_sixteen_chunks() {
    let saves = temp_saves("regions");
    let storage = WorldStorage::create(&saves, 0, "Regions").unwrap();
    let generator = OverworldGenerator::new(0);
    let positions = [
        ChunkPosition { x: 0, z: 0 },
        ChunkPosition { x: 15, z: 15 },
        ChunkPosition { x: 16, z: 0 },
        ChunkPosition { x: -1, z: -1 },
        ChunkPosition { x: -16, z: -16 },
        ChunkPosition { x: -17, z: 0 },
    ];
    for position in positions {
        storage
            .save_chunk(position, &generator.generate(position))
            .unwrap();
    }

    assert_eq!(REGION_SIZE, 16);
    assert_eq!(region_of(ChunkPosition { x: 15, z: 15 }), (0, 0));
    assert_eq!(region_of(ChunkPosition { x: 16, z: 0 }), (1, 0));
    assert_eq!(region_of(ChunkPosition { x: -1, z: -1 }), (-1, -1));
    assert_eq!(region_of(ChunkPosition { x: -17, z: 0 }), (-2, 0));

    for position in positions {
        let path = storage
            .root()
            .join(region_dir_name(region_of(position)))
            .join(chunk_file_name(position));
        assert!(path.is_file(), "missing {}", path.display());
    }
}

#[test]
fn saving_one_chunk_keeps_the_others_in_its_region() {
    let saves = temp_saves("merge");
    let storage = WorldStorage::create(&saves, 0, "Merge").unwrap();
    let generator = OverworldGenerator::new(0);
    let first = ChunkPosition { x: 1, z: 1 };
    let second = ChunkPosition { x: 2, z: 2 };
    storage
        .save_chunk(first, &generator.generate(first))
        .unwrap();
    storage
        .save_chunk(second, &generator.generate(second))
        .unwrap();

    // Rewrite the first chunk with an edit and confirm the second is untouched.
    let mut edited = generator.generate(first);
    edited.chunk.set(4, 70, 4, Block::GoldBlock);
    storage.save_chunk(first, &edited).unwrap();

    let reloaded = storage.load_chunk(first).unwrap();
    assert_eq!(reloaded.chunk.get(4, 70, 4), Some(Block::GoldBlock));
    assert_same_blocks(
        &storage.load_chunk(second).unwrap().chunk,
        &generator.generate(second).chunk,
    );
}

#[test]
fn save_chunks_writes_every_chunk_once() {
    let saves = temp_saves("batch");
    let storage = WorldStorage::create(&saves, 0, "Batch").unwrap();
    let generator = OverworldGenerator::new(0);
    let positions = [
        ChunkPosition { x: 0, z: 0 },
        ChunkPosition { x: 1, z: 0 },
        ChunkPosition { x: 16, z: 0 },
    ];
    let chunks: Vec<_> = positions
        .iter()
        .map(|position| (*position, generator.generate(*position)))
        .collect();
    let batch: Vec<_> = chunks
        .iter()
        .map(|(position, chunk)| (*position, chunk))
        .collect();

    assert_eq!(storage.save_chunks(batch).unwrap(), 3);
    for position in positions {
        assert!(storage.load_chunk(position).is_some());
    }
}

#[test]
fn missing_chunks_load_as_none() {
    let saves = temp_saves("missing");
    let storage = WorldStorage::create(&saves, 0, "Missing").unwrap();
    assert!(storage.load_chunk(ChunkPosition { x: 3, z: 3 }).is_none());
}

#[test]
fn open_latest_or_create_resumes_the_newest_world() {
    let saves = temp_saves("resume");
    WorldStorage::create(&saves, 7, "First").unwrap();
    // Creation times are stored in milliseconds, so separate the two worlds.
    thread::sleep(Duration::from_millis(10));
    let second = WorldStorage::create(&saves, 9, "Second").unwrap();

    let resumed = WorldStorage::open_latest_or_create(&saves, 0).unwrap();
    assert_eq!(resumed.root(), second.root());
    assert_eq!(resumed.seed(), 9);
    assert_eq!(resumed.manifest().name, "Second");
}

#[test]
fn open_latest_or_create_makes_a_world_when_none_exists() {
    let saves = temp_saves("empty");
    let storage = WorldStorage::open_latest_or_create(&saves, 42).unwrap();
    assert_eq!(storage.seed(), 42);
    assert!(storage.root().join("level.json").is_file());
}

#[test]
fn player_pose_round_trips_through_player_json() {
    let saves = temp_saves("player");
    let storage = WorldStorage::create(&saves, 0, "Player").unwrap();
    let transform = Transform {
        translation: Vec3::new(32.5, 71.0, -12.25),
        rotation: Quat::from_euler(EulerRot::YXZ, 1.2, -0.4, 0.0),
        ..default()
    };
    storage
        .save_player(&StoredPlayer::from_transform(&transform))
        .unwrap();

    assert!(storage.root().join("player.json").is_file());
    let loaded = storage.load_player().expect("player should load");
    assert_eq!(loaded.format_version, FORMAT_VERSION);
    assert!((loaded.x - 32.5).abs() < f32::EPSILON);
    assert!((loaded.y - 71.0).abs() < f32::EPSILON);
    assert!((loaded.z - -12.25).abs() < f32::EPSILON);
    let restored = loaded.to_transform();
    assert!(
        restored
            .translation
            .abs_diff_eq(transform.translation, 0.001)
    );
    let (yaw, pitch, _) = restored.rotation.to_euler(EulerRot::YXZ);
    assert!((yaw - 1.2).abs() < 0.001);
    assert!((pitch - -0.4).abs() < 0.001);
}

#[test]
fn the_world_is_saved_and_resumed_across_runs() {
    let saves = temp_saves("app");

    let mut first = persistence_app(&saves);
    first.world_mut().spawn((
        Player,
        Transform {
            translation: Vec3::new(48.0, 72.0, -24.0),
            rotation: Quat::from_euler(EulerRot::YXZ, 0.75, -0.2, 0.0),
            ..default()
        },
    ));
    assert!(run_until(&mut first, Duration::from_secs(5), |app| {
        app.world()
            .resource::<WorldChunks>()
            .get(ChunkPosition::ZERO)
            .is_some()
    }));

    let root = first
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled")
        .root()
        .to_path_buf();

    // Exiting flushes the dirty chunks.
    first.world_mut().write_message(AppExit::Success);
    first.update();
    let chunk = root
        .join(region_dir_name((0, 0)))
        .join(chunk_file_name(ChunkPosition::ZERO));
    assert!(chunk.is_file(), "missing {}", chunk.display());
    assert!(
        root.join("player.json").is_file(),
        "missing {}",
        root.join("player.json").display()
    );

    // A second run resumes the same world and reads the saved chunk and player.
    let mut second = persistence_app(&saves);
    second.update();
    let storage = second
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled");
    assert_eq!(storage.root(), root.as_path());
    assert!(storage.load_chunk(ChunkPosition::ZERO).is_some());
    let player = storage.load_player().expect("player should load");
    assert!((player.x - 48.0).abs() < 0.001);
    assert!((player.y - 72.0).abs() < 0.001);
    assert!((player.z - -24.0).abs() < 0.001);
    assert!((player.yaw - 0.75).abs() < 0.001);
    assert!((player.pitch - -0.2).abs() < 0.001);
}

#[test]
fn exit_requested_during_update_saves_the_latest_player_pose() {
    fn request_exit(mut exit: MessageWriter<AppExit>) {
        exit.write(AppExit::Success);
    }

    let saves = temp_saves("exit-pose");
    let mut app = persistence_app(&saves);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(91.0, 73.0, -17.0)));
    app.add_systems(Update, request_exit);

    app.update();

    let storage = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled");
    let player = storage
        .load_player()
        .expect("player should be saved on exit");
    assert!((player.x - 91.0).abs() < 0.001);
    assert!((player.y - 73.0).abs() < 0.001);
    assert!((player.z - -17.0).abs() < 0.001);
}

/// A headless app whose autosave fires every frame, so a test does not have to
/// simulate half a minute of world time to reach a save.
fn draining_app(saves: &Path) -> App {
    app_with(PersistencePlugin::new(saves.to_path_buf()).with_autosave(0.0))
}

/// Run until the spawn chunk is loaded, which is what makes it worth saving.
fn run_until_spawn_chunk(app: &mut App) {
    assert!(run_until(app, Duration::from_secs(10), |app| {
        app.world()
            .resource::<WorldChunks>()
            .get(ChunkPosition::ZERO)
            .is_some()
    }));
}

#[test]
fn an_autosave_drain_reaches_disk_over_several_frames() {
    let saves = temp_saves("drain");
    let mut app = draining_app(&saves);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 72.0, 8.0)));
    run_until_spawn_chunk(&mut app);

    // The save is spread over the frames that follow, so nothing but the edit
    // and the drain itself can put this block on disk. No exit flush is used.
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let edited = chunks
            .get_mut(ChunkPosition::ZERO)
            .expect("spawn chunk should be loaded");
        assert_ne!(edited.chunk.get(4, 120, 4), Some(Block::GoldBlock));
        edited.chunk.set(4, 120, 4, Block::GoldBlock);
    }
    app.world_mut()
        .resource_mut::<WorldPersistence>()
        .mark_dirty(ChunkPosition::ZERO);

    let storage = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled")
        .clone();
    let saved = || {
        storage
            .load_chunk(ChunkPosition::ZERO)
            .is_some_and(|chunk| chunk.chunk.get(4, 120, 4) == Some(Block::GoldBlock))
    };
    assert!(
        run_until(&mut app, Duration::from_secs(20), |_| saved()),
        "the drain never wrote the edited chunk"
    );
    assert!(
        storage.load_player().is_some(),
        "the player should ride along with the first batch"
    );
}

#[test]
fn a_save_request_drains_without_waiting_for_the_autosave_timer() {
    let saves = temp_saves("request");
    // The default timer is a minute, so only the request can save in time.
    let mut app = persistence_app(&saves);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 72.0, 8.0)));
    run_until_spawn_chunk(&mut app);

    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let edited = chunks
            .get_mut(ChunkPosition::ZERO)
            .expect("spawn chunk should be loaded");
        edited.chunk.set(4, 120, 4, Block::GoldBlock);
    }
    let mut persistence = app.world_mut().resource_mut::<WorldPersistence>();
    persistence.mark_dirty(ChunkPosition::ZERO);
    persistence.request_save();
    let storage = persistence
        .storage()
        .expect("persistence should be enabled")
        .clone();

    assert!(
        run_until(&mut app, Duration::from_secs(20), |_| {
            storage
                .load_chunk(ChunkPosition::ZERO)
                .is_some_and(|chunk| chunk.chunk.get(4, 120, 4) == Some(Block::GoldBlock))
        }),
        "the requested save never wrote the edited chunk"
    );
}

#[test]
fn a_drain_spreads_more_chunks_than_one_frame_can_hold() {
    let saves = temp_saves("spread");
    let mut app = draining_app(&saves);
    run_until_spawn_chunk(&mut app);

    // Well outside the streaming radius, so only the save path touches these.
    let positions: Vec<ChunkPosition> = (0..40)
        .map(|index| ChunkPosition {
            x: 500 + index,
            z: -500,
        })
        .collect();
    let generator = OverworldGenerator::new(0);
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        for position in &positions {
            chunks.insert(*position, generator.generate(*position));
        }
        let mut persistence = app.world_mut().resource_mut::<WorldPersistence>();
        for position in &positions {
            persistence.mark_dirty(*position);
        }
    }

    let storage = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled")
        .clone();
    assert!(
        run_until(&mut app, Duration::from_secs(60), |_| positions
            .iter()
            .all(|position| storage.load_chunk(*position).is_some())),
        "the drain left chunks behind"
    );
}

#[test]
fn a_chunk_unloads_without_the_mobs_that_left_it_since_the_autosave() {
    let saves = temp_saves("mob-unload");
    let mut app = draining_app(&saves);
    let player = app
        .world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 100.0, 8.0)))
        .id();
    run_until_spawn_chunk(&mut app);
    assert!(run_until(&mut app, Duration::from_secs(30), |app| {
        let streaming = app
            .world()
            .resource::<game::world::streaming::WorldStreaming>();
        streaming.generating_job_count() == 0 && streaming.populating_job_count() == 0
    }));

    // The autosave writes a pig into the spawn chunk's records.
    let ground = (0..CHUNK_HEIGHT as i32)
        .rev()
        .find(|&y| {
            app.world()
                .resource::<WorldChunks>()
                .block_at(8, y, 8)
                .is_some_and(|block| block != Block::Air)
        })
        .unwrap();
    super::mobs::summon(
        &mut app,
        Mob::new(MobType::Pig, 1),
        Vec3::new(8.5, (ground + 1) as f32, 8.5),
    );
    let storage = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled")
        .clone();
    let saved_mobs = || {
        storage
            .load_chunk(ChunkPosition::ZERO)
            .map_or(0, |chunk| chunk.chunk.mob_records().len())
    };
    assert!(run_until(
        &mut app,
        Duration::from_secs(20),
        |_| saved_mobs() > 0
    ));

    // Every mob dies, then the player leaves before another autosave runs.
    let mobs: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<Mob>>()
        .iter(app.world())
        .collect();
    for mob in mobs {
        app.world_mut().despawn(mob);
    }
    app.world_mut()
        .get_mut::<Transform>(player)
        .unwrap()
        .translation
        .x = 3200.0;
    assert!(
        run_until(&mut app, Duration::from_secs(20), |app| {
            !app.world()
                .resource::<WorldChunks>()
                .contains(ChunkPosition::ZERO)
                && saved_mobs() == 0
        }),
        "the unloaded chunk kept {} mobs from the last autosave",
        saved_mobs()
    );
}

#[test]
fn a_chunk_unloaded_while_its_write_is_in_flight_keeps_the_newer_edit() {
    let saves = temp_saves("in-flight");
    let mut app = draining_app(&saves);
    run_until_spawn_chunk(&mut app);

    // Submit a write of the unedited chunk, then unload and edit it before that
    // write lands. Exiting has to wait for the in-flight write and then save the
    // newer unloaded copy, or the stale snapshot would win.
    app.world_mut()
        .resource_mut::<WorldPersistence>()
        .mark_dirty(ChunkPosition::ZERO);
    app.update();
    assert!(
        app.world().resource::<WorldPersistence>().write_in_flight(),
        "the pump should have handed a batch to the writer"
    );

    let mut chunk = app
        .world_mut()
        .resource_mut::<WorldChunks>()
        .remove(ChunkPosition::ZERO)
        .expect("spawn chunk should be loaded");
    chunk.chunk.set(4, 120, 4, Block::GoldBlock);
    app.world_mut()
        .resource_mut::<WorldPersistence>()
        .queue_unload(ChunkPosition::ZERO, chunk);

    app.world_mut().write_message(AppExit::Success);
    app.update();

    let storage = app
        .world()
        .resource::<WorldPersistence>()
        .storage()
        .expect("persistence should be enabled");
    let reloaded = storage
        .load_chunk(ChunkPosition::ZERO)
        .expect("the chunk should be on disk");
    assert_eq!(reloaded.chunk.get(4, 120, 4), Some(Block::GoldBlock));
}

#[test]
fn list_worlds_returns_every_world_newest_played_first_with_its_difficulty() {
    use game::app::settings::Difficulty;
    use game::world::persistence::list_worlds;

    let saves = temp_saves("list");
    let first = WorldStorage::create_with(&saves, 1, "First", Some(Difficulty::Hard)).unwrap();
    thread::sleep(Duration::from_millis(5));
    let second = WorldStorage::create(&saves, 2, "Second").unwrap();
    // A folder that is not a world must not appear in the list.
    fs::create_dir_all(saves.join("not-a-world")).unwrap();

    let worlds = list_worlds(&saves);
    assert_eq!(worlds.len(), 2);
    assert_eq!(worlds[0].root, second.root());
    assert_eq!(worlds[0].manifest.name, "Second");
    assert_eq!(worlds[0].manifest.difficulty, None);
    assert_eq!(worlds[1].root, first.root());
    assert_eq!(worlds[1].manifest.seed, 1);
    assert_eq!(worlds[1].manifest.difficulty, Some(Difficulty::Hard));
    assert!(list_worlds(&saves.join("missing")).is_empty());
}

#[test]
fn a_deferred_persistence_plugin_loads_no_world_until_one_is_chosen() {
    let saves = temp_saves("deferred");
    let mut app = app_with(PersistencePlugin::new(saves.clone()).deferred());
    app.update();

    assert!(app.world().get_resource::<WorldPersistence>().is_none());
    assert!(
        app.world()
            .get_resource::<game::world::streaming::WorldStreaming>()
            .is_none()
    );
    assert!(!saves.join("level.json").exists());
    assert!(
        fs::read_dir(&saves).unwrap().next().is_none(),
        "no world folder should be created at startup"
    );
}

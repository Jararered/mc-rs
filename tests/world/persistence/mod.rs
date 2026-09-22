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
use game::item::ItemStack;
use game::player::Player;
use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::ChunkDroppedItem;
use game::world::generation::WorldGenerator;
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
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(WorldPlugin)
        .add_plugins(PersistencePlugin::new(saves.to_path_buf()));
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
fn chunk_round_trips_through_a_chunk_file() {
    let saves = temp_saves("roundtrip");
    let storage = WorldStorage::create(&saves, 0, "Roundtrip").unwrap();
    let position = ChunkPos { x: -1, z: 2 };
    let generated = WorldGenerator::new(0).generate(position);
    storage.save_chunk(position, &generated).unwrap();

    let loaded = storage.load_chunk(position).expect("chunk should load");
    assert_same_blocks(&loaded.chunk, &generated.chunk);
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
fn legacy_species_bytes_stay_spruce_while_cake_uses_the_same_number() {
    assert_eq!(BlockId::Cake.as_u8(), 92);
    assert!(!BlockId::Cake.in_world());
    let saves = temp_saves("legacy-spruce");
    let storage = WorldStorage::create(&saves, 0, "Legacy").unwrap();
    let position = ChunkPos::ZERO;
    let generated = WorldGenerator::new(0).generate(position);
    storage.save_chunk(position, &generated).unwrap();

    let path = storage
        .root()
        .join(region_dir_name(region_of(position)))
        .join(chunk_file_name(position));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let blocks = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
    value["runs"] = serde_json::json!([[92, blocks as u16]]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let loaded = storage
        .load_chunk(position)
        .expect("legacy spruce byte still loads");
    assert!(
        loaded
            .chunk
            .blocks()
            .iter()
            .all(|block| *block == BlockId::SpruceLeaves)
    );

    value["runs"] = serde_json::json!([[20, blocks as u16]]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(storage.load_chunk(position).is_none());
}

#[test]
fn dropped_items_round_trip_inside_their_chunk() {
    let saves = temp_saves("items");
    let storage = WorldStorage::create(&saves, 0, "Items").unwrap();
    let position = ChunkPos { x: 1, z: -1 };
    let mut generated = WorldGenerator::new(0).generate(position);
    generated.items.push(ChunkDroppedItem {
        stack: ItemStack::from_block(BlockId::Cobblestone, 3).unwrap(),
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
    let generator = WorldGenerator::new(0);
    let positions = [
        ChunkPos { x: 0, z: 0 },
        ChunkPos { x: 15, z: 15 },
        ChunkPos { x: 16, z: 0 },
        ChunkPos { x: -1, z: -1 },
        ChunkPos { x: -16, z: -16 },
        ChunkPos { x: -17, z: 0 },
    ];
    for position in positions {
        storage
            .save_chunk(position, &generator.generate(position))
            .unwrap();
    }

    assert_eq!(REGION_SIZE, 16);
    assert_eq!(region_of(ChunkPos { x: 15, z: 15 }), (0, 0));
    assert_eq!(region_of(ChunkPos { x: 16, z: 0 }), (1, 0));
    assert_eq!(region_of(ChunkPos { x: -1, z: -1 }), (-1, -1));
    assert_eq!(region_of(ChunkPos { x: -17, z: 0 }), (-2, 0));

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
    let generator = WorldGenerator::new(0);
    let first = ChunkPos { x: 1, z: 1 };
    let second = ChunkPos { x: 2, z: 2 };
    storage
        .save_chunk(first, &generator.generate(first))
        .unwrap();
    storage
        .save_chunk(second, &generator.generate(second))
        .unwrap();

    // Rewrite the first chunk with an edit and confirm the second is untouched.
    let mut edited = generator.generate(first);
    edited.chunk.set(4, 70, 4, BlockId::GoldBlock);
    storage.save_chunk(first, &edited).unwrap();

    let reloaded = storage.load_chunk(first).unwrap();
    assert_eq!(reloaded.chunk.get(4, 70, 4), Some(BlockId::GoldBlock));
    assert_same_blocks(
        &storage.load_chunk(second).unwrap().chunk,
        &generator.generate(second).chunk,
    );
}

#[test]
fn save_chunks_writes_every_chunk_once() {
    let saves = temp_saves("batch");
    let storage = WorldStorage::create(&saves, 0, "Batch").unwrap();
    let generator = WorldGenerator::new(0);
    let positions = [
        ChunkPos { x: 0, z: 0 },
        ChunkPos { x: 1, z: 0 },
        ChunkPos { x: 16, z: 0 },
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
    assert!(storage.load_chunk(ChunkPos { x: 3, z: 3 }).is_none());
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
            .get(ChunkPos::ZERO)
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
        .join(chunk_file_name(ChunkPos::ZERO));
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
    assert!(storage.load_chunk(ChunkPos::ZERO).is_some());
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

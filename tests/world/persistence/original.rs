//! The Beta 1.7.3 save format: files Beta can read, worlds Beta wrote, and the
//! chunk, player and level data moving through both.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use bevy::prelude::*;
use flate2::read::GzDecoder;
use flate2::read::ZlibDecoder;
use game::block::blocks::Block;
use game::block::direction::Direction;
use game::entity::SavedBody;
use game::entity::SavedSlot;
use game::entity::minecart::Cargo;
use game::entity::minecart::CartKind;
use game::entity::mobs::Mob;
use game::entity::mobs::MobRecord;
use game::entity::mobs::MobSpawner;
use game::entity::mobs::MobType;
use game::inventory::Hotbar;
use game::inventory::Inventory;
use game::item::Item;
use game::item::ItemStack;
use game::player::LocalPlayer;
use game::world::biome::Biome;
use game::world::chest::CHEST_SLOTS;
use game::world::chest::Chest;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkDroppedItem;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::WorldChunks;
use game::world::difficulty::Difficulty;
use game::world::dimension::Dimension;
use game::world::furnace::Furnace;
use game::world::generation::ChunkGenerator;
use game::world::generation::nether::NetherGenerator;
use game::world::generation::overworld::OverworldGenerator;
use game::world::persistence::SaveFormat;
use game::world::persistence::StoredPlayer;
use game::world::persistence::StoredStack;
use game::world::persistence::WorldPersistence;
use game::world::persistence::WorldStorage;
use game::world::persistence::delete_world;
use game::world::persistence::list_worlds;

use super::assert_same_blocks;
use super::persistence_app;
use super::run_until;
use super::temp_saves;
use crate::world::generation::overworld::beta_reference;

const SEED: u64 = 7;

fn original_world(label: &str) -> (PathBuf, WorldStorage) {
    let saves = temp_saves(label);
    let storage = WorldStorage::create_in_format(
        &saves,
        SEED,
        "Beta World",
        Some(Difficulty::Hard),
        SaveFormat::Original,
    )
    .unwrap();
    (saves, storage)
}

fn stack(item: Item, count: u8) -> ItemStack {
    ItemStack::new(item, count).unwrap()
}

fn stored(item: Item, count: u8) -> StoredStack {
    StoredStack {
        id: item.as_u16(),
        count,
        data: 0,
    }
}

fn gunzip(path: &Path) -> Vec<u8> {
    let mut out = Vec::new();
    GzDecoder::new(fs::read(path).unwrap().as_slice())
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn assert_same_metadata(left: &Chunk, right: &Chunk) {
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert_eq!(
                    left.metadata(x, y, z),
                    right.metadata(x, y, z),
                    "metadata at {x},{y},{z}"
                );
            }
        }
    }
}

/// A chunk with something of everything the format carries.
fn busy_chunk(position: ChunkPosition) -> GeneratedChunk {
    let mut generated = OverworldGenerator::new(SEED).generate(position);
    let chunk = &mut generated.chunk;
    chunk.set_with_metadata(
        1,
        70,
        1,
        Block::Pumpkin,
        Block::Pumpkin.facing_metadata(Direction::East),
    );

    chunk.set(5, 71, 5, Block::Chest);
    let mut chest = Chest::default();
    chest.slots[0] = Some(stack(Item::Stick, 3));
    chest.slots[CHEST_SLOTS - 1] = Some(ItemStack::with_data(Item::IronShovel, 1, 10).unwrap());
    chunk.insert_chest(Chunk::index(5, 71, 5), chest);

    chunk.set(6, 71, 5, Block::Furnace);
    chunk.insert_furnace(
        Chunk::index(6, 71, 5),
        Furnace {
            slots: [
                Some(stack(
                    Item::from_u16(u16::from(Block::IronOre.as_u8())).unwrap(),
                    5,
                )),
                Some(stack(Item::Coal, 7)),
                Some(stack(Item::IronIngot, 2)),
            ],
            burn_ticks: 120,
            fuel_ticks: 1600,
            cook_ticks: 33,
        },
    );

    chunk.set(8, 72, 8, Block::MobSpawner);
    chunk.insert_spawner(
        Chunk::index(8, 72, 8),
        MobSpawner {
            kind: MobType::Zombie,
            delay: 40,
            rng_state: MobSpawner::default().rng_state,
        },
    );

    let mut sheep = Mob::new(MobType::Sheep, 9);
    sheep.variant = 5;
    sheep.sheared = true;
    sheep.health = 8;
    let mut wolf = Mob::new(MobType::Wolf, 11);
    wolf.tamed = true;
    wolf.owner = Some("Steve".to_owned());
    wolf.sitting = true;
    chunk.set_mob_records(vec![
        MobRecord {
            mob: sheep,
            feet: [
                position.x as f32 * 16.0 + 2.5,
                80.0,
                position.z as f32 * 16.0 + 3.5,
            ],
            velocity: [0.0, 0.0, 0.0],
            yaw: 90.0,
        },
        MobRecord {
            mob: wolf,
            feet: [
                position.x as f32 * 16.0 + 9.5,
                80.0,
                position.z as f32 * 16.0 + 9.5,
            ],
            velocity: [0.1, 0.0, -0.1],
            yaw: 270.0,
        },
    ]);

    let mut cargo = Cargo::default();
    cargo.0[3] = Some(stack(Item::Stick, 5));
    chunk.set_saved_bodies(vec![
        SavedBody::Minecart {
            center: [
                position.x as f32 * 16.0 + 6.5,
                64.5,
                position.z as f32 * 16.0 + 6.5,
            ],
            motion: [0.25, 0.0, 0.0],
            kind: CartKind::Chest,
            fuel: 0,
            push: [0.0; 2],
            cargo: SavedSlot::pack(&cargo),
        },
        SavedBody::Minecart {
            center: [
                position.x as f32 * 16.0 + 7.5,
                64.5,
                position.z as f32 * 16.0 + 6.5,
            ],
            motion: [0.0; 3],
            kind: CartKind::Furnace,
            fuel: 600,
            push: [1.0, 0.0],
            cargo: Vec::new(),
        },
        SavedBody::Boat {
            center: [
                position.x as f32 * 16.0 + 9.5,
                63.25,
                position.z as f32 * 16.0 + 6.5,
            ],
            motion: [0.0, 0.0, 0.125],
            yaw: 90.0,
        },
    ]);

    generated.items.push(ChunkDroppedItem {
        stack: stack(Item::Stick, 3),
        position: [
            position.x as f32 * 16.0 + 4.5,
            71.0,
            position.z as f32 * 16.0 + 4.5,
        ],
        motion: [0.0, 0.1, 0.0],
        age_ticks: 100,
        pickup_delay_ticks: 0,
        hover_start: 0.0,
        rng_state: 1,
        health: 3,
        fire: 40,
    });
    generated
}

fn assert_busy_chunk_loaded(
    loaded: &GeneratedChunk,
    original: &GeneratedChunk,
    position: ChunkPosition,
) {
    assert_same_blocks(&loaded.chunk, &original.chunk);
    assert_same_metadata(&loaded.chunk, &original.chunk);
    assert_eq!(loaded.populated, original.populated);

    let chest = loaded.chunk.chest(Chunk::index(5, 71, 5)).expect("chest");
    assert_eq!(
        chest,
        original.chunk.chest(Chunk::index(5, 71, 5)).unwrap(),
        "chest contents"
    );

    let furnace = loaded
        .chunk
        .furnace(Chunk::index(6, 71, 5))
        .expect("furnace");
    assert_eq!(
        furnace.slots,
        original
            .chunk
            .furnace(Chunk::index(6, 71, 5))
            .unwrap()
            .slots
    );
    assert_eq!(furnace.burn_ticks, 120);
    assert_eq!(furnace.cook_ticks, 33);
    // Beta stores only the time left; the burn length comes from the fuel.
    assert_eq!(furnace.fuel_ticks, 1600);

    let spawner = loaded
        .chunk
        .spawners()
        .find(|(index, _)| *index == Chunk::index(8, 72, 8))
        .map(|(_, spawner)| *spawner)
        .expect("spawner");
    assert_eq!(spawner.kind, MobType::Zombie);
    assert_eq!(spawner.delay, 40);

    let mobs = loaded.chunk.mob_records();
    assert_eq!(mobs.len(), 2);
    let sheep = mobs.iter().find(|m| m.mob.kind == MobType::Sheep).unwrap();
    assert_eq!(sheep.mob.variant, 5);
    assert!(sheep.mob.sheared);
    assert_eq!(sheep.mob.health, 8);
    assert!((sheep.yaw - 90.0).abs() < 0.01);
    assert!((sheep.feet[0] - (position.x as f32 * 16.0 + 2.5)).abs() < 0.001);
    let wolf = mobs.iter().find(|m| m.mob.kind == MobType::Wolf).unwrap();
    assert!(wolf.mob.tamed && wolf.mob.sitting && !wolf.mob.angry);
    assert_eq!(wolf.mob.owner.as_deref(), Some("Steve"));
    assert!((wolf.velocity[0] - 0.1).abs() < 0.001);

    let carts = loaded.chunk.saved_bodies();
    assert_eq!(carts.len(), 3);
    let boat = carts.iter().find_map(|body| match body {
        SavedBody::Boat {
            center,
            motion,
            yaw,
        } => Some((*center, *motion, *yaw)),
        _ => None,
    });
    let (center, motion, yaw) = boat.expect("boat");
    assert!((center[1] - 63.25).abs() < 0.001);
    assert!((motion[2] - 0.125).abs() < 0.001);
    assert!((yaw - 90.0).abs() < 0.001);
    let cart = |wanted: CartKind| {
        carts.iter().find_map(|body| match body {
            SavedBody::Minecart {
                center,
                motion,
                kind,
                fuel,
                push,
                cargo,
            } if *kind == wanted => Some((*center, *motion, *fuel, *push, cargo.clone())),
            _ => None,
        })
    };
    let (center, motion, _, _, cargo) = cart(CartKind::Chest).expect("chest cart");
    assert!((center[0] - (position.x as f32 * 16.0 + 6.5)).abs() < 0.001);
    assert!((motion[0] - 0.25).abs() < 0.001);
    assert_eq!(
        SavedSlot::unpack(&cargo).0[3],
        Some(stack(Item::Stick, 5)),
        "cargo"
    );
    let (_, _, fuel, push, cargo) = cart(CartKind::Furnace).expect("furnace cart");
    assert_eq!((fuel, push), (600, [1.0, 0.0]));
    assert!(cargo.is_empty());

    assert_eq!(loaded.items.len(), 1);
    let item = &loaded.items[0];
    assert_eq!(item.stack, stack(Item::Stick, 3));
    assert_eq!(item.age_ticks, 100);
    assert_eq!((item.health, item.fire), (3, 40));
    assert!((item.position[0] - (position.x as f32 * 16.0 + 4.5)).abs() < 0.001);
}

#[test]
fn a_new_original_world_has_the_files_beta_expects() {
    let (saves, storage) = original_world("original-files");
    let root = storage.root().to_path_buf();

    let lock = fs::read(root.join("session.lock")).unwrap();
    assert_eq!(lock.len(), 8, "session.lock holds one big-endian long");
    assert!(root.join("region").is_dir());
    assert!(!root.join("level.json").exists());

    // gzip NBT whose root is an unnamed compound holding `Data`.
    let level = gunzip(&root.join("level.dat"));
    assert_eq!(&level[..3], &[10, 0, 0], "unnamed compound root");
    for key in [
        &b"Data"[..],
        b"RandomSeed",
        b"LevelName",
        b"SpawnX",
        b"Time",
        b"version",
    ] {
        assert!(
            contains(&level, key),
            "level.dat lacks {}",
            String::from_utf8_lossy(key)
        );
    }

    let worlds = list_worlds(&saves);
    assert_eq!(worlds.len(), 1);
    assert_eq!(worlds[0].manifest.format, SaveFormat::Original);
    assert_eq!(worlds[0].manifest.name, "Beta World");
    assert_eq!(worlds[0].manifest.seed, SEED);
}

#[test]
fn an_original_world_reopens_with_its_manifest() {
    let (_saves, storage) = original_world("original-manifest");
    storage.set_world_time(12_345);
    storage.set_weather(&game::world::weather::WorldWeather {
        raining: true,
        rain_time: 777,
        thundering: true,
        thunder_time: 99,
        ..default()
    });
    storage
        .save_chunk(
            ChunkPosition::ZERO,
            &OverworldGenerator::new(SEED).generate(ChunkPosition::ZERO),
        )
        .unwrap();

    let reopened = WorldStorage::open(storage.root().to_path_buf()).unwrap();
    let manifest = reopened.manifest();
    assert_eq!(reopened.format(), SaveFormat::Original);
    assert_eq!(manifest.seed, SEED);
    assert_eq!(manifest.name, "Beta World");
    assert_eq!(manifest.world_time, 12_345);
    assert!(manifest.weather.raining && manifest.weather.thundering);
    assert_eq!(manifest.weather.rain_time, 777);
    assert_eq!(manifest.weather.thunder_time, 99);
    assert_eq!(manifest.difficulty, Some(Difficulty::Hard));
    assert!(manifest.last_played_unix_millis >= manifest.created_unix_millis);
}

#[test]
fn chunks_round_trip_through_region_files_across_regions() {
    let (_saves, storage) = original_world("original-chunks");
    // Three regions, two with negative coordinates, floor-divided like Beta.
    let positions = [
        ChunkPosition { x: -1, z: 2 },
        ChunkPosition { x: 0, z: 0 },
        ChunkPosition { x: 32, z: -33 },
    ];
    let originals: Vec<_> = positions.iter().map(|p| busy_chunk(*p)).collect();
    storage
        .save_chunks(positions.iter().copied().zip(originals.iter()))
        .unwrap();

    for name in ["r.-1.0.mcr", "r.0.0.mcr", "r.1.-2.mcr"] {
        let path = storage.root().join("region").join(name);
        let length = fs::metadata(&path)
            .unwrap_or_else(|_| panic!("missing {name}"))
            .len();
        assert_eq!(length % 4096, 0, "{name} is whole sectors");
    }

    for (position, original) in positions.iter().zip(&originals) {
        let loaded = storage.load_chunk(*position).expect("chunk should load");
        assert_busy_chunk_loaded(&loaded, original, *position);
        // Beta stores no biomes, so they come back from the seed.
        let expected = OverworldGenerator::new(SEED).generate(*position).biomes;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert_eq!(loaded.biomes.get(x, z).biome, expected.get(x, z).biome);
            }
        }
    }
    assert!(storage.load_chunk(ChunkPosition { x: 5, z: 5 }).is_none());
    assert!(storage.load_chunk(ChunkPosition { x: -40, z: 5 }).is_none());
}

#[test]
fn region_files_follow_beta_s_layout() {
    let (_saves, storage) = original_world("original-layout");
    let position = ChunkPosition { x: 3, z: 1 };
    storage
        .save_chunk(position, &OverworldGenerator::new(SEED).generate(position))
        .unwrap();

    let bytes = fs::read(storage.root().join("region").join("r.0.0.mcr")).unwrap();
    let slot = (3 + 1 * 32) * 4;
    let entry = u32::from_be_bytes(bytes[slot..slot + 4].try_into().unwrap());
    let (sector, count) = ((entry >> 8) as usize, (entry & 0xFF) as usize);
    assert!(
        sector >= 2 && count >= 1,
        "the entry points past the two header sectors"
    );
    assert!(bytes.len() >= (sector + count) * 4096);
    // Every other slot is empty.
    assert_eq!(
        bytes[..4096]
            .chunks(4)
            .filter(|entry| entry.iter().any(|b| *b != 0))
            .count(),
        1
    );
    let timestamp = u32::from_be_bytes(bytes[4096 + slot..4096 + slot + 4].try_into().unwrap());
    assert!(timestamp > 1_000_000_000, "the save time is in seconds");

    let record = &bytes[sector * 4096..];
    let length = u32::from_be_bytes(record[..4].try_into().unwrap()) as usize;
    assert!(length >= 1 && length <= count * 4096);
    assert_eq!(record[4], 2, "zlib, as Beta writes");
    let mut nbt = Vec::new();
    ZlibDecoder::new(&record[5..4 + length])
        .read_to_end(&mut nbt)
        .unwrap();
    assert_eq!(&nbt[..3], &[10, 0, 0], "an unnamed compound root");
    for key in [
        &b"Level"[..],
        b"xPos",
        b"zPos",
        b"LastUpdate",
        b"Blocks",
        b"Data",
        b"SkyLight",
        b"BlockLight",
        b"HeightMap",
        b"TerrainPopulated",
        b"Entities",
        b"TileEntities",
    ] {
        assert!(
            contains(&nbt, key),
            "chunk lacks {}",
            String::from_utf8_lossy(key)
        );
    }
}

/// A chunk whose blocks barely compress, so it takes many sectors.
fn noisy_chunk(position: ChunkPosition, salt: usize) -> GeneratedChunk {
    let mut generated = OverworldGenerator::new(SEED).generate(position);
    let palette = [
        Block::Stone,
        Block::Dirt,
        Block::Gravel,
        Block::Sand,
        Block::CoalOre,
    ];
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let hash = (x * 73 + y * 31 + z * 17 + salt).wrapping_mul(2_654_435_761) >> 7;
                generated.chunk.set(x, y, z, palette[hash % palette.len()]);
            }
        }
    }
    generated
}

#[test]
fn rewriting_a_chunk_with_a_different_size_keeps_its_neighbours() {
    let (_saves, storage) = original_world("original-rewrite");
    let positions = [0, 1, 2].map(|x| ChunkPosition { x, z: 0 });
    let mut chunks: Vec<_> = positions
        .iter()
        .map(|p| OverworldGenerator::new(SEED).generate(*p))
        .collect();
    storage
        .save_chunks(positions.iter().copied().zip(chunks.iter()))
        .unwrap();

    // Grow the first chunk past its sectors, then shrink it, then grow the
    // middle one: each move must leave the others readable.
    for (index, replacement) in [
        (0, noisy_chunk(positions[0], 1)),
        (0, OverworldGenerator::new(SEED).generate(positions[0])),
        (1, noisy_chunk(positions[1], 2)),
        (0, noisy_chunk(positions[0], 3)),
    ] {
        chunks[index] = replacement;
        storage
            .save_chunk(positions[index], &chunks[index])
            .unwrap();
        for (position, expected) in positions.iter().zip(&chunks) {
            let loaded = storage.load_chunk(*position).expect("chunk should load");
            assert_same_blocks(&loaded.chunk, &expected.chunk);
        }
    }
    let length = fs::metadata(storage.root().join("region").join("r.0.0.mcr"))
        .unwrap()
        .len();
    assert_eq!(length % 4096, 0);
}

#[test]
fn the_player_is_stored_in_level_dat() {
    let (_saves, storage) = original_world("original-player");
    let mut hotbar = Hotbar::default();
    hotbar.slots[2] = Some(stack(Item::Stick, 3));
    let mut inventory = Inventory::default();
    inventory.main[5] = Some(stack(Item::IronIngot, 12));
    inventory.main[26] = Some(stack(Item::Coal, 64));
    inventory.armor[0] = Some(ItemStack::with_data(Item::IronHelmet, 1, 20).unwrap());
    inventory.armor[3] = Some(stack(Item::DiamondBoots, 1));
    // Beta saves neither the crafting grid nor the held stack.
    inventory.crafting[1] = Some(stack(Item::Stick, 4));
    inventory.carried = Some(stack(Item::Coal, 2));

    let mut player = StoredPlayer::from_transform(&Transform {
        translation: Vec3::new(48.0, 72.0, -24.0),
        rotation: Quat::from_euler(EulerRot::YXZ, 0.75, -0.2, 0.0),
        ..default()
    })
    .with_flying(true, 2.5)
    .with_game_mode(game::player::GameMode::Creative)
    .with_health(14)
    .with_inventory(&hotbar, &inventory);
    player.air = 88;
    player.fire = 130;
    player.fall_distance = 4.25;
    // A bed spawn is Beta's `SpawnX/Y/Z`, and no bed is no tags at all.
    player.spawn = Some([40, 70, -30]);
    storage.save_player(&player).unwrap();
    let reopen = || {
        WorldStorage::open(storage.root().to_path_buf())
            .unwrap()
            .load_player()
            .unwrap()
    };
    assert_eq!(reopen().spawn, Some([40, 70, -30]));
    player.spawn = None;
    storage.save_player(&player).unwrap();
    assert_eq!(reopen().spawn, None);

    let loaded = WorldStorage::open(storage.root().to_path_buf())
        .unwrap()
        .load_player()
        .expect("the player is in level.dat");
    assert!((loaded.x - 48.0).abs() < 0.001);
    assert!((loaded.y - 72.0).abs() < 0.001);
    assert!((loaded.z - -24.0).abs() < 0.001);
    assert!((loaded.yaw - 0.75).abs() < 0.001, "yaw {}", loaded.yaw);
    assert!((loaded.pitch - -0.2).abs() < 0.001);
    assert_eq!(loaded.health, 14);
    assert_eq!(
        (loaded.air, loaded.fire, loaded.fall_distance),
        (88, 130, 4.25)
    );
    assert!(loaded.flying);
    assert_eq!(loaded.game_mode, game::player::GameMode::Creative);
    assert_eq!(loaded.fly_speed, 2.5);

    assert_eq!(loaded.hotbar[2], Some(stored(Item::Stick, 3)));
    assert_eq!(loaded.main[5], Some(stored(Item::IronIngot, 12)));
    assert_eq!(loaded.main[26], Some(stored(Item::Coal, 64)));
    assert_eq!(
        loaded.armor[0],
        Some(StoredStack {
            id: Item::IronHelmet.as_u16(),
            count: 1,
            data: 20
        })
    );
    assert_eq!(loaded.armor[3], Some(stored(Item::DiamondBoots, 1)));
    // The crafting grid and the held stack moved into free slots.
    let all: Vec<_> = loaded.hotbar.iter().chain(&loaded.main).flatten().collect();
    assert!(all.contains(&&stored(Item::Stick, 4)));
    assert!(all.contains(&&stored(Item::Coal, 2)));

    // And it is where Beta looks: `Data.Player` with an `Inventory` list, boots
    // in slot 100 and the helmet in slot 103.
    let level = gunzip(&storage.root().join("level.dat"));
    for key in [
        &b"Player"[..],
        b"Inventory",
        b"Pos",
        b"Rotation",
        b"Health",
        b"Dimension",
    ] {
        assert!(
            contains(&level, key),
            "level.dat lacks {}",
            String::from_utf8_lossy(key)
        );
    }
}

#[test]
fn saving_chunks_keeps_the_player() {
    let (_saves, storage) = original_world("original-keeps-player");
    storage
        .save_player(&StoredPlayer::from_transform(&Transform::from_xyz(
            1.0, 70.0, 2.0,
        )))
        .unwrap();
    // Chunk saves rewrite level.dat, which must not lose the player.
    storage
        .save_chunk(
            ChunkPosition::ZERO,
            &OverworldGenerator::new(SEED).generate(ChunkPosition::ZERO),
        )
        .unwrap();
    let loaded = storage.load_player().expect("player survives");
    assert!((loaded.y - 70.0).abs() < 0.001);
}

#[test]
fn level_dat_keeps_the_previous_copy_like_beta() {
    let (_saves, storage) = original_world("original-level-old");
    storage
        .save_chunk(
            ChunkPosition::ZERO,
            &OverworldGenerator::new(SEED).generate(ChunkPosition::ZERO),
        )
        .unwrap();
    assert!(storage.root().join("level.dat_old").is_file());
    assert!(!storage.root().join("level.dat_new").exists());
}

#[test]
fn original_worlds_can_be_deleted() {
    let (saves, storage) = original_world("original-delete");
    let root = storage.root().to_path_buf();
    drop(storage);
    delete_world(&saves, &root).unwrap();
    assert!(!root.exists());
}

#[test]
fn an_original_world_is_saved_and_resumed_by_the_app() {
    let (saves, storage) = original_world("original-app");
    let root = storage.root().to_path_buf();
    drop(storage);

    let mut first = persistence_app(&saves);
    first.world_mut().spawn((
        LocalPlayer,
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
    assert_eq!(
        first
            .world()
            .resource::<WorldPersistence>()
            .storage()
            .expect("persistence should be enabled")
            .root(),
        root.as_path(),
        "the newest world is the Beta one"
    );

    // Exiting flushes the dirty chunks and the player.
    first.world_mut().write_message(AppExit::Success);
    first.update();
    assert!(root.join("region").join("r.0.0.mcr").is_file());
    assert!(!root.join("player.json").exists());

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
    assert!((player.z - -24.0).abs() < 0.001);
    assert!((player.yaw - 0.75).abs() < 0.001);
}

// ---------------------------------------------------------------------------
// Worlds the real game wrote

fn refs_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("refs/mc_b1.7.3_release/1.7.3-LTS/jars")
}

fn copy_directory(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of a saved Beta world in a fresh saves folder, or `None` when the
/// reference checkout is missing. Opening a world rewrites `session.lock`, so
/// the checked-in copy must never be opened directly.
fn beta_world_copy(label: &str, relative: &str) -> Option<(PathBuf, PathBuf)> {
    let source = refs_directory().join(relative);
    if !source.join("level.dat").is_file() {
        eprintln!("skipping: {} is missing", source.display());
        return None;
    }
    let saves = temp_saves(label);
    let root = saves.join("world");
    fs::create_dir_all(&root).unwrap();
    fs::copy(source.join("level.dat"), root.join("level.dat")).unwrap();
    copy_directory(&source.join("region"), &root.join("region"));
    Some((saves, root))
}

#[test]
fn chunks_of_the_beta_server_world_decode_to_the_pinned_blocks() {
    let Some((saves, root)) = beta_world_copy("beta-server-world", "world") else {
        return;
    };
    let worlds = list_worlds(&saves);
    assert_eq!(worlds.len(), 1, "a Beta folder is listed as a world");
    assert_eq!(worlds[0].manifest.format, SaveFormat::Original);
    assert_eq!(worlds[0].manifest.seed, beta_reference::SEED as u64);

    let storage = WorldStorage::open(root).unwrap();
    assert_eq!(storage.seed(), beta_reference::SEED as u64);
    assert!(
        storage.load_player().is_none(),
        "the server saves no player in level.dat"
    );
    for (x, z, expected) in beta_reference::CHUNKS {
        let position = ChunkPosition { x, z };
        let loaded = storage
            .load_chunk(position)
            .unwrap_or_else(|| panic!("chunk ({x}, {z}) should load from the region file"));
        assert!(
            loaded.populated,
            "chunk ({x}, {z}) was populated by the server"
        );
        assert_eq!(
            beta_reference::hash(&loaded.chunk),
            expected,
            "chunk ({x}, {z}) read from the Beta region file differs from the pinned blocks"
        );
    }
}

#[test]
fn a_beta_chunk_survives_being_saved_by_this_game() {
    let Some((_saves, root)) = beta_world_copy("beta-resave", "world") else {
        return;
    };
    let storage = WorldStorage::open(root).unwrap();
    let positions = [(-12, -8), (-1, 3), (9, -6)].map(|(x, z)| ChunkPosition { x, z });
    let before: Vec<_> = positions
        .iter()
        .map(|p| storage.load_chunk(*p).expect("chunk should load"))
        .collect();
    storage
        .save_chunks(positions.iter().copied().zip(before.iter()))
        .unwrap();
    for (position, original) in positions.iter().zip(&before) {
        let after = storage.load_chunk(*position).expect("chunk should reload");
        assert_same_blocks(&after.chunk, &original.chunk);
        assert_same_metadata(&after.chunk, &original.chunk);
        assert_eq!(
            after.chunk.mob_records().len(),
            original.chunk.mob_records().len()
        );
        assert_eq!(after.items.len(), original.items.len());
        assert_eq!(
            after.chunk.chests().count(),
            original.chunk.chests().count()
        );
    }
    // The chunks outside this save were not disturbed.
    for (x, z, expected) in beta_reference::CHUNKS {
        let loaded = storage.load_chunk(ChunkPosition { x, z }).unwrap();
        assert_eq!(beta_reference::hash(&loaded.chunk), expected);
    }
}

#[test]
fn the_beta_client_save_loads_its_player() {
    let Some((_saves, root)) = beta_world_copy("beta-client-save", "saves/New World") else {
        return;
    };
    let storage = WorldStorage::open(root).unwrap();
    assert_eq!(storage.seed(), -2_376_593_833_904_673_016_i64 as u64);
    let player = storage
        .load_player()
        .expect("the client keeps its player in level.dat");
    assert!(
        player.y > 0.0 && player.y < CHUNK_HEIGHT as f32 + 5.0,
        "y {}",
        player.y
    );
    assert!(player.health > 0 && player.health <= game::player::MAX_PLAYER_HEALTH);
    // The spawn chunk is in the region files.
    let spawn = ChunkPosition::from_block(player.x.floor() as i32, player.z.floor() as i32);
    let chunk = storage
        .load_chunk(spawn)
        .expect("the chunk under the player was saved");
    assert!(chunk.populated);
}

/// What `ChunkProviderHell.provideChunk` decides and nothing later moves:
/// netherrack, soul sand and bedrock. Everything else (air, lava, gravel that
/// fell, and decoration, whose placement Beta leaves to chunk load order)
/// counts as open.
fn nether_terrain(block: Block) -> u8 {
    match block {
        Block::Netherrack | Block::SoulSand | Block::Bedrock => block.as_u8(),
        _ => 0,
    }
}

#[test]
fn the_nether_generator_matches_the_beta_server_nether() {
    let Some((_saves, root)) = beta_world_copy("beta-server-nether", "world") else {
        return;
    };
    let source = refs_directory().join("world/DIM-1");
    if !source.is_dir() {
        eprintln!("skipping: {} is missing", source.display());
        return;
    }
    copy_directory(&source, &root.join("DIM-1"));
    let storage = WorldStorage::open(root).unwrap();
    let generator = NetherGenerator::new(storage.seed());

    let mut compared = 0;
    for x in -6..6 {
        for z in -6..6 {
            let position = ChunkPosition { x, z };
            let Some(saved) = storage.load_chunk_in(Dimension::Nether, position) else {
                continue;
            };
            assert_eq!(saved.biomes.get(0, 0).biome, Biome::Hell);
            let ours = generator.generate_base(position);
            let mut different = 0;
            for (index, (&ours, &beta)) in ours
                .chunk
                .raw_blocks()
                .iter()
                .zip(saved.chunk.raw_blocks())
                .enumerate()
            {
                let (ours, beta) = (Block::from(ours), Block::from(beta));
                // `WorldGenHellLava` turns a netherrack cell into a lava
                // source, which then flows; that is population, not terrain.
                if ours == Block::Netherrack && matches!(beta, Block::Lava | Block::FlowingLava) {
                    continue;
                }
                if nether_terrain(ours) != nether_terrain(beta) {
                    different += 1;
                    if different <= 5 {
                        let (bx, bz, by) = (index % 16, index / 16 % 16, index / 256);
                        eprintln!(
                            "chunk ({x}, {z}) at ({bx}, {by}, {bz}): {ours:?} vs Beta {beta:?}"
                        );
                    }
                }
            }
            assert_eq!(
                different, 0,
                "chunk ({x}, {z}) differs from the Beta 1.7.3 server's Nether"
            );
            compared += 1;
        }
    }
    assert!(compared > 50, "only {compared} Nether chunks were found");
}

#[test]
fn dispensers_and_note_blocks_use_beta_tile_entities() {
    let (saves, storage) = original_world("redstone-tiles");
    let position = ChunkPosition::ZERO;
    let mut generated = OverworldGenerator::new(SEED).generate(position);
    generated
        .chunk
        .set_with_metadata(3, 90, 4, Block::Dispenser, 3);
    generated
        .chunk
        .dispenser_mut(Chunk::index(3, 90, 4))
        .unwrap()
        .slots[2] = Some(stack(Item::Arrow, 9));
    generated.chunk.set(7, 90, 4, Block::NoteBlock);
    generated
        .chunk
        .note_mut(Chunk::index(7, 90, 4))
        .unwrap()
        .pitch = 12;
    storage.save_chunk(position, &generated).unwrap();

    let loaded = storage.load_chunk(position).unwrap();
    assert_eq!(
        loaded
            .chunk
            .dispenser(Chunk::index(3, 90, 4))
            .unwrap()
            .slots[2],
        Some(stack(Item::Arrow, 9))
    );
    let (_, note) = loaded
        .chunk
        .notes()
        .find(|(index, _)| *index == Chunk::index(7, 90, 4))
        .unwrap();
    assert_eq!(note.pitch, 12);
    fs::remove_dir_all(saves).unwrap();
}

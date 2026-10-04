//! Saving and loading the world.
//!
//! Every world gets its own folder under `saves/`, named from the creation time
//! and a hash so two worlds never collide:
//!
//! ```text
//! saves/
//!   world-20260920-153012-1a2b3c4d/
//!     level.json          world manifest: name, seed, creation time
//!     player.json         camera pose and inventory
//!     regions0,0/         chunks 0..15 x 0..15
//!       chunk0,0.bin
//!       chunk1,0.bin
//!     regions0,-1/        chunks 0..15 x -16..-1
//!       chunk0,-1.bin
//! ```
//!
//! Chunks are grouped into 16×16 region folders. Each chunk is its own serde
//! JSON file, written through a temporary path so an interrupted save cannot
//! corrupt the previous one.
//!
//! Saving never blocks the main thread for a whole batch. The autosave timer
//! starts a drain, each frame snapshots a few chunks under a small time budget,
//! and the JSON encoding plus the file writes run on [`IoTaskPool`]. Only one
//! write is in flight at a time, which keeps chunks reaching disk in the order
//! they were snapshotted and stops a burst of churn from queueing without
//! bound. Exiting waits for the in-flight write and then saves what is left in
//! one synchronous pass.
//!
//! This is a native format, not the Beta one. Beta compatibility is a later
//! priority; when it arrives, its adapters belong here alongside this format.

use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;
use serde::Deserialize;
use serde::Serialize;

use crate::block::blocks::Block;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::player::Player;
use crate::world::biome::Biome;
use crate::world::biome::BiomeMap;
use crate::world::biome::Climate;
use crate::world::block_ticks::BlockTicks;
use crate::world::chest::CHEST_SLOTS;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;
use crate::world::chunk::PendingTick;
use crate::world::chunk::WorldChunks;
use crate::world::furnace::FURNACE_SLOTS;
use crate::world::furnace::Furnace;
use crate::world::streaming::setup_streaming;

/// Default directory, relative to the working directory, that holds world folders.
pub const SAVES_DIRECTORY: &str = "saves";
/// Chunks per region along each axis. `regions0,0` covers chunks 0..15.
pub const REGION_SIZE: i32 = 16;
/// On-disk format version, written into the manifest and the player file.
pub const FORMAT_VERSION: u32 = 1;
/// Chunk file version. Version 2 stores species and orientation in block
/// metadata instead of in extra block ids; version 1 chunks regenerate.
pub const CHUNK_FORMAT_VERSION: u32 = 2;

const MANIFEST_FILE: &str = "level.json";
const PLAYER_FILE: &str = "player.json";
const AUTOSAVE_SECONDS: f32 = 60.0;
/// Most chunks one frame may turn into save snapshots. Each one walks its 4096
/// blocks, so this bounds the main-thread cost of a save; the encoding and the
/// file writes happen on a background task.
const MAX_SNAPSHOTS_PER_FRAME: usize = 8;
/// Stop snapshotting once a frame has spent this long on it, whichever limit
/// bites first. A slow frame must not turn into a bigger one.
const SNAPSHOT_BUDGET: Duration = Duration::from_millis(2);
const BLOCKS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
const COLUMNS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_SIZE;

/// Player pose and inventory stored as `player.json` in a world folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPlayer {
    pub format_version: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    #[serde(default = "full_player_health")]
    pub health: u8,
    #[serde(default)]
    pub hotbar: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub selected: usize,
    #[serde(default)]
    pub main: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub crafting: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub armor: Vec<Option<StoredStack>>,
    #[serde(default)]
    pub carried: Option<StoredStack>,
    #[serde(default)]
    pub flying: bool,
    #[serde(default)]
    pub fly_speed: f32,
}

const fn full_player_health() -> u8 {
    crate::player::MAX_PLAYER_HEALTH
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredStack {
    pub id: u16,
    pub count: u8,
    pub data: u16,
}

impl StoredStack {
    fn from_stack(stack: ItemStack) -> Self {
        Self {
            id: stack.item().as_u16(),
            count: stack.count(),
            data: stack.data(),
        }
    }
    fn into_stack(self) -> Option<ItemStack> {
        let id = Item::from_u16(self.id)?;
        ItemStack::with_data(id, self.count, self.data).ok()
    }
}

impl StoredPlayer {
    pub fn from_transform(transform: &Transform) -> Self {
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        Self {
            format_version: FORMAT_VERSION,
            x: transform.translation.x,
            y: transform.translation.y,
            z: transform.translation.z,
            yaw,
            pitch,
            health: full_player_health(),
            hotbar: Vec::new(),
            selected: 0,
            main: Vec::new(),
            crafting: Vec::new(),
            armor: Vec::new(),
            carried: None,
            flying: false,
            fly_speed: 1.0,
        }
    }

    pub fn with_flying(mut self, flying: bool, fly_speed: f32) -> Self {
        self.flying = flying;
        self.fly_speed = fly_speed;
        self
    }

    pub fn with_health(mut self, health: u8) -> Self {
        self.health = health;
        self
    }

    pub fn with_inventory(mut self, hotbar: &Hotbar, inventory: &Inventory) -> Self {
        self.hotbar = hotbar
            .slots
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.selected = hotbar.selected;
        self.main = inventory
            .main
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.crafting = inventory
            .crafting
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.armor = inventory
            .armor
            .iter()
            .map(|slot| slot.map(StoredStack::from_stack))
            .collect();
        self.carried = inventory.carried.map(StoredStack::from_stack);
        self
    }

    pub fn to_inventory(&self) -> (Hotbar, Inventory) {
        let mut hotbar = Hotbar::default();
        let mut inventory = Inventory::default();
        for (target, saved) in hotbar.slots.iter_mut().zip(&self.hotbar) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        hotbar.select(self.selected);
        for (target, saved) in inventory.main.iter_mut().zip(&self.main) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        for (target, saved) in inventory.crafting.iter_mut().zip(&self.crafting) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        for (target, saved) in inventory.armor.iter_mut().zip(&self.armor) {
            *target = saved.and_then(StoredStack::into_stack);
        }
        inventory.carried = self.carried.and_then(StoredStack::into_stack);
        (hotbar, inventory)
    }

    pub fn to_transform(&self) -> Transform {
        Transform {
            translation: Vec3::new(self.x, self.y, self.z),
            rotation: Quat::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0),
            ..default()
        }
    }
}

/// Metadata for one world folder, stored as `level.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldManifest {
    pub name: String,
    pub seed: u64,
    pub created_unix_millis: u64,
    pub last_played_unix_millis: u64,
    pub format_version: u32,
    /// World age in 20 Hz ticks. Missing on saves from before the day cycle.
    #[serde(default)]
    pub world_time: u64,
    #[serde(default)]
    pub weather: crate::world::weather::WorldWeather,
}

/// The region a chunk belongs to, as `(region_x, region_z)`.
pub fn region_of(position: ChunkPosition) -> (i32, i32) {
    (
        position.x.div_euclid(REGION_SIZE),
        position.z.div_euclid(REGION_SIZE),
    )
}

/// Folder name for a region, for example `regions0,0`.
pub fn region_dir_name(region: (i32, i32)) -> String {
    format!("regions{},{}", region.0, region.1)
}

/// File name for a chunk, for example `chunk0,0.bin`.
pub fn chunk_file_name(position: ChunkPosition) -> String {
    format!("chunk{},{}.bin", position.x, position.z)
}

/// Reads and writes one world folder.
///
/// All methods take `&self` and synchronize internally, so a single instance can
/// be shared with background generation jobs while the main thread saves.
pub struct WorldStorage {
    root: PathBuf,
    manifest: Mutex<WorldManifest>,
}

impl WorldStorage {
    /// Create a fresh world folder under `saves_directory`.
    pub fn create(saves_directory: &Path, seed: u64, name: &str) -> io::Result<Self> {
        fs::create_dir_all(saves_directory)?;
        let now = unix_millis();
        let folder = format!(
            "world-{}-{:08x}",
            format_utc(now),
            world_hash(seed, now) as u32
        );
        let root = saves_directory.join(folder);
        fs::create_dir_all(&root)?;
        let storage = Self {
            root,
            manifest: Mutex::new(WorldManifest {
                name: name.to_string(),
                seed,
                created_unix_millis: now,
                last_played_unix_millis: now,
                format_version: FORMAT_VERSION,
                world_time: 0,
                weather: default(),
            }),
        };
        storage.write_manifest()?;
        Ok(storage)
    }

    /// Open an existing world folder.
    pub fn open(root: PathBuf) -> io::Result<Self> {
        let manifest = read_manifest(&root)?;
        Ok(Self {
            root,
            manifest: Mutex::new(manifest),
        })
    }

    /// Resume the most recently created world, or create one if none exists.
    pub fn open_latest_or_create(saves_directory: &Path, seed: u64) -> io::Result<Self> {
        match latest_world_directory(saves_directory) {
            Some(root) => Self::open(root),
            None => Self::create(saves_directory, seed, "New World"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> WorldManifest {
        self.manifest.lock().unwrap().clone()
    }

    pub fn seed(&self) -> u64 {
        self.manifest.lock().unwrap().seed
    }

    /// Remember the day counter. The next manifest write persists it.
    pub fn set_world_time(&self, time: u64) {
        self.manifest.lock().unwrap().world_time = time;
    }

    pub fn set_weather(&self, weather: &crate::world::weather::WorldWeather) {
        self.manifest.lock().unwrap().weather = weather.clone();
    }

    /// Load the stored player state, or `None` if it was never saved.
    pub fn load_player(&self) -> Option<StoredPlayer> {
        let path = self.root.join(PLAYER_FILE);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StoredPlayer>(&bytes) {
                Ok(player) if player.format_version == FORMAT_VERSION => Some(player),
                Ok(_) => {
                    warn!("Ignoring player save with unsupported format");
                    None
                }
                Err(error) => {
                    warn!("Ignoring unreadable player {}: {error}", path.display());
                    None
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                warn!("Ignoring unreadable player {}: {error}", path.display());
                None
            }
        }
    }

    /// Write player state as pretty-printed JSON.
    pub fn save_player(&self, player: &StoredPlayer) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(player).map_err(io::Error::other)?;
        let path = self.root.join(PLAYER_FILE);
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(&temporary, path)?;
        self.touch()
    }

    /// Load a stored chunk, or `None` if it was never saved.
    pub fn load_chunk(&self, position: ChunkPosition) -> Option<GeneratedChunk> {
        let path = self.chunk_path(position);
        match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<StoredChunk>(&bytes) {
                Ok(stored) => stored.into_generated(),
                Err(error) => {
                    warn!("Ignoring unreadable chunk {}: {error}", path.display());
                    None
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                warn!("Ignoring unreadable chunk {}: {error}", path.display());
                None
            }
        }
    }

    /// Save one chunk to its region folder.
    pub fn save_chunk(&self, position: ChunkPosition, chunk: &GeneratedChunk) -> io::Result<()> {
        self.save_chunks([(position, chunk)]).map(|_| ())
    }

    /// Save many chunks, creating each region folder as needed.
    pub fn save_chunks<'a>(
        &self,
        chunks: impl IntoIterator<Item = (ChunkPosition, &'a GeneratedChunk)>,
    ) -> io::Result<usize> {
        let stored: Vec<_> = chunks
            .into_iter()
            .map(|(position, chunk)| {
                (
                    position,
                    StoredChunk::from_generated(chunk, &chunk.items, chunk.chunk.pending_ticks()),
                )
            })
            .collect();
        self.write_stored_chunks(stored)
    }

    /// Write chunks that are already in their stored form.
    ///
    /// This is the half that touches the disk, so a caller that builds
    /// [`StoredChunk`]s on the main thread can run it on a background task. The
    /// manifest is written once at the end, and only one caller at a time should
    /// be in here so that the last write of a chunk is the newest one.
    fn write_stored_chunks(&self, chunks: Vec<(ChunkPosition, StoredChunk)>) -> io::Result<usize> {
        let mut by_region: HashMap<(i32, i32), Vec<(ChunkPosition, StoredChunk)>> = HashMap::new();
        for (position, chunk) in chunks {
            by_region
                .entry(region_of(position))
                .or_default()
                .push((position, chunk));
        }
        if by_region.is_empty() {
            return Ok(0);
        }

        let mut saved = 0;
        for (region, entries) in by_region {
            let directory = self.region_path(region);
            fs::create_dir_all(&directory)?;
            for (position, chunk) in entries {
                write_chunk_file(&directory.join(chunk_file_name(position)), &chunk)?;
                saved += 1;
            }
        }
        self.touch()?;
        Ok(saved)
    }

    fn region_path(&self, region: (i32, i32)) -> PathBuf {
        self.root.join(region_dir_name(region))
    }

    fn chunk_path(&self, position: ChunkPosition) -> PathBuf {
        self.region_path(region_of(position))
            .join(chunk_file_name(position))
    }

    fn write_manifest(&self) -> io::Result<()> {
        let manifest = self.manifest.lock().unwrap();
        write_manifest_file(&self.root, &manifest)
    }

    fn touch(&self) -> io::Result<()> {
        let mut manifest = self.manifest.lock().unwrap();
        manifest.last_played_unix_millis = unix_millis();
        write_manifest_file(&self.root, &manifest)
    }
}

/// The on-disk form of a chunk. Blocks are run-length encoded because generated
/// terrain is mostly long runs of air and stone.
#[derive(Serialize, Deserialize)]
struct StoredChunk {
    format_version: u32,
    runs: Vec<(u8, u16)>,
    heightmap: Vec<u8>,
    biomes: Vec<StoredClimate>,
    /// Absent on chunks saved before dropped items were stored.
    #[serde(default)]
    items: Vec<StoredDroppedItem>,
    #[serde(default)]
    mobs: Vec<crate::entity::mobs::MobRecord>,
    #[serde(default)]
    spawners: Vec<(u16, crate::entity::mobs::MobSpawner)>,
    /// Absent on chunks saved before furnace inventories were added.
    #[serde(default)]
    furnaces: Vec<StoredFurnace>,
    /// Absent on chunks saved before chest inventories were added.
    #[serde(default)]
    chests: Vec<StoredChest>,
    /// Absent on chunks saved before population ran across chunks. Those were
    /// decorated in full when generated.
    #[serde(default = "populated_default")]
    populated: bool,
    /// Run-length encoded metadata nibbles, two blocks per byte in chunk
    /// index order. Empty when every value is zero, and absent on chunks
    /// saved before block metadata existed.
    #[serde(default)]
    metadata: Vec<(u8, u16)>,
    /// Scheduled block ticks that were pending when the chunk was unloaded.
    /// Absent on chunks saved before block ticks existed.
    #[serde(default)]
    ticks: Vec<StoredTick>,
}

/// A pending scheduled tick, stored relative to the save so a chunk that
/// stays unloaded for a while resumes with the same remaining delay.
#[derive(Serialize, Deserialize)]
struct StoredTick {
    index: u16,
    block: u8,
    delay: u32,
}

const fn populated_default() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct StoredFurnace {
    index: u16,
    slots: [Option<StoredStack>; FURNACE_SLOTS],
    burn_ticks: u16,
    fuel_ticks: u16,
    cook_ticks: u16,
}

#[derive(Serialize, Deserialize)]
struct StoredChest {
    index: u16,
    slots: [Option<StoredStack>; CHEST_SLOTS],
}

#[derive(Serialize, Deserialize)]
struct StoredDroppedItem {
    stack: StoredStack,
    x: f32,
    y: f32,
    z: f32,
    motion_x: f32,
    motion_y: f32,
    motion_z: f32,
    age_ticks: u32,
    pickup_delay_ticks: u16,
    hover_start: f32,
    rng_state: u64,
}

/// Climate quantized to a byte per field. Temperature and humidity only feed the
/// 256x256 grass/foliage palette lookup, so a byte is visually lossless there.
#[derive(Serialize, Deserialize)]
struct StoredClimate {
    temperature: u8,
    humidity: u8,
    biome: u8,
}

impl StoredChunk {
    /// Snapshot a live chunk. `items` and `ticks` carry the dropped items and
    /// pending block ticks that live outside the chunk while it is loaded, so
    /// they are passed in rather than written into the chunk and read back.
    fn from_generated(
        generated: &GeneratedChunk,
        items: &[ChunkDroppedItem],
        ticks: &[PendingTick],
    ) -> Self {
        Self {
            format_version: CHUNK_FORMAT_VERSION,
            runs: encode_blocks(generated.chunk.raw_blocks()),
            heightmap: generated.heightmap.heights().to_vec(),
            biomes: generated
                .biomes
                .cells()
                .iter()
                .map(|climate| StoredClimate {
                    temperature: quantize(climate.temperature),
                    humidity: quantize(climate.humidity),
                    biome: climate.biome.as_u8(),
                })
                .collect(),
            items: items
                .iter()
                .map(|item| StoredDroppedItem {
                    stack: StoredStack::from_stack(item.stack),
                    x: item.position[0],
                    y: item.position[1],
                    z: item.position[2],
                    motion_x: item.motion[0],
                    motion_y: item.motion[1],
                    motion_z: item.motion[2],
                    age_ticks: item.age_ticks,
                    pickup_delay_ticks: item.pickup_delay_ticks,
                    hover_start: item.hover_start,
                    rng_state: item.rng_state,
                })
                .collect(),
            mobs: generated.chunk.mob_records().to_vec(),
            spawners: generated
                .chunk
                .spawners()
                .map(|(index, spawner)| (index as u16, *spawner))
                .collect(),
            furnaces: generated
                .chunk
                .furnaces()
                .map(|(index, furnace)| StoredFurnace {
                    index: index as u16,
                    slots: furnace
                        .slots
                        .map(|stack| stack.map(StoredStack::from_stack)),
                    burn_ticks: furnace.burn_ticks,
                    fuel_ticks: furnace.fuel_ticks,
                    cook_ticks: furnace.cook_ticks,
                })
                .collect(),
            chests: generated
                .chunk
                .chests()
                .map(|(index, chest)| StoredChest {
                    index: index as u16,
                    slots: chest.slots.map(|stack| stack.map(StoredStack::from_stack)),
                })
                .collect(),
            populated: generated.populated,
            metadata: generated
                .chunk
                .raw_metadata()
                .map(encode_blocks)
                .unwrap_or_default(),
            ticks: ticks
                .iter()
                .map(|tick| StoredTick {
                    index: tick.index,
                    block: tick.block.as_u8(),
                    delay: tick.delay,
                })
                .collect(),
        }
    }

    /// The chunk as the bytes of its file. Encoding is the expensive half of a
    /// save, which is why it belongs off the main thread.
    fn to_bytes(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(io::Error::other)
    }

    fn into_generated(self) -> Option<GeneratedChunk> {
        if self.format_version != CHUNK_FORMAT_VERSION {
            return None;
        }
        if self.heightmap.len() != COLUMNS_PER_CHUNK || self.biomes.len() != COLUMNS_PER_CHUNK {
            return None;
        }
        let blocks = decode_blocks(&self.runs)?;

        let mut heights = [0u8; COLUMNS_PER_CHUNK];
        heights.copy_from_slice(&self.heightmap);

        let cells: [Climate; COLUMNS_PER_CHUNK] = std::array::from_fn(|index| {
            let stored = &self.biomes[index];
            Climate {
                temperature: dequantize(stored.temperature),
                humidity: dequantize(stored.humidity),
                biome: Biome::from_u8(stored.biome).unwrap_or(Biome::Plains),
            }
        });

        let mut chunk = Chunk::from_blocks(blocks);
        chunk.set_mob_records(self.mobs);
        for (index, spawner) in self.spawners {
            chunk.insert_spawner(usize::from(index), spawner);
        }
        if !self.metadata.is_empty() {
            chunk.set_raw_metadata(decode_runs(&self.metadata, BLOCKS_PER_CHUNK / 2)?);
        }
        chunk.set_pending_ticks(
            self.ticks
                .into_iter()
                .filter(|tick| usize::from(tick.index) < BLOCKS_PER_CHUNK)
                .filter_map(|tick| {
                    Some(PendingTick {
                        index: tick.index,
                        block: Block::from_u8(tick.block)?,
                        delay: tick.delay,
                    })
                })
                .collect(),
        );
        for furnace in self.furnaces {
            let index = usize::from(furnace.index);
            if index >= BLOCKS_PER_CHUNK {
                continue;
            }
            let y = index / (CHUNK_SIZE * CHUNK_SIZE);
            let z = index / CHUNK_SIZE % CHUNK_SIZE;
            let x = index % CHUNK_SIZE;
            if !matches!(
                chunk.get(x, y, z),
                Some(block) if block.is_furnace()
            ) {
                continue;
            }
            let slots = furnace
                .slots
                .map(|stack| stack.and_then(StoredStack::into_stack));
            chunk.insert_furnace(
                index,
                Furnace {
                    slots,
                    burn_ticks: furnace.burn_ticks,
                    fuel_ticks: furnace.fuel_ticks,
                    cook_ticks: furnace
                        .cook_ticks
                        .min(crate::world::furnace::SMELT_TICKS - 1),
                },
            );
        }
        for chest in self.chests {
            let index = usize::from(chest.index);
            if index >= BLOCKS_PER_CHUNK {
                continue;
            }
            let y = index / (CHUNK_SIZE * CHUNK_SIZE);
            let z = index / CHUNK_SIZE % CHUNK_SIZE;
            let x = index % CHUNK_SIZE;
            if !chunk.get(x, y, z).is_some_and(Block::is_chest) {
                continue;
            }
            let slots = chest
                .slots
                .map(|stack| stack.and_then(StoredStack::into_stack));
            chunk.insert_chest(index, Chest { slots });
        }

        Some(GeneratedChunk {
            chunk,
            heightmap: Heightmap::from_heights(heights),
            biomes: BiomeMap::from_cells(cells),
            items: self
                .items
                .into_iter()
                .filter_map(|item| {
                    item.stack.into_stack().map(|stack| ChunkDroppedItem {
                        stack,
                        position: [item.x, item.y, item.z],
                        motion: [item.motion_x, item.motion_y, item.motion_z],
                        age_ticks: item.age_ticks,
                        pickup_delay_ticks: item.pickup_delay_ticks,
                        hover_start: item.hover_start,
                        rng_state: item.rng_state,
                    })
                })
                .collect(),
            populated: self.populated,
        })
    }
}

fn decode_runs(runs: &[(u8, u16)], expected: usize) -> Option<Vec<u8>> {
    let total: usize = runs.iter().map(|(_, length)| *length as usize).sum();
    if total != expected {
        return None;
    }
    let mut bytes = Vec::with_capacity(total);
    for (value, length) in runs {
        bytes.resize(bytes.len() + *length as usize, *value);
    }
    Some(bytes)
}

fn encode_blocks(blocks: &[u8]) -> Vec<(u8, u16)> {
    let mut runs = Vec::new();
    let Some((&first, rest)) = blocks.split_first() else {
        return runs;
    };
    let mut value = first;
    let mut length: u16 = 1;
    for &next in rest {
        if next == value && length < u16::MAX {
            length += 1;
        } else {
            runs.push((value, length));
            value = next;
            length = 1;
        }
    }
    runs.push((value, length));
    runs
}

fn decode_blocks(runs: &[(u8, u16)]) -> Option<Vec<Block>> {
    let total: usize = runs.iter().map(|(_, length)| *length as usize).sum();
    if total != BLOCKS_PER_CHUNK {
        return None;
    }
    let mut blocks = Vec::with_capacity(total);
    for (value, length) in runs {
        let block = Block::from_u8(*value)?;
        if !block.in_world() {
            return None;
        }
        blocks.resize(blocks.len() + *length as usize, block);
    }
    Some(blocks)
}

fn quantize(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn dequantize(value: u8) -> f64 {
    f64::from(value) / 255.0
}

fn write_chunk_file(path: &Path, chunk: &StoredChunk) -> io::Result<()> {
    let bytes = chunk.to_bytes()?;
    // Write through a temporary file so an interrupted save leaves the previous
    // chunk intact instead of a half-written one.
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

fn read_manifest(root: &Path) -> io::Result<WorldManifest> {
    let bytes = fs::read(root.join(MANIFEST_FILE))?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn write_manifest_file(root: &Path, manifest: &WorldManifest) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    fs::write(root.join(MANIFEST_FILE), bytes)
}

/// The newest world folder, ordered by creation time and then by name.
fn latest_world_directory(saves_directory: &Path) -> Option<PathBuf> {
    let mut worlds: Vec<(u64, PathBuf)> = fs::read_dir(saves_directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let manifest = read_manifest(&path).ok()?;
            Some((manifest.created_unix_millis, path))
        })
        .collect();
    worlds.sort();
    worlds.pop().map(|(_, path)| path)
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// A hash of the seed and creation time, mixed with the current nanoseconds so
/// two worlds created in the same millisecond still get distinct folders.
fn world_hash(seed: u64, timestamp_millis: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    timestamp_millis.hash(&mut hasher);
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    hasher.finish()
}

/// `YYYYMMDD-HHMMSS` in UTC, for readable and sortable folder names.
fn format_utc(timestamp_millis: u64) -> String {
    let seconds = (timestamp_millis / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (time / 3600, (time % 3600) / 60, time % 60);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Days since the Unix epoch to a civil date, using Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = (days - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Configuration for [`PersistencePlugin`], inserted as a resource so tests can
/// point a world at a temporary directory.
#[derive(Resource, Clone)]
struct PersistenceConfig {
    saves_directory: PathBuf,
    seed: u64,
    autosave_seconds: f32,
}

/// Adds world saving and loading to the app.
///
/// This is deliberately separate from [`crate::world::plugin::WorldPlugin`] so
/// tests can run the world without touching the filesystem.
pub struct PersistencePlugin {
    saves_directory: PathBuf,
    seed: u64,
    autosave_seconds: f32,
}

impl PersistencePlugin {
    pub fn new(saves_directory: impl Into<PathBuf>) -> Self {
        Self {
            saves_directory: saves_directory.into(),
            seed: 0,
            autosave_seconds: AUTOSAVE_SECONDS,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Seconds between autosaves. Shortened by tests so they do not have to
    /// simulate half a minute of world time.
    pub fn with_autosave(mut self, seconds: f32) -> Self {
        self.autosave_seconds = seconds;
        self
    }
}

impl Default for PersistencePlugin {
    fn default() -> Self {
        Self::new(SAVES_DIRECTORY)
    }
}

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(PersistenceConfig {
            saves_directory: self.saves_directory.clone(),
            seed: self.seed,
            autosave_seconds: self.autosave_seconds,
        })
        .add_systems(Startup, setup_persistence.before(setup_streaming))
        // AppExit can be written by a UI system during Update. Run the save
        // pump after all Update systems so the exit message and the latest
        // player transform are both visible before Bevy shuts down.
        .add_systems(Last, flush_persistence);
    }
}

/// A batch of chunk snapshots handed to the background writer, plus the player
/// state to write alongside them.
struct SaveBatch {
    positions: Vec<ChunkPosition>,
    chunks: Vec<(ChunkPosition, StoredChunk)>,
    player: Option<StoredPlayer>,
}

/// What a background write reported back, read on the main thread once the task
/// finishes.
struct SaveOutcome {
    positions: Vec<ChunkPosition>,
    saved: usize,
    error: Option<String>,
}

/// Shared world storage plus the bookkeeping needed to save it.
#[derive(Resource)]
pub struct WorldPersistence {
    storage: Option<Arc<WorldStorage>>,
    /// Chunks that changed since the last save and are still loaded.
    dirty: HashSet<ChunkPosition>,
    /// Dirty chunks that were unloaded before a save could reach them.
    pending: Vec<(ChunkPosition, GeneratedChunk)>,
    /// Chunks already snapshotted and on their way to disk. A position is in
    /// at most one of `dirty`, `saving`, and `pending`.
    saving: HashSet<ChunkPosition>,
    /// The write in flight, if any. Only one runs at a time so chunks reach disk
    /// in the order they were snapshotted and a burst of churn cannot queue
    /// without bound.
    writer: Option<Task<SaveOutcome>>,
    /// Chunks already snapshotted into a batch for the current drain. A chunk is
    /// written at most once per autosave, so a world that keeps changing does
    /// not keep the drain open forever.
    drained: HashSet<ChunkPosition>,
    /// A save is due: the autosave fired and the drain has not finished.
    draining: bool,
    /// The player record has not been attached to a batch yet.
    player_pending: bool,
    /// When set, the next generation pass ignores chunks already on disk.
    regenerating: bool,
    /// A save was asked for outside the timer, such as by the pause menu.
    save_requested: bool,
    timer: Timer,
}

impl WorldPersistence {
    fn new(storage: WorldStorage, autosave_seconds: f32) -> Self {
        Self {
            storage: Some(Arc::new(storage)),
            dirty: HashSet::new(),
            pending: Vec::new(),
            saving: HashSet::new(),
            drained: HashSet::new(),
            writer: None,
            draining: false,
            player_pending: false,
            regenerating: false,
            save_requested: false,
            timer: Timer::from_seconds(autosave_seconds, TimerMode::Repeating),
        }
    }

    fn disabled() -> Self {
        Self {
            storage: None,
            dirty: HashSet::new(),
            pending: Vec::new(),
            saving: HashSet::new(),
            drained: HashSet::new(),
            writer: None,
            draining: false,
            player_pending: false,
            regenerating: false,
            save_requested: false,
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
        }
    }

    pub fn storage(&self) -> Option<&Arc<WorldStorage>> {
        self.storage.as_ref()
    }

    pub fn seed(&self) -> u64 {
        self.storage.as_ref().map_or(0, |storage| storage.seed())
    }

    /// Record that a chunk's data changed and should be written on the next save.
    pub fn mark_dirty(&mut self, position: ChunkPosition) {
        self.dirty.insert(position);
    }

    /// Keep an unloaded chunk's data around until the next save, but only if it
    /// actually changed. Unmodified chunks are regenerated identically instead.
    ///
    /// A chunk whose snapshot is already with the writer counts as changed: the
    /// snapshot predates the items and ticks streaming just attached to it, so
    /// the unloaded copy has to be saved after that write to win.
    pub fn queue_unload(&mut self, position: ChunkPosition, chunk: GeneratedChunk) {
        let changed = self.dirty.remove(&position) | self.saving.remove(&position);
        if changed {
            self.pending.push((position, chunk));
        }
    }

    /// Start a save on the next frame instead of waiting for the autosave
    /// timer, which then restarts. The save drains like an autosave, so it
    /// never blocks a frame.
    pub fn request_save(&mut self) {
        self.save_requested = true;
    }

    /// Regenerate from the world generator instead of loading from disk.
    pub fn request_regeneration(&mut self) {
        self.regenerating = true;
    }

    pub fn bypass_load(&self) -> bool {
        self.regenerating
    }

    pub fn finish_regeneration(&mut self) {
        self.regenerating = false;
    }

    /// True when a chunk the current cycle has not covered yet still needs a
    /// snapshot. A chunk already snapshotted this cycle does not, so a world
    /// that keeps changing still finishes its drain.
    fn has_work(&self) -> bool {
        !self.pending.is_empty()
            || self
                .dirty
                .iter()
                .any(|position| !self.drained.contains(position))
    }

    /// True while a background write is in flight. The next batch waits for it,
    /// so chunks reach disk in the order they were snapshotted.
    pub fn write_in_flight(&self) -> bool {
        self.writer.is_some()
    }

    /// Close the drain. A chunk edited after its snapshot this cycle waits for
    /// the next autosave, which is the same guarantee the old single-pass save
    /// gave.
    fn finish_drain(&mut self) {
        self.draining = false;
    }

    /// Begin a cycle. Each chunk is written at most once per cycle, so a world
    /// that keeps changing does not keep the drain open forever.
    fn start_drain(&mut self) {
        self.draining = true;
        self.drained.clear();
    }

    /// Retire the write that finished, if there was one.
    fn collect_finished_write(&mut self) {
        if !self.writer.as_ref().is_some_and(Task::is_finished) {
            return;
        }
        // A finished task still has to be polled for its output, and dropping it
        // would cancel it.
        let Some(mut task) = self.writer.take() else {
            return;
        };
        let Some(outcome) = check_ready(&mut task) else {
            return;
        };
        for position in &outcome.positions {
            self.saving.remove(position);
        }
        match &outcome.error {
            Some(error) => warn!("Failed to save world: {error}"),
            None if outcome.saved > 0 => info!("Saved {} chunks", outcome.saved),
            None => {}
        }
    }

    /// Snapshot up to [`MAX_SNAPSHOTS_PER_FRAME`] chunks under
    /// [`SNAPSHOT_BUDGET`] and hand them to the background writer.
    ///
    /// Runs once per frame while a save is due, so writing hundreds of chunks
    /// costs a little every frame instead of one long stall. Returns the number
    /// of chunks snapshotted.
    fn pump(
        &mut self,
        chunks: &WorldChunks,
        items: &HashMap<ChunkPosition, Vec<ChunkDroppedItem>>,
        ticks: &HashMap<ChunkPosition, Vec<PendingTick>>,
        player: Option<&StoredPlayer>,
    ) -> usize {
        if !self.draining || self.write_in_flight() || self.storage.is_none() {
            return 0;
        }

        let start = Instant::now();
        let mut batch: Vec<(ChunkPosition, StoredChunk)> = Vec::new();
        let mut positions: Vec<ChunkPosition> = Vec::new();

        // Unloaded chunks first: they are plain owned data, and getting one onto
        // disk frees the chunk's memory.
        let mut pending = std::mem::take(&mut self.pending);
        let mut taken = 0;
        while taken < pending.len() {
            if positions.len() >= MAX_SNAPSHOTS_PER_FRAME || start.elapsed() >= SNAPSHOT_BUDGET {
                break;
            }
            let (position, chunk) = &pending[taken];
            batch.push((
                *position,
                StoredChunk::from_generated(chunk, &chunk.items, chunk.chunk.pending_ticks()),
            ));
            positions.push(*position);
            taken += 1;
        }
        self.pending = pending.split_off(taken);

        // A chunk that is neither loaded nor with the writer has left the world
        // without being queued, which regeneration does. Its next load
        // regenerates it, so there is nothing to save.
        self.dirty.retain(|position| {
            self.saving.contains(position)
                || self.drained.contains(position)
                || chunks.contains(*position)
        });
        let candidates: Vec<ChunkPosition> = self
            .dirty
            .iter()
            .copied()
            .filter(|position| !self.saving.contains(position) && !self.drained.contains(position))
            .collect();
        for position in candidates {
            if positions.len() >= MAX_SNAPSHOTS_PER_FRAME || start.elapsed() >= SNAPSHOT_BUDGET {
                break;
            }
            let Some(chunk) = chunks.get(position) else {
                continue;
            };
            batch.push((
                position,
                StoredChunk::from_generated(
                    chunk,
                    items.get(&position).map_or(&[][..], Vec::as_slice),
                    ticks.get(&position).map_or(&[][..], Vec::as_slice),
                ),
            ));
            positions.push(position);
        }

        let player = player.filter(|_| self.player_pending).cloned();
        if player.is_some() {
            self.player_pending = false;
        }
        if batch.is_empty() && player.is_none() {
            return 0;
        }
        for position in &positions {
            self.dirty.remove(position);
            self.saving.insert(*position);
            self.drained.insert(*position);
        }
        let snapshotted = positions.len();
        self.submit(SaveBatch {
            positions,
            chunks: batch,
            player,
        });
        snapshotted
    }

    /// Send a batch to the writer thread.
    fn submit(&mut self, batch: SaveBatch) {
        let Some(storage) = self.storage.clone() else {
            return;
        };
        self.writer = Some(IoTaskPool::get().spawn(async move {
            let SaveBatch {
                positions,
                chunks,
                player,
            } = batch;
            let saved = match storage.write_stored_chunks(chunks) {
                Ok(saved) => saved,
                Err(error) => {
                    return SaveOutcome {
                        positions,
                        saved: 0,
                        error: Some(error.to_string()),
                    };
                }
            };
            // The manifest must be written after the chunks, and this task owns
            // the world folder for its duration.
            let error = match player {
                Some(player) => storage.save_player(&player).err(),
                None => None,
            };
            SaveOutcome {
                positions,
                saved,
                error: error.map(|error| error.to_string()),
            }
        }));
    }

    /// Wait for the in-flight write. Dropping a `Task` cancels it, so the
    /// outcome has to be taken before the task is dropped.
    fn wait_for_write(&mut self) {
        let Some(mut task) = self.writer.take() else {
            return;
        };
        while !task.is_finished() {
            std::thread::yield_now();
        }
        if let Some(outcome) = check_ready(&mut task)
            && let Some(error) = outcome.error
        {
            warn!("Failed to save world: {error}");
        }
        self.saving.clear();
    }

    /// Save everything still outstanding in one synchronous pass, as the exit
    /// path does. Waits for the background writer first so its older snapshots
    /// cannot land after this pass and undo it.
    pub fn flush(
        &mut self,
        chunks: &WorldChunks,
        player: Option<(
            &Transform,
            Option<&Hotbar>,
            Option<&Inventory>,
            bool,
            f32,
            u8,
        )>,
        items: &HashMap<ChunkPosition, Vec<ChunkDroppedItem>>,
        ticks: Option<&BlockTicks>,
    ) {
        let Some(storage) = self.storage.clone() else {
            return;
        };
        self.wait_for_write();
        self.finish_drain();
        self.drained.clear();
        self.player_pending = false;

        let mut batch: Vec<(ChunkPosition, StoredChunk)> = Vec::new();
        for (position, chunk) in self.pending.drain(..) {
            batch.push((
                position,
                StoredChunk::from_generated(&chunk, &chunk.items, chunk.chunk.pending_ticks()),
            ));
        }
        let ticks = ticks
            .map(BlockTicks::pending_ticks_by_chunk)
            .unwrap_or_default();
        for position in self.dirty.drain() {
            let Some(chunk) = chunks.get(position) else {
                continue;
            };
            batch.push((
                position,
                StoredChunk::from_generated(
                    chunk,
                    items.get(&position).map_or(&[][..], Vec::as_slice),
                    ticks.get(&position).map_or(&[][..], Vec::as_slice),
                ),
            ));
        }
        if !batch.is_empty() {
            match storage.write_stored_chunks(batch) {
                Ok(count) => info!("Saved {count} chunks to {}", storage.root().display()),
                Err(error) => warn!("Failed to save world: {error}"),
            }
        }
        if let Some((transform, hotbar, inventory, flying, fly_speed, health)) = player
            && let Err(error) = storage.save_player(
                &StoredPlayer::from_transform(transform)
                    .with_flying(flying, fly_speed)
                    .with_health(health)
                    .with_inventory(
                        hotbar.unwrap_or(&Hotbar::default()),
                        inventory.unwrap_or(&Inventory::default()),
                    ),
            )
        {
            warn!("Failed to save player: {error}");
        }
    }
}

fn setup_persistence(
    mut commands: Commands,
    config: Res<PersistenceConfig>,
    mut tick: Option<ResMut<crate::world::tick::WorldTick>>,
    mut weather: Option<ResMut<crate::world::weather::WorldWeather>>,
) {
    match WorldStorage::open_latest_or_create(&config.saves_directory, config.seed) {
        Ok(storage) => {
            info!(
                "World '{}' loaded from {}",
                storage.manifest().name,
                storage.root().display()
            );
            if let Some(tick) = tick.as_deref_mut() {
                tick.set_world_time(storage.manifest().world_time);
            }
            if let Some(weather) = weather.as_deref_mut() {
                *weather = storage.manifest().weather;
            }
            commands.insert_resource(WorldPersistence::new(storage, config.autosave_seconds));
        }
        Err(error) => {
            warn!("World persistence disabled: {error}");
            commands.insert_resource(WorldPersistence::disabled());
        }
    }
}

/// Save what has changed, without stalling the frame.
///
/// The autosave timer starts a drain rather than the save itself: each frame
/// snapshots a few chunks and the encoding and writes run on a background task,
/// so a world with hundreds of changed chunks never pays for all of them in one
/// frame. Exiting skips the drain and saves the rest synchronously, so nothing
/// is lost by quitting mid-save.
fn flush_persistence(
    mut persistence: ResMut<WorldPersistence>,
    mut chunks: ResMut<WorldChunks>,
    player: Query<
        (
            &Transform,
            Option<&Hotbar>,
            Option<&Inventory>,
            Option<&crate::entity::Flying>,
            &crate::player::FlySpeed,
            Option<&crate::player::PlayerHealth>,
        ),
        With<Player>,
    >,
    items: Query<
        (
            &Transform,
            &crate::entity::DroppedItem,
            &crate::entity::drops::items::ItemMotion,
            &crate::entity::drops::items::DroppedItemState,
        ),
        Without<crate::entity::drops::items::PickupAnimation>,
    >,
    mobs: Query<(
        &Transform,
        &crate::entity::Velocity,
        &crate::entity::mobs::Mob,
        Option<&crate::entity::creature::Living>,
    )>,
    time: Res<Time>,
    tick: Option<Res<crate::world::tick::WorldTick>>,
    block_ticks: Option<Res<BlockTicks>>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    mut exit: MessageReader<AppExit>,
) {
    let exiting = exit.read().next().is_some();
    persistence.timer.tick(time.delta());
    let requested = std::mem::take(&mut persistence.save_requested);
    if requested {
        persistence.timer.reset();
    }
    let autosave_due = requested || persistence.timer.just_finished();
    if autosave_due {
        persistence.start_drain();
        persistence.player_pending = true;
    }

    if (autosave_due || exiting)
        && let Some(storage) = persistence.storage()
    {
        if let Some(weather) = weather.as_deref() {
            storage.set_weather(weather);
        }
        if let Some(tick) = tick.as_deref() {
            storage.set_world_time(tick.world_time());
        }
    }
    if autosave_due || exiting {
        let mut by_chunk: HashMap<ChunkPosition, Vec<crate::entity::mobs::MobRecord>> =
            HashMap::new();
        for (transform, velocity, mob, living) in &mobs {
            by_chunk
                .entry(ChunkPosition::from_world(
                    transform.translation.x,
                    transform.translation.z,
                ))
                .or_default()
                .push(crate::entity::mobs::MobRecord::capture(
                    mob,
                    transform.translation,
                    velocity.0,
                    living,
                ));
        }
        let positions: Vec<_> = chunks.positions().collect();
        for position in positions {
            let records = by_chunk.remove(&position).unwrap_or_default();
            if let Some(chunk) = chunks.get_mut(position) {
                if !records.is_empty() || !chunk.chunk.mob_records().is_empty() {
                    chunk.chunk.set_mob_records(records);
                    persistence.mark_dirty(position);
                }
            }
        }
    }

    if exiting {
        if let Some(tick) = tick.as_deref()
            && let Some(storage) = persistence.storage()
        {
            storage.set_world_time(tick.world_time());
        }
        let items = dropped_items_by_chunk(&items);
        persistence.flush(
            &chunks,
            player.single().ok().map(
                |(transform, hotbar, inventory, flying, fly_speed, health)| {
                    (
                        transform,
                        hotbar,
                        inventory,
                        flying.is_some(),
                        fly_speed.0,
                        health.map_or(full_player_health(), |health| health.current),
                    )
                },
            ),
            &items,
            block_ticks.as_deref(),
        );
        return;
    }

    // Retire last frame's write before deciding there is nothing left to do.
    persistence.collect_finished_write();
    // Gathering the dropped items and pending ticks walks the whole world's
    // entities, so skip the frames that only wait on a write.
    if !persistence.draining || persistence.write_in_flight() {
        return;
    }

    let items = dropped_items_by_chunk(&items);
    let ticks = block_ticks
        .as_deref()
        .map(BlockTicks::pending_ticks_by_chunk)
        .unwrap_or_default();
    let record = if persistence.player_pending {
        player.single().ok().map(
            |(transform, hotbar, inventory, flying, fly_speed, health)| {
                StoredPlayer::from_transform(transform)
                    .with_flying(flying.is_some(), fly_speed.0)
                    .with_health(health.map_or(full_player_health(), |health| health.current))
                    .with_inventory(
                        hotbar.unwrap_or(&Hotbar::default()),
                        inventory.unwrap_or(&Inventory::default()),
                    )
            },
        )
    } else {
        None
    };
    if persistence.player_pending && record.is_none() {
        // Nothing to record until a player exists; let the drain finish.
        persistence.player_pending = false;
    }
    persistence.pump(&chunks, &items, &ticks, record.as_ref());
    if !persistence.has_work() && !persistence.player_pending && !persistence.write_in_flight() {
        persistence.finish_drain();
    }
}

/// The live dropped items, grouped by the chunk they would be saved into.
fn dropped_items_by_chunk(
    items: &Query<
        (
            &Transform,
            &crate::entity::DroppedItem,
            &crate::entity::drops::items::ItemMotion,
            &crate::entity::drops::items::DroppedItemState,
        ),
        Without<crate::entity::drops::items::PickupAnimation>,
    >,
) -> HashMap<ChunkPosition, Vec<ChunkDroppedItem>> {
    let mut grouped: HashMap<ChunkPosition, Vec<ChunkDroppedItem>> = HashMap::new();
    for (transform, dropped, motion, state) in items {
        let position = ChunkPosition::from_block(
            transform.translation.x.floor() as i32,
            transform.translation.z.floor() as i32,
        );
        grouped
            .entry(position)
            .or_default()
            .push(crate::entity::drops::items::chunk_record(
                dropped.0,
                transform.translation,
                motion.0,
                state,
            ));
    }
    grouped
}

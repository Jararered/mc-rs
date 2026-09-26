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
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::block::id::Id;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::player::Player;
use crate::world::chest::CHEST_SLOTS;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::furnace::FURNACE_SLOTS;
use crate::world::furnace::Furnace;
use crate::world::generation::Biome;
use crate::world::generation::BiomeMap;
use crate::world::generation::ChunkDroppedItem;
use crate::world::generation::Climate;
use crate::world::generation::GeneratedChunk;
use crate::world::generation::Heightmap;
use crate::world::streaming::setup_streaming;

/// Default directory, relative to the working directory, that holds world folders.
pub const SAVES_DIRECTORY: &str = "saves";
/// Chunks per region along each axis. `regions0,0` covers chunks 0..15.
pub const REGION_SIZE: i32 = 16;
/// On-disk format version, written into both the manifest and every chunk file.
pub const FORMAT_VERSION: u32 = 1;

const MANIFEST_FILE: &str = "level.json";
const PLAYER_FILE: &str = "player.json";
const AUTOSAVE_SECONDS: f32 = 30.0;
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
        let id = ItemId::from_u16(self.id)?;
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
        let mut by_region: HashMap<(i32, i32), Vec<(ChunkPosition, &'a GeneratedChunk)>> =
            HashMap::new();
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
                write_chunk_file(&directory.join(chunk_file_name(position)), chunk)?;
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
    /// Absent on chunks saved before furnace inventories were added.
    #[serde(default)]
    furnaces: Vec<StoredFurnace>,
    /// Absent on chunks saved before chest inventories were added.
    #[serde(default)]
    chests: Vec<StoredChest>,
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
    fn from_generated(generated: &GeneratedChunk) -> Self {
        Self {
            format_version: FORMAT_VERSION,
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
            items: generated
                .items
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
        }
    }

    fn into_generated(self) -> Option<GeneratedChunk> {
        if self.format_version != FORMAT_VERSION {
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
            if !chunk.get(x, y, z).is_some_and(Id::is_chest) {
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
        })
    }
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

fn decode_blocks(runs: &[(u8, u16)]) -> Option<Vec<Id>> {
    let total: usize = runs.iter().map(|(_, length)| *length as usize).sum();
    if total != BLOCKS_PER_CHUNK {
        return None;
    }
    let mut blocks = Vec::with_capacity(total);
    for (value, length) in runs {
        // Bytes 92..=99 are the older chunk encoding of species and torch
        // facing. 92..=96 are also cake through trapdoor, so this remap runs
        // before `from_u8`.
        let block = match *value {
            92 => Id::SpruceLeaves,
            93 => Id::BirchLeaves,
            94 => Id::SpruceWood,
            95 => Id::BirchWood,
            96 => Id::TorchWest,
            97 => Id::TorchEast,
            98 => Id::TorchNorth,
            99 => Id::TorchSouth,
            value => Id::from_u8(value)?,
        };
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

fn write_chunk_file(path: &Path, chunk: &GeneratedChunk) -> io::Result<()> {
    let stored = StoredChunk::from_generated(chunk);
    let bytes = serde_json::to_vec(&stored).map_err(io::Error::other)?;
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
}

/// Adds world saving and loading to the app.
///
/// This is deliberately separate from [`crate::world::plugin::WorldPlugin`] so
/// tests can run the world without touching the filesystem.
pub struct PersistencePlugin {
    saves_directory: PathBuf,
    seed: u64,
}

impl PersistencePlugin {
    pub fn new(saves_directory: impl Into<PathBuf>) -> Self {
        Self {
            saves_directory: saves_directory.into(),
            seed: 0,
        }
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
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
        })
        .add_systems(Startup, setup_persistence.before(setup_streaming))
        // AppExit can be written by a UI system during Update. Run the final
        // flush after all Update systems so the exit message and the latest
        // player transform are both visible before Bevy shuts down.
        .add_systems(Last, flush_persistence);
    }
}

/// Shared world storage plus the bookkeeping needed to save it.
#[derive(Resource)]
pub struct WorldPersistence {
    storage: Option<Arc<WorldStorage>>,
    /// Chunks that changed since the last save and are still loaded.
    dirty: HashSet<ChunkPosition>,
    /// Dirty chunks that were unloaded before a save could reach them.
    pending: Vec<(ChunkPosition, GeneratedChunk)>,
    /// Set by the F4 regeneration key so the next generation pass ignores disk.
    regenerating: bool,
    timer: Timer,
}

impl WorldPersistence {
    fn new(storage: WorldStorage) -> Self {
        Self {
            storage: Some(Arc::new(storage)),
            dirty: HashSet::new(),
            pending: Vec::new(),
            regenerating: false,
            timer: Timer::from_seconds(AUTOSAVE_SECONDS, TimerMode::Repeating),
        }
    }

    fn disabled() -> Self {
        Self {
            storage: None,
            dirty: HashSet::new(),
            pending: Vec::new(),
            regenerating: false,
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
    pub fn queue_unload(&mut self, position: ChunkPosition, chunk: GeneratedChunk) {
        if self.dirty.remove(&position) {
            self.pending.push((position, chunk));
        }
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

    /// Write dirty chunks and current player state.
    pub fn flush(
        &mut self,
        chunks: &mut WorldChunks,
        player: Option<(&Transform, Option<&Hotbar>, Option<&Inventory>, bool, f32)>,
        items: &std::collections::HashMap<ChunkPosition, Vec<ChunkDroppedItem>>,
    ) {
        let Some(storage) = self.storage.clone() else {
            return;
        };

        let pending = std::mem::take(&mut self.pending);
        let dirty: Vec<_> = self.dirty.drain().collect();
        for position in &dirty {
            if let Some(chunk) = chunks.get_mut(*position) {
                chunk.items = items.get(position).cloned().unwrap_or_default();
            }
        }
        let mut batch: Vec<(ChunkPosition, &GeneratedChunk)> =
            Vec::with_capacity(pending.len() + dirty.len());
        for (position, chunk) in &pending {
            batch.push((*position, chunk));
        }
        for position in &dirty {
            if let Some(chunk) = chunks.get(*position) {
                batch.push((*position, chunk));
            }
        }
        if !batch.is_empty() {
            match storage.save_chunks(batch) {
                Ok(count) => info!("Saved {count} chunks to {}", storage.root().display()),
                Err(error) => warn!("Failed to save world: {error}"),
            }
        }
        // Entities remain the live copy. Drop the snapshot so a later load of
        // this in-memory chunk does not spawn the items a second time.
        for position in &dirty {
            if let Some(chunk) = chunks.get_mut(*position) {
                chunk.items.clear();
            }
        }
        if let Some((transform, hotbar, inventory, flying, fly_speed)) = player
            && let Err(error) = storage.save_player(
                &StoredPlayer::from_transform(transform)
                    .with_flying(flying, fly_speed)
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
            commands.insert_resource(WorldPersistence::new(storage));
        }
        Err(error) => {
            warn!("World persistence disabled: {error}");
            commands.insert_resource(WorldPersistence::disabled());
        }
    }
}

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
    time: Res<Time>,
    tick: Option<Res<crate::world::tick::WorldTick>>,
    mut exit: MessageReader<AppExit>,
) {
    let exiting = exit.read().next().is_some();
    persistence.timer.tick(time.delta());
    if exiting || persistence.timer.just_finished() {
        if let Some(tick) = tick.as_deref()
            && let Some(storage) = persistence.storage()
        {
            storage.set_world_time(tick.world_time());
        }
        let mut saved = std::collections::HashMap::<ChunkPosition, Vec<ChunkDroppedItem>>::new();
        for (transform, dropped, motion, state) in &items {
            let position = ChunkPosition::from_block(
                transform.translation.x.floor() as i32,
                transform.translation.z.floor() as i32,
            );
            saved
                .entry(position)
                .or_default()
                .push(crate::entity::drops::items::chunk_record(
                    dropped.0,
                    transform.translation,
                    motion.0,
                    state,
                ));
        }
        persistence.flush(
            &mut chunks,
            player
                .single()
                .ok()
                .map(|(transform, hotbar, inventory, flying, fly_speed)| {
                    (transform, hotbar, inventory, flying.is_some(), fly_speed.0)
                }),
            &saved,
        );
    }
}

//! Saving and loading the world.
//!
//! Every world gets its own folder under `saves/`, named from the creation time
//! and a hash so two worlds never collide. What is inside depends on the
//! world's [`SaveFormat`], chosen when it is created:
//!
//! - [`binary`] is the native format: JSON manifest, player and chunks.
//! - [`original`] is Beta 1.7.3's own format (`level.dat` and McRegion files),
//!   so the world also opens in the original client and server.
//!
//! Everything else here is format-agnostic. [`WorldStorage`] is the one handle
//! the rest of the game uses, and it dispatches to the world's format.
//!
//! Saving never blocks the main thread for a whole batch. The autosave timer
//! starts a drain, each frame snapshots a few chunks under a small time budget,
//! and the encoding plus the file writes run on [`IoTaskPool`]. Only one
//! write is in flight at a time, which keeps chunks reaching disk in the order
//! they were snapshotted and stops a burst of churn from queueing without
//! bound. Exiting waits for the in-flight write and then saves what is left in
//! one synchronous pass.

mod binary;
mod original;

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

pub use self::binary::CHUNK_FORMAT_VERSION;
pub use self::binary::REGION_SIZE;
pub use self::binary::REGIONS_DIRECTORY;
pub use self::binary::chunk_file_name;
pub use self::binary::region_dir_name;
pub use self::binary::region_of;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::player::Player;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::PendingTick;
use crate::world::chunk::WorldChunks;
use crate::world::streaming::setup_streaming;

/// Default directory, relative to the working directory, that holds world folders.
pub const SAVES_DIRECTORY: &str = "saves";
/// On-disk format version, written into the manifest and the player file.
pub const FORMAT_VERSION: u32 = 1;

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
/// The same limits while the game is paused or on a menu, where nothing is
/// simulated and a longer frame is not noticed, so a save finishes sooner.
const PAUSED_MAX_SNAPSHOTS_PER_FRAME: usize = 128;
const PAUSED_SNAPSHOT_BUDGET: Duration = Duration::from_millis(50);

/// How a world is laid out on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SaveFormat {
    /// This game's own format, which keeps everything it simulates.
    #[default]
    Binary,
    /// Beta 1.7.3's format, readable by the original client and server.
    Original,
}

impl SaveFormat {
    pub const ALL: [Self; 2] = [Self::Binary, Self::Original];

    /// The next format, wrapping around, for a button that cycles.
    pub fn cycle(self) -> Self {
        match self {
            Self::Binary => Self::Original,
            Self::Original => Self::Binary,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "Native",
            Self::Original => "Beta 1.7.3",
        }
    }
}

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
    /// Difficulty chosen when the world was created, applied to the client
    /// setting when the world loads. Worlds from before this field keep
    /// whatever the player has selected.
    #[serde(default)]
    pub difficulty: Option<crate::app::settings::Difficulty>,
    /// The layout of the world folder. Worlds from before Beta saving are
    /// native.
    #[serde(default)]
    pub format: SaveFormat,
}

/// One world folder found in the saves directory.
#[derive(Debug, Clone)]
pub struct WorldSummary {
    pub root: PathBuf,
    pub manifest: WorldManifest,
}

/// Every readable world under `saves_directory`, most recently played first.
pub fn list_worlds(saves_directory: &Path) -> Vec<WorldSummary> {
    let Ok(entries) = fs::read_dir(saves_directory) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldSummary> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|root| {
            let manifest = read_manifest(&root).ok()?;
            Some(WorldSummary { root, manifest })
        })
        .collect();
    worlds.sort_by(|a, b| {
        b.manifest
            .last_played_unix_millis
            .cmp(&a.manifest.last_played_unix_millis)
            .then_with(|| a.root.cmp(&b.root))
    });
    worlds
}

/// A chunk snapshot in the stored form of its world's format.
///
/// Building one is the cheap half of a save and happens on the main thread;
/// encoding it and writing the file happen on the writer.
enum StoredChunkData {
    Binary(binary::StoredChunk),
    Original(original::ChunkSnapshot),
}

impl StoredChunkData {
    fn from_generated(
        format: SaveFormat,
        position: ChunkPosition,
        generated: &GeneratedChunk,
        items: &[ChunkDroppedItem],
        ticks: &[PendingTick],
    ) -> Self {
        match format {
            SaveFormat::Binary => {
                Self::Binary(binary::StoredChunk::from_generated(generated, items, ticks))
            }
            SaveFormat::Original => Self::Original(original::ChunkSnapshot::from_generated(
                position, generated, items,
            )),
        }
    }
}

/// How a [`WorldStorage`] reaches the disk.
enum Backend {
    Binary,
    Original(original::OriginalStore),
}

/// Reads and writes one world folder.
///
/// All methods take `&self` and synchronize internally, so a single instance can
/// be shared with background generation jobs while the main thread saves.
pub struct WorldStorage {
    root: PathBuf,
    manifest: Mutex<WorldManifest>,
    backend: Backend,
}

impl WorldStorage {
    /// Create a fresh world folder under `saves_directory`.
    pub fn create(saves_directory: &Path, seed: u64, name: &str) -> io::Result<Self> {
        Self::create_with(saves_directory, seed, name, None)
    }

    /// Like [`Self::create`], recording the difficulty the world was made with.
    pub fn create_with(
        saves_directory: &Path,
        seed: u64,
        name: &str,
        difficulty: Option<crate::app::settings::Difficulty>,
    ) -> io::Result<Self> {
        Self::create_in_format(saves_directory, seed, name, difficulty, SaveFormat::Binary)
    }

    /// Like [`Self::create_with`], laying the world out in `format`.
    pub fn create_in_format(
        saves_directory: &Path,
        seed: u64,
        name: &str,
        difficulty: Option<crate::app::settings::Difficulty>,
        format: SaveFormat,
    ) -> io::Result<Self> {
        fs::create_dir_all(saves_directory)?;
        let now = unix_millis();
        let folder = format!(
            "world-{}-{:08x}",
            format_utc(now),
            world_hash(seed, now) as u32
        );
        let root = saves_directory.join(folder);
        fs::create_dir_all(&root)?;
        let manifest = WorldManifest {
            name: name.to_string(),
            seed,
            created_unix_millis: now,
            last_played_unix_millis: now,
            format_version: FORMAT_VERSION,
            world_time: 0,
            weather: default(),
            difficulty,
            format,
        };
        let backend = match format {
            SaveFormat::Binary => {
                write_manifest_file(&root, &manifest)?;
                Backend::Binary
            }
            SaveFormat::Original => {
                Backend::Original(original::OriginalStore::create(&root, &manifest)?)
            }
        };
        Ok(Self {
            root,
            manifest: Mutex::new(manifest),
            backend,
        })
    }

    /// Open an existing world folder in whichever format it was saved in.
    pub fn open(root: PathBuf) -> io::Result<Self> {
        let manifest = read_manifest(&root)?;
        let backend = match manifest.format {
            SaveFormat::Binary => {
                if let Err(error) = binary::migrate_region_folders(&root) {
                    warn!(
                        "Could not move region folders of {}: {error}",
                        root.display()
                    );
                }
                Backend::Binary
            }
            SaveFormat::Original => {
                Backend::Original(original::OriginalStore::open(&root, manifest.seed))
            }
        };
        Ok(Self {
            root,
            manifest: Mutex::new(manifest),
            backend,
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

    pub fn format(&self) -> SaveFormat {
        match self.backend {
            Backend::Binary => SaveFormat::Binary,
            Backend::Original(_) => SaveFormat::Original,
        }
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
        if matches!(self.backend, Backend::Original(_)) {
            return match original::read_level(&self.root) {
                Ok(level) => level.player,
                Err(error) => {
                    warn!(
                        "Ignoring unreadable player in {}: {error}",
                        self.root.display()
                    );
                    None
                }
            };
        }
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

    /// Write the player state: `player.json` in a native world, the `Player`
    /// tag of `level.dat` in a Beta one.
    pub fn save_player(&self, player: &StoredPlayer) -> io::Result<()> {
        if matches!(self.backend, Backend::Original(_)) {
            self.manifest.lock().unwrap().last_played_unix_millis = unix_millis();
            return self.write_manifest(Some(player));
        }
        let bytes = serde_json::to_vec_pretty(player).map_err(io::Error::other)?;
        let path = self.root.join(PLAYER_FILE);
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(&temporary, path)?;
        self.touch()
    }

    /// Load a stored chunk, or `None` if it was never saved.
    pub fn load_chunk(&self, position: ChunkPosition) -> Option<GeneratedChunk> {
        match &self.backend {
            Backend::Binary => binary::load_chunk(&self.root, position),
            Backend::Original(store) => store.load_chunk(position),
        }
    }

    /// Save one chunk to its region.
    pub fn save_chunk(&self, position: ChunkPosition, chunk: &GeneratedChunk) -> io::Result<()> {
        self.save_chunks([(position, chunk)]).map(|_| ())
    }

    /// Save many chunks, creating each region as needed.
    pub fn save_chunks<'a>(
        &self,
        chunks: impl IntoIterator<Item = (ChunkPosition, &'a GeneratedChunk)>,
    ) -> io::Result<usize> {
        let format = self.format();
        let stored: Vec<_> = chunks
            .into_iter()
            .map(|(position, chunk)| {
                (
                    position,
                    StoredChunkData::from_generated(
                        format,
                        position,
                        chunk,
                        &chunk.items,
                        chunk.chunk.pending_ticks(),
                    ),
                )
            })
            .collect();
        self.write_stored_chunks(stored)
    }

    /// Write chunks that are already in their stored form.
    ///
    /// This is the half that touches the disk, so a caller that builds
    /// [`StoredChunkData`] on the main thread can run it on a background task.
    /// The manifest is written once at the end, and only one caller at a time
    /// should be in here so that the last write of a chunk is the newest one.
    fn write_stored_chunks(
        &self,
        chunks: Vec<(ChunkPosition, StoredChunkData)>,
    ) -> io::Result<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let saved = match &self.backend {
            Backend::Binary => {
                let chunks = chunks
                    .into_iter()
                    .filter_map(|(position, chunk)| match chunk {
                        StoredChunkData::Binary(chunk) => Some((position, chunk)),
                        StoredChunkData::Original(_) => None,
                    })
                    .collect();
                binary::write_chunks(&self.root, chunks)?
            }
            Backend::Original(store) => {
                let chunks = chunks
                    .into_iter()
                    .filter_map(|(position, chunk)| match chunk {
                        StoredChunkData::Original(chunk) => Some((position, chunk)),
                        StoredChunkData::Binary(_) => None,
                    })
                    .collect();
                let world_time = self.manifest.lock().unwrap().world_time;
                store.write_chunks(chunks, world_time)?
            }
        };
        self.touch()?;
        Ok(saved)
    }

    /// Write the manifest: `level.json`, or `level.dat` for a Beta world, with
    /// the player if one is given. A Beta world's player is part of `level.dat`,
    /// and without one the stored player stays as it is.
    fn write_manifest(&self, player: Option<&StoredPlayer>) -> io::Result<()> {
        let manifest = self.manifest.lock().unwrap();
        match &self.backend {
            Backend::Binary => write_manifest_file(&self.root, &manifest),
            Backend::Original(_) => original::write_level(&self.root, &manifest, player),
        }
    }

    fn touch(&self) -> io::Result<()> {
        self.manifest.lock().unwrap().last_played_unix_millis = unix_millis();
        self.write_manifest(None)
    }
}

/// Permanently remove a world folder. Only a direct child of `saves_directory`
/// that holds a world is removed, so a stray path cannot delete anything else.
pub fn delete_world(saves_directory: &Path, root: &Path) -> io::Result<()> {
    if root.parent() != Some(saves_directory) || !is_world_folder(root) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a world folder in the saves directory",
        ));
    }
    fs::remove_dir_all(root)
}

/// True for a folder holding a native or a Beta world.
fn is_world_folder(root: &Path) -> bool {
    root.join(MANIFEST_FILE).is_file() || original::is_world(root)
}

/// The manifest of a native world, or the one `level.dat` describes.
fn read_manifest(root: &Path) -> io::Result<WorldManifest> {
    match fs::read(root.join(MANIFEST_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound && original::is_world(root) => {
            original::read_level(root).map(|level| level.manifest)
        }
        Err(error) => Err(error),
    }
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
pub(crate) struct PersistenceConfig {
    pub(crate) saves_directory: PathBuf,
    seed: u64,
    autosave_seconds: f32,
    /// No world loads at startup; the game picks one later through
    /// [`PendingWorld`].
    deferred: bool,
}

/// Run condition for the startup systems that load a world. They are skipped
/// when the game chooses its world at runtime instead.
pub(crate) fn starts_with_world(config: Option<Res<PersistenceConfig>>) -> bool {
    config.is_none_or(|config| !config.deferred)
}

/// A world chosen at runtime, waiting for the load chain to install it.
#[derive(Resource)]
pub struct PendingWorld(pub Option<WorldStorage>);

/// Adds world saving and loading to the app.
///
/// This is deliberately separate from [`crate::world::plugin::WorldPlugin`] so
/// tests can run the world without touching the filesystem.
pub struct PersistencePlugin {
    saves_directory: PathBuf,
    seed: u64,
    autosave_seconds: f32,
    deferred: bool,
}

impl PersistencePlugin {
    pub fn new(saves_directory: impl Into<PathBuf>) -> Self {
        Self {
            saves_directory: saves_directory.into(),
            seed: 0,
            autosave_seconds: AUTOSAVE_SECONDS,
            deferred: false,
        }
    }

    /// Load no world at startup. The game installs one later by inserting a
    /// [`PendingWorld`], so a menu can choose which world to play.
    pub fn deferred(mut self) -> Self {
        self.deferred = true;
        self
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
            deferred: self.deferred,
        })
        .add_systems(
            Startup,
            setup_persistence
                .before(setup_streaming)
                .run_if(starts_with_world),
        )
        // AppExit can be written by a UI system during Update. Run the save
        // pump after all Update systems so the exit message and the latest
        // player transform are both visible before Bevy shuts down.
        .add_systems(
            Last,
            flush_persistence.run_if(resource_exists::<WorldPersistence>),
        );
    }
}

/// A batch of chunk snapshots handed to the background writer, plus the player
/// state to write alongside them.
struct SaveBatch {
    positions: Vec<ChunkPosition>,
    chunks: Vec<(ChunkPosition, StoredChunkData)>,
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

    /// True when nothing is waiting to be written: no drain, no write in flight,
    /// no unsaved chunk the last save did not cover.
    pub fn is_idle(&self) -> bool {
        !self.draining
            && !self.save_requested
            && !self.player_pending
            && self.writer.is_none()
            && !self.has_work()
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
    /// [`SNAPSHOT_BUDGET`] (the `PAUSED_` limits when `paused`) and hand them to
    /// the background writer.
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
        paused: bool,
    ) -> usize {
        if !self.draining || self.write_in_flight() || self.storage.is_none() {
            return 0;
        }
        let (max_snapshots, budget) = if paused {
            (PAUSED_MAX_SNAPSHOTS_PER_FRAME, PAUSED_SNAPSHOT_BUDGET)
        } else {
            (MAX_SNAPSHOTS_PER_FRAME, SNAPSHOT_BUDGET)
        };

        let format = self
            .storage
            .as_ref()
            .map_or(SaveFormat::Binary, |s| s.format());
        let start = Instant::now();
        let mut batch: Vec<(ChunkPosition, StoredChunkData)> = Vec::new();
        let mut positions: Vec<ChunkPosition> = Vec::new();

        // Unloaded chunks first: they are plain owned data, and getting one onto
        // disk frees the chunk's memory.
        let mut pending = std::mem::take(&mut self.pending);
        let mut taken = 0;
        while taken < pending.len() {
            if positions.len() >= max_snapshots || start.elapsed() >= budget {
                break;
            }
            let (position, chunk) = &pending[taken];
            batch.push((
                *position,
                StoredChunkData::from_generated(
                    format,
                    *position,
                    chunk,
                    &chunk.items,
                    chunk.chunk.pending_ticks(),
                ),
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
            if positions.len() >= max_snapshots || start.elapsed() >= budget {
                break;
            }
            let Some(chunk) = chunks.get(position) else {
                continue;
            };
            batch.push((
                position,
                StoredChunkData::from_generated(
                    format,
                    position,
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
        let format = storage.format();

        let mut batch: Vec<(ChunkPosition, StoredChunkData)> = Vec::new();
        for (position, chunk) in self.pending.drain(..) {
            batch.push((
                position,
                StoredChunkData::from_generated(
                    format,
                    position,
                    &chunk,
                    &chunk.items,
                    chunk.chunk.pending_ticks(),
                ),
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
                StoredChunkData::from_generated(
                    format,
                    position,
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
    mut settings: Option<ResMut<crate::app::settings::GameSettings>>,
) {
    match WorldStorage::open_latest_or_create(&config.saves_directory, config.seed) {
        Ok(storage) => install_world(
            &mut commands,
            storage,
            config.autosave_seconds,
            tick.as_deref_mut(),
            weather.as_deref_mut(),
            settings.as_deref_mut(),
        ),
        Err(error) => {
            warn!("World persistence disabled: {error}");
            commands.insert_resource(WorldPersistence::disabled());
        }
    }
}

/// Installs a world chosen at runtime: the first step of the load chain that
/// [`PendingWorld`] starts.
pub(crate) fn activate_pending_world(
    mut commands: Commands,
    config: Res<PersistenceConfig>,
    mut pending: ResMut<PendingWorld>,
    mut tick: Option<ResMut<crate::world::tick::WorldTick>>,
    mut weather: Option<ResMut<crate::world::weather::WorldWeather>>,
    mut settings: Option<ResMut<crate::app::settings::GameSettings>>,
) {
    match pending.0.take() {
        Some(storage) => install_world(
            &mut commands,
            storage,
            config.autosave_seconds,
            tick.as_deref_mut(),
            weather.as_deref_mut(),
            settings.as_deref_mut(),
        ),
        None => commands.insert_resource(WorldPersistence::disabled()),
    }
}

/// Makes `storage` the live world: its clock, weather and difficulty become the
/// game's, and a [`WorldPersistence`] starts saving into it.
fn install_world(
    commands: &mut Commands,
    storage: WorldStorage,
    autosave_seconds: f32,
    tick: Option<&mut crate::world::tick::WorldTick>,
    weather: Option<&mut crate::world::weather::WorldWeather>,
    settings: Option<&mut crate::app::settings::GameSettings>,
) {
    let manifest = storage.manifest();
    info!(
        "World '{}' loaded from {}",
        manifest.name,
        storage.root().display()
    );
    if let Some(tick) = tick {
        tick.set_world_time(manifest.world_time);
        // Ticks counted against the previous world's clock do not carry over.
        tick.idle();
    }
    if let Some(weather) = weather {
        *weather = manifest.weather;
    }
    if let (Some(settings), Some(difficulty)) = (settings, manifest.difficulty) {
        settings.difficulty = difficulty;
    }
    commands.insert_resource(WorldPersistence::new(storage, autosave_seconds));
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
    bodies: Query<crate::entity::SavedBodyData, crate::entity::SavedBodyFilter>,
    time: Res<Time>,
    tick: Option<Res<crate::world::tick::WorldTick>>,
    block_ticks: Option<Res<BlockTicks>>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    mut exit: MessageReader<AppExit>,
    pause: Option<Res<crate::app::state::PauseMenu>>,
    screen: Option<Res<State<crate::app::state::AppScreen>>>,
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
        let mut bodies_by_chunk: HashMap<ChunkPosition, Vec<crate::entity::SavedBody>> =
            HashMap::new();
        for (_, transform, falling, tnt, velocity) in &bodies {
            if let Some(body) = crate::entity::SavedBody::capture(transform, falling, tnt, velocity)
            {
                bodies_by_chunk
                    .entry(ChunkPosition::from_world(
                        transform.translation.x,
                        transform.translation.z,
                    ))
                    .or_default()
                    .push(body);
            }
        }
        let positions: Vec<_> = chunks.positions().collect();
        for position in positions {
            let records = by_chunk.remove(&position).unwrap_or_default();
            let saved_bodies = bodies_by_chunk.remove(&position).unwrap_or_default();
            if let Some(chunk) = chunks.get_mut(position) {
                if !records.is_empty() || !chunk.chunk.mob_records().is_empty() {
                    chunk.chunk.set_mob_records(records);
                    persistence.mark_dirty(position);
                }
                if !saved_bodies.is_empty() || !chunk.chunk.saved_bodies().is_empty() {
                    chunk.chunk.set_saved_bodies(saved_bodies);
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
    let paused = pause.is_some_and(|pause| pause.open)
        || screen.is_some_and(|screen| *screen.get() != crate::app::state::AppScreen::Playing);
    persistence.pump(&chunks, &items, &ticks, record.as_ref(), paused);
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

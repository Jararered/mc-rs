//! A world folder on disk: its manifest, the world list, and `WorldStorage`,
//! which reads and writes chunks and the player in either save format.

use super::FORMAT_VERSION;
use super::MANIFEST_FILE;
use super::PLAYER_FILE;
use super::binary;
use super::original;
use super::player::StoredPlayer;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::PendingTick;
use crate::world::dimension::Dimension;
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;
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
    pub difficulty: Option<crate::world::difficulty::Difficulty>,
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
pub(super) enum StoredChunkData {
    Binary(binary::StoredChunk),
    Original(original::ChunkSnapshot),
}

impl StoredChunkData {
    pub(super) fn from_generated(
        format: SaveFormat,
        position: ChunkPosition,
        generated: &GeneratedChunk,
        items: &[ChunkDroppedItem],
        ticks: &[PendingTick],
        light: Option<Arc<[u8]>>,
    ) -> Self {
        match format {
            SaveFormat::Binary => {
                Self::Binary(binary::StoredChunk::from_generated(generated, items, ticks))
            }
            // Only Beta's format stores light.
            SaveFormat::Original => Self::Original(original::ChunkSnapshot::from_generated(
                position, generated, items, light,
            )),
        }
    }
}

/// How a [`WorldStorage`] reaches the disk.
pub(super) enum Backend {
    Binary,
    Original(original::OriginalStore),
}

/// Reads and writes one world folder.
///
/// All methods take `&self` and synchronize internally, so a single instance can
/// be shared with background generation jobs while the main thread saves.
pub struct WorldStorage {
    pub(super) root: PathBuf,
    pub(super) manifest: Mutex<WorldManifest>,
    pub(super) backend: Backend,
    /// The dimension [`Self::load_chunk`] and chunk writes address. It only
    /// changes between drains, when nothing is in flight.
    pub(super) dimension: Mutex<Dimension>,
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
        difficulty: Option<crate::world::difficulty::Difficulty>,
    ) -> io::Result<Self> {
        Self::create_in_format(saves_directory, seed, name, difficulty, SaveFormat::Binary)
    }

    /// Like [`Self::create_with`], laying the world out in `format`.
    pub fn create_in_format(
        saves_directory: &Path,
        seed: u64,
        name: &str,
        difficulty: Option<crate::world::difficulty::Difficulty>,
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
            dimension: Mutex::new(Dimension::Overworld),
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
            dimension: Mutex::new(Dimension::Overworld),
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

    pub fn set_difficulty(&self, difficulty: crate::world::difficulty::Difficulty) {
        self.manifest.lock().unwrap().difficulty = Some(difficulty);
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

    /// The dimension chunk reads and writes address.
    pub fn dimension(&self) -> Dimension {
        *self.dimension.lock().unwrap()
    }

    /// Point chunk reads and writes at `dimension`. Call it only while no
    /// save is in flight: a write that is running finishes where it started,
    /// but one that has not begun would land in the new dimension.
    pub fn set_dimension(&self, dimension: Dimension) {
        *self.dimension.lock().unwrap() = dimension;
    }

    /// Where a native world keeps `dimension`'s `regions` folder.
    pub(super) fn chunk_root(&self, dimension: Dimension) -> PathBuf {
        match dimension.folder() {
            Some(folder) => self.root.join(folder),
            None => self.root.clone(),
        }
    }

    /// Load a stored chunk of the active dimension, or `None` if it was never
    /// saved.
    pub fn load_chunk(&self, position: ChunkPosition) -> Option<GeneratedChunk> {
        self.load_chunk_in(self.dimension(), position)
    }

    /// [`Self::load_chunk`] for a named dimension.
    pub fn load_chunk_in(
        &self,
        dimension: Dimension,
        position: ChunkPosition,
    ) -> Option<GeneratedChunk> {
        match &self.backend {
            Backend::Binary => binary::load_chunk(&self.chunk_root(dimension), position),
            Backend::Original(store) => store.load_chunk(dimension, position),
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
                        None,
                    ),
                )
            })
            .collect();
        let saved = self.write_stored_chunks(&stored)?;
        if saved > 0 {
            self.touch()?;
        }
        Ok(saved)
    }

    /// Write chunks that are already in their stored form.
    ///
    /// This is the half that touches the disk, so a caller that builds
    /// [`StoredChunkData`] on the main thread can run it on a background task.
    /// Only one caller at a time should be in here so that the last write of a
    /// chunk is the newest one. The chunks are borrowed so a caller whose write
    /// fails still has them to try again. The manifest is not written; a
    /// caller follows up with [`Self::touch`] or [`Self::save_player`].
    pub(super) fn write_stored_chunks(
        &self,
        chunks: &[(ChunkPosition, StoredChunkData)],
    ) -> io::Result<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }
        let dimension = self.dimension();
        match &self.backend {
            Backend::Binary => binary::write_chunks(
                &self.chunk_root(dimension),
                chunks.iter().filter_map(|(position, chunk)| match chunk {
                    StoredChunkData::Binary(chunk) => Some((*position, chunk)),
                    StoredChunkData::Original(_) => None,
                }),
            ),
            Backend::Original(store) => {
                let world_time = self.manifest.lock().unwrap().world_time;
                store.write_chunks(
                    dimension,
                    chunks.iter().filter_map(|(position, chunk)| match chunk {
                        StoredChunkData::Original(chunk) => Some((*position, chunk)),
                        StoredChunkData::Binary(_) => None,
                    }),
                    world_time,
                )
            }
        }
    }

    /// Write the manifest: `level.json`, or `level.dat` for a Beta world, with
    /// the player if one is given. A Beta world's player is part of `level.dat`,
    /// and without one the stored player stays as it is.
    pub(super) fn write_manifest(&self, player: Option<&StoredPlayer>) -> io::Result<()> {
        // Copy it out so the main thread's setters never wait on the disk.
        let manifest = self.manifest.lock().unwrap().clone();
        match &self.backend {
            Backend::Binary => write_manifest_file(&self.root, &manifest),
            Backend::Original(_) => original::write_level(&self.root, &manifest, player),
        }
    }

    pub(super) fn touch(&self) -> io::Result<()> {
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
pub(super) fn is_world_folder(root: &Path) -> bool {
    root.join(MANIFEST_FILE).is_file() || original::is_world(root)
}

/// The manifest of a native world, or the one `level.dat` describes.
pub(super) fn read_manifest(root: &Path) -> io::Result<WorldManifest> {
    match fs::read(root.join(MANIFEST_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound && original::is_world(root) => {
            original::read_level(root).map(|level| level.manifest)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn write_manifest_file(root: &Path, manifest: &WorldManifest) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?;
    // Through a temporary file, so an interrupted write cannot leave a manifest
    // that no longer parses and hide the world from the world list.
    let path = root.join(MANIFEST_FILE);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

/// The newest world folder, ordered by creation time and then by name.
pub(super) fn latest_world_directory(saves_directory: &Path) -> Option<PathBuf> {
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

pub(super) fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// A hash of the seed and creation time, mixed with the current nanoseconds so
/// two worlds created in the same millisecond still get distinct folders.
pub(super) fn world_hash(seed: u64, timestamp_millis: u64) -> u64 {
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
pub(super) fn format_utc(timestamp_millis: u64) -> String {
    let seconds = (timestamp_millis / 1000) as i64;
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (time / 3600, (time % 3600) / 60, time % 60);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Days since the Unix epoch to a civil date, using Howard Hinnant's algorithm.
pub(super) fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

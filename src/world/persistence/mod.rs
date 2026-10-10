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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

pub use self::binary::CHUNK_FORMAT_VERSION;
pub use self::binary::REGION_SIZE;
pub use self::binary::REGIONS_DIRECTORY;
pub use self::binary::chunk_file_name;
pub use self::binary::region_dir_name;
pub use self::binary::region_of;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::PendingTick;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::Dimension;
use crate::world::lighting::LightCache;
use crate::world::streaming::setup_streaming;

mod player;
mod storage;
mod systems;

pub use player::StoredPlayer;
pub use player::StoredStack;
pub use storage::SaveFormat;
use storage::StoredChunkData;
pub use storage::WorldManifest;
pub use storage::WorldStorage;
pub use storage::WorldSummary;
pub use storage::delete_world;
pub use storage::list_worlds;
pub(crate) use systems::activate_pending_world;
use systems::flush_persistence;
use systems::setup_persistence;

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

/// Add only the per-frame save pump, for an app whose [`WorldPersistence`] is
/// put in by hand rather than opened by [`PersistencePlugin`]: one dimension
/// of a [`WorldHost`](crate::world::host::WorldHost).
pub fn add_saving(app: &mut App) {
    app.add_systems(
        Last,
        flush_persistence.run_if(resource_exists::<WorldPersistence>),
    );
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
    /// Write the manifest even without a player, as the batch that closes a
    /// drain does.
    manifest: bool,
}

/// What a background write reported back, read on the main thread once the task
/// finishes.
struct SaveOutcome {
    positions: Vec<ChunkPosition>,
    /// The batch's chunks, handed back when the write failed.
    failed: Vec<(ChunkPosition, StoredChunkData)>,
    saved: usize,
    error: Option<String>,
}

/// Shared world storage plus the bookkeeping needed to save it.
#[derive(Resource)]
pub struct WorldPersistence {
    storage: Option<Arc<WorldStorage>>,
    /// The dimension whose chunks this loads and saves. Several of these can
    /// share one storage, one to a dimension.
    dimension: Dimension,
    /// This one also writes what the dimensions share: the manifest (time,
    /// weather, difficulty) and the local player's record. Exactly one of
    /// those sharing a storage does.
    level: bool,
    /// Chunks that changed since the last save and are still loaded.
    dirty: HashSet<ChunkPosition>,
    /// Dirty chunks that were unloaded before a save could reach them, each
    /// with the light it had when it left, which only Beta's format stores.
    pending: Vec<(ChunkPosition, GeneratedChunk, Option<Arc<[u8]>>)>,
    /// Snapshots whose write failed and whose chunk is no longer loaded. The
    /// next drain, or the exit save, writes them again.
    retry: Vec<(ChunkPosition, StoredChunkData)>,
    /// The current drain has not picked `retry` up yet.
    retry_due: bool,
    /// The current drain has not written the manifest yet.
    manifest_pending: bool,
    /// The world is being saved in order to unload it, so it must stop
    /// changing.
    closing: bool,
    /// Chunks already snapshotted and on their way to disk. A position is in
    /// at most one of `dirty`, `saving`, and `pending`, except that a chunk
    /// whose failed write is being retried is `saving` and may be dirty too.
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
    /// Why the last write failed, until a later write succeeds.
    save_error: Option<String>,
    timer: Timer,
}

impl WorldPersistence {
    fn new(storage: WorldStorage, dimension: Dimension, autosave_seconds: f32) -> Self {
        Self {
            level: true,
            ..Self::for_dimension(Arc::new(storage), dimension, autosave_seconds)
        }
    }

    /// Saving for one more dimension of a world that is already open. It
    /// writes only its own chunks; the [`WorldPersistence`] the world was
    /// opened with keeps writing the manifest and the player.
    pub fn for_dimension(
        storage: Arc<WorldStorage>,
        dimension: Dimension,
        autosave_seconds: f32,
    ) -> Self {
        Self {
            storage: Some(storage),
            dimension,
            level: false,
            dirty: HashSet::new(),
            pending: Vec::new(),
            retry: Vec::new(),
            retry_due: false,
            manifest_pending: false,
            closing: false,
            saving: HashSet::new(),
            drained: HashSet::new(),
            writer: None,
            draining: false,
            player_pending: false,
            regenerating: false,
            save_requested: false,
            save_error: None,
            timer: Timer::from_seconds(autosave_seconds, TimerMode::Repeating),
        }
    }

    fn disabled() -> Self {
        Self {
            storage: None,
            dimension: Dimension::Overworld,
            level: true,
            dirty: HashSet::new(),
            pending: Vec::new(),
            retry: Vec::new(),
            retry_due: false,
            manifest_pending: false,
            closing: false,
            saving: HashSet::new(),
            drained: HashSet::new(),
            writer: None,
            draining: false,
            player_pending: false,
            regenerating: false,
            save_requested: false,
            save_error: None,
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
        self.queue_unload_lit(position, chunk, None);
    }

    /// [`Self::queue_unload`] with the chunk's cached light, so a Beta world
    /// saves the light the chunk had among its neighbors.
    pub fn queue_unload_lit(
        &mut self,
        position: ChunkPosition,
        chunk: GeneratedChunk,
        light: Option<Arc<[u8]>>,
    ) {
        let changed = self.dirty.remove(&position) | self.saving.remove(&position);
        if changed {
            self.pending.push((position, chunk, light));
        }
    }

    /// Take back a chunk that unloaded with unsaved changes and has not been
    /// written yet. The disk still holds its older state, or nothing, so a
    /// reload must come from here. The chunk is marked dirty again.
    pub fn take_pending(&mut self, position: ChunkPosition) -> Option<GeneratedChunk> {
        let index = self
            .pending
            .iter()
            .rposition(|(pending, ..)| *pending == position)?;
        let (_, chunk, _) = self.pending.remove(index);
        // Older copies of the same chunk are superseded.
        self.pending.retain(|(pending, ..)| *pending != position);
        self.dirty.insert(position);
        Some(chunk)
    }

    /// True while a snapshot of this chunk is with the writer. Reading the
    /// chunk from disk before that write lands could return its older state.
    pub fn is_saving(&self, position: ChunkPosition) -> bool {
        self.saving.contains(&position)
    }

    /// [`Self::request_save`] before the world unloads. The world clock stops
    /// until then, so nothing changes behind a chunk's last snapshot.
    pub fn request_final_save(&mut self) {
        self.closing = true;
        self.save_requested = true;
    }

    /// Hold the world still ahead of [`Self::request_final_save`], while the
    /// jobs that still hold chunks finish.
    pub fn begin_closing(&mut self) {
        self.closing = true;
    }

    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Let the world run again after a final save that did not unload it:
    /// the save taken before travelling to the other dimension.
    pub fn resume(&mut self) {
        self.closing = false;
    }

    /// Point chunk loads and saves at `dimension`. Only valid once
    /// [`Self::is_idle`] with nothing unsaved, since the bookkeeping here is
    /// keyed by chunk position alone.
    pub fn set_dimension(&mut self, dimension: Dimension) {
        debug_assert!(self.is_idle() && !self.has_unsaved_chunks());
        self.dimension = dimension;
    }

    pub fn dimension(&self) -> Dimension {
        self.dimension
    }

    /// Load one of this dimension's stored chunks.
    pub fn load_chunk(&self, position: ChunkPosition) -> Option<GeneratedChunk> {
        self.storage
            .as_ref()?
            .load_chunk_in(self.dimension, position)
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

    /// True when a chunk is newer in memory than on disk. A drain writes each
    /// chunk once, so one that changed after its snapshot, or whose write
    /// failed, is still unsaved when the drain goes idle.
    pub fn has_unsaved_chunks(&self) -> bool {
        !self.dirty.is_empty() || !self.pending.is_empty() || !self.retry.is_empty()
    }

    /// Why the last write failed, if no write has succeeded since.
    pub fn save_error(&self) -> Option<&str> {
        self.save_error.as_deref()
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
        self.retry_due = true;
        self.manifest_pending = self.level;
    }

    /// Account for a write that finished. Chunks of a failed write are kept:
    /// a loaded one is dirty again, and an unloaded one waits in `retry`.
    fn retire(&mut self, outcome: SaveOutcome) {
        for (position, chunk) in outcome.failed {
            // No longer `saving` means the chunk unloaded during the write and
            // its newer copy is already in `pending`.
            if !self.saving.contains(&position) {
                continue;
            }
            self.dirty.insert(position);
            self.retry.retain(|(retry, _)| *retry != position);
            self.retry.push((position, chunk));
        }
        for position in &outcome.positions {
            self.saving.remove(position);
        }
        match &outcome.error {
            Some(error) => warn!("Failed to save world: {error}"),
            None if outcome.saved > 0 => info!("Saved {} chunks", outcome.saved),
            None => {}
        }
        if outcome.error.is_some() || self.retry.is_empty() {
            self.save_error = outcome.error;
        }
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
        self.retire(outcome);
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
        light: Option<&LightCache>,
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

        // Snapshots a failed write handed back go first, so anything newer in
        // this batch is written over them.
        let retried: Vec<ChunkPosition> = if std::mem::take(&mut self.retry_due) {
            batch.append(&mut self.retry);
            batch.iter().map(|(position, _)| *position).collect()
        } else {
            Vec::new()
        };

        // Unloaded chunks first: they are plain owned data, and getting one onto
        // disk frees the chunk's memory.
        let mut pending = std::mem::take(&mut self.pending);
        let mut taken = 0;
        while taken < pending.len() {
            if positions.len() >= max_snapshots || start.elapsed() >= budget {
                break;
            }
            let (position, chunk, light) = &pending[taken];
            batch.push((
                *position,
                StoredChunkData::from_generated(
                    format,
                    *position,
                    chunk,
                    &chunk.items,
                    chunk.chunk.pending_ticks(),
                    light.clone(),
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
                    light.and_then(|light| light.cells(position)),
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
        // A retried chunk counts as `saving` so nothing reads it from disk
        // meanwhile, but it keeps any dirty mark: its snapshot is an old one.
        self.saving.extend(retried.iter().copied());
        positions.extend(retried);
        self.submit(SaveBatch {
            positions,
            chunks: batch,
            player,
            manifest: false,
        });
        snapshotted
    }

    /// Send a batch to the writer thread.
    fn submit(&mut self, batch: SaveBatch) {
        let Some(storage) = self.storage.clone() else {
            return;
        };
        let dimension = self.dimension;
        self.writer = Some(IoTaskPool::get().spawn(async move {
            let SaveBatch {
                positions,
                chunks,
                player,
                manifest,
            } = batch;
            let saved = match storage.write_stored_chunks(dimension, &chunks) {
                Ok(saved) => saved,
                Err(error) => {
                    return SaveOutcome {
                        positions,
                        failed: chunks,
                        saved: 0,
                        error: Some(error.to_string()),
                    };
                }
            };
            // This task owns the world folder for its duration. Saving the
            // player writes the manifest too.
            let error = match player {
                Some(player) => storage.save_player(&player).err(),
                None if manifest => storage.touch().err(),
                None => None,
            };
            SaveOutcome {
                positions,
                failed: Vec::new(),
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
        if let Some(outcome) = check_ready(&mut task) {
            self.retire(outcome);
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
            crate::player::GameMode,
            u8,
            Option<&crate::player::PlayerSurvival>,
            Option<&crate::player::sleep::PlayerSleep>,
        )>,
        items: &HashMap<ChunkPosition, Vec<ChunkDroppedItem>>,
        ticks: Option<&BlockTicks>,
        light: Option<&LightCache>,
    ) {
        let Some(storage) = self.storage.clone() else {
            return;
        };
        self.wait_for_write();
        self.finish_drain();
        self.drained.clear();
        self.player_pending = false;
        self.retry_due = false;
        self.manifest_pending = false;
        let format = storage.format();

        // Oldest first: a failed write's snapshots, then unloaded chunks, then
        // the live world.
        let mut batch: Vec<(ChunkPosition, StoredChunkData)> = std::mem::take(&mut self.retry);
        for (position, chunk, light) in self.pending.drain(..) {
            batch.push((
                position,
                StoredChunkData::from_generated(
                    format,
                    position,
                    &chunk,
                    &chunk.items,
                    chunk.chunk.pending_ticks(),
                    light,
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
                    light.and_then(|light| light.cells(position)),
                ),
            ));
        }
        if !batch.is_empty() {
            match storage
                .write_stored_chunks(self.dimension, &batch)
                .and_then(|count| {
                    if self.level {
                        storage.touch()?;
                    }
                    Ok(count)
                }) {
                Ok(count) => info!("Saved {count} chunks to {}", storage.root().display()),
                Err(error) => warn!("Failed to save world: {error}"),
            }
        }
        if let Some((
            transform,
            hotbar,
            inventory,
            flying,
            fly_speed,
            game_mode,
            health,
            survival,
            sleep,
        )) = player
            && let Err(error) = storage.save_player(
                &StoredPlayer::from_transform(transform)
                    .with_dimension(self.dimension)
                    .with_flying(flying, fly_speed)
                    .with_game_mode(game_mode)
                    .with_health(health)
                    .with_survival(survival)
                    .with_sleep(sleep)
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

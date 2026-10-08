//! Which world is loaded, and loading or leaving one while the game runs.
//!
//! The game starts with no world. Choosing one on the title screen calls
//! [`WorldSession::request_load`]; leaving to the title screen calls
//! [`WorldSession::request_leave`], which saves the world and unloads it.
//!
//! Leaving happens in two steps. The world first holds still while the
//! generation and population jobs in flight hand their chunks back; only then
//! is the final save requested, so no chunk changes after its snapshot. The
//! world unloads once nothing is left unsaved, and a save that fails is tried
//! again rather than thrown away.
//!
//! Travelling between the Overworld and the Nether ([`WorldSession::request_travel`])
//! is a leave that keeps the player: the same settle and final save, then
//! only the dimension's own state (chunks, light, block ticks, mobs, items)
//! is dropped, the storage is pointed at the other dimension, and a background
//! task runs Beta's `Teleporter` there before streaming starts again.
//!
//! Loading reuses the startup systems that used to run once: the chosen
//! storage is installed, the spawn area is built, and the player is spawned,
//! in that order, before the game switches to [`AppScreen::Playing`]. The
//! spawn area is built on the compute pool, so the menu keeps drawing until
//! it is ready.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

use crate::app::settings::ClientDifficulty;
use crate::app::settings::Difficulty;
use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::app::state::SettingsReturn;
use crate::chat::ChatHistory;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntityDiagnostics;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::explosion::PrimedTnt;
use crate::entity::falling_block::FallingBlock;
use crate::entity::mobs::Mob;
use crate::entity::particles::block::BlockParticles;
use crate::entity::particles::rain::RainParticles;
use crate::entity::projectiles::Arrow;
use crate::entity::projectiles::Fireball;
use crate::entity::shadow::ShadowOwner;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::player::interaction::overlay::BlockFocus;
use crate::player::sleep::BED_MISSING_MESSAGE;
use crate::player::sleep::PlayerSleep;
use crate::player::sleep::bed_chunks;
use crate::player::sleep::bed_respawn_feet;
use crate::rendering::sky::SkyAnchor;
use crate::rendering::weather::LightningBolt;
use crate::rendering::weather::SkyFlash;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::ActiveDimension;
use crate::world::dimension::Dimension;
use crate::world::generation::ChunkGenerator;
use crate::world::generation::WorldGeneration;
use crate::world::generation::generate_area;
use crate::world::generation::nether::NetherGenerator;
use crate::world::generation::overworld::OverworldGenerator;
use crate::world::lighting::LightCache;
use crate::world::persistence::PendingWorld;
use crate::world::persistence::PersistenceConfig;
use crate::world::persistence::SaveFormat;
use crate::world::persistence::WorldPersistence;
use crate::world::persistence::WorldStorage;
use crate::world::persistence::activate_pending_world;
use crate::world::portal;
use crate::world::portal::PortalArea;
use crate::world::portal::PortalChange;
use crate::world::streaming::SpawnAreaTask;
use crate::world::streaming::StreamingDiagnostics;
use crate::world::streaming::WorldStreaming;
use crate::world::streaming::setup_streaming;
use crate::world::streaming::start_spawn_area;
use crate::world::weather::WorldWeather;

/// The world to play.
#[derive(Debug, Clone)]
pub enum WorldChoice {
    /// A world folder already in the saves directory.
    Existing(PathBuf),
    New {
        name: String,
        seed: u64,
        difficulty: Difficulty,
        format: SaveFormat,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No world exists.
    #[default]
    Empty,
    /// The load chain is installing a world.
    Loading,
    Active,
    /// Unloading: the world holds still while the jobs that own chunks finish.
    Settling,
    /// Unloading: the final save is draining.
    Saving,
    /// Changing dimension: the old one is unloaded and the teleporter is
    /// finding or building the way out in the new one.
    Arriving,
}

/// Why the player is changing dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Travel {
    /// `Minecraft.usePortal`: through a portal to the other dimension, coming
    /// out of the nearest portal there or a new one.
    Portal,
    /// `Minecraft.respawn` where `canRespawnHere` is false: back to the
    /// Overworld's spawn point. Beta sends the player through the teleporter
    /// first and can leave a stray portal behind; that is not copied.
    Respawn,
}

/// What the teleporter worked out on its background task.
struct Arrival {
    /// Chunks to put straight into the world, and whether each still needs
    /// saving: the surroundings a new portal was built in.
    chunks: Vec<(ChunkPosition, GeneratedChunk, bool)>,
    changes: Vec<PortalChange>,
    /// Where the player's feet go, or `None` to stay at the scaled position.
    feet: Option<DVec3>,
    /// A respawn found the player's bed gone or boxed in.
    bed_missing: bool,
}

/// An [`Arrival`] waiting for the chain that resumes the world.
#[derive(Resource)]
struct PendingArrival {
    arrival: Option<Arrival>,
    /// Set once the arrival is admitted: it stood the player somewhere.
    placed: bool,
    bed_missing: bool,
}

/// `Entity.yOffset` for the player. `usePortal` calls `setLocationAndAngles`
/// twice with the player's own `posY`, and each call adds this again, so the
/// teleporter measures from that much above the eyes.
const PORTAL_Y_DRIFT: f64 = 2.0 * 1.62;
/// Chunks around the arrival point that are generated before a portal is
/// built, enough to hold `Teleporter.createPortal`'s 16 block search and the
/// frame it writes.
const ARRIVAL_CHUNK_RADIUS: i32 = 2;

/// Seconds between attempts at a final save that could not write everything.
const SAVE_RETRY_SECONDS: f32 = 2.0;

const SAVING_NOTICE: &str = "Saving world...";
const LOADING_NOTICE: &str = "Loading world...";

#[derive(Resource, Default)]
pub struct WorldSession {
    phase: Phase,
    requested: Option<WorldChoice>,
    leave: bool,
    /// What the world list shows about a save in progress or a failure.
    notice: Option<String>,
    /// Seconds until a final save that left chunks unwritten is tried again.
    retry_in: f32,
    /// When the world being loaded was opened, for the load time in the log.
    load_started: Option<Instant>,
    /// A dimension change asked for and not started yet.
    travel: Option<Travel>,
    /// The dimension change in progress and where it leads.
    travelling: Option<(Travel, Dimension)>,
    arrival: Option<Task<Arrival>>,
}

impl WorldSession {
    /// A world is loaded and being played.
    pub fn is_active(&self) -> bool {
        self.phase == Phase::Active
    }

    /// A world is loading, saving or unloading, so another cannot start yet.
    pub fn is_busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Loading | Phase::Settling | Phase::Saving | Phase::Arriving
        )
    }

    /// Why a world cannot be chosen right now, or why the last one did not
    /// load: a save still running, a save that keeps failing, or a world that
    /// could not be opened.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The world waiting to load, if one was requested and has not started.
    pub fn pending_choice(&self) -> Option<&WorldChoice> {
        self.requested.as_ref()
    }

    /// Play `choice`, saving and unloading the current world first if needed.
    pub fn request_load(&mut self, choice: WorldChoice) {
        self.requested = Some(choice);
        if !self.is_busy() {
            self.notice = None;
        }
    }

    /// Move the player to the other dimension, saving and unloading this one
    /// first. Ignored unless a world is being played.
    pub fn request_travel(&mut self, travel: Travel) {
        if self.phase == Phase::Active && self.travelling.is_none() {
            self.travel = Some(travel);
        }
    }

    /// The dimension the player is on the way to, while the change is under
    /// way. The world is unloaded for the duration.
    pub fn travelling_to(&self) -> Option<Dimension> {
        self.travelling.map(|(_, target)| target)
    }

    /// Save and unload the current world, dropping any pending load.
    pub fn request_leave(&mut self) {
        self.requested = None;
        self.leave = true;
    }
}

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSession>()
            .init_resource::<ActiveDimension>()
            .init_resource::<PauseMenu>()
            .init_resource::<SettingsReturn>()
            .add_systems(Update, drive_session)
            .add_systems(
                Update,
                (
                    // Once only: a second pass would find the storage taken
                    // and install a world that saves nowhere.
                    (activate_pending_world, start_spawn_area)
                        .chain()
                        .run_if(not(resource_exists::<SpawnAreaTask>)),
                    setup_streaming,
                    // These wait for the frame the spawn area arrives in.
                    (crate::player::spawn_player, finish_load)
                        .chain()
                        .run_if(resource_exists::<WorldStreaming>),
                )
                    .chain()
                    .after(drive_session)
                    .run_if(resource_exists::<PendingWorld>),
            )
            .add_systems(
                Update,
                (admit_arrival, setup_streaming, finish_arrival)
                    .chain()
                    .after(drive_session)
                    .run_if(resource_exists::<PendingArrival>),
            );
    }
}

/// Everything that holds one world's state and is reset when it unloads.
#[derive(SystemParam)]
struct WorldState<'w, 's> {
    chunks: ResMut<'w, WorldChunks>,
    light: ResMut<'w, LightCache>,
    ticks: ResMut<'w, BlockTicks>,
    particles: ResMut<'w, BlockParticles>,
    rain_particles: Option<ResMut<'w, RainParticles>>,
    weather: Option<ResMut<'w, WorldWeather>>,
    sky_flash: Option<ResMut<'w, SkyFlash>>,
    inventory: ResMut<'w, InventorySession>,
    workbench: ResMut<'w, ActiveWorkbench>,
    focus: ResMut<'w, BlockFocus>,
    pause: ResMut<'w, PauseMenu>,
    settings_return: ResMut<'w, SettingsReturn>,
    settings: Option<ResMut<'w, GameSettings>>,
    client_difficulty: Option<ResMut<'w, ClientDifficulty>>,
    streaming_perf: Option<ResMut<'w, StreamingDiagnostics>>,
    entity_perf: Option<ResMut<'w, EntityDiagnostics>>,
    dimension: ResMut<'w, ActiveDimension>,
    player: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Velocity,
            &'static mut CollisionState,
            &'static mut PlayerInterpolation,
        ),
        With<Player>,
    >,
    sleep: Query<'w, 's, &'static PlayerSleep, With<Player>>,
    /// What a change of dimension leaves standing.
    kept: Query<'w, 's, (), Or<(With<Player>, With<SkyAnchor>, With<ShadowOwner>)>>,
    entities: Query<
        'w,
        's,
        Entity,
        Or<(
            With<Mob>,
            With<DroppedItem>,
            With<FallingBlock>,
            With<PrimedTnt>,
            With<Arrow>,
            With<Fireball>,
            With<LightningBolt>,
            With<ShadowOwner>,
            With<Player>,
            With<SkyAnchor>,
        )>,
    >,
}

fn drive_session(
    mut commands: Commands,
    mut session: ResMut<WorldSession>,
    time: Res<Time<Real>>,
    config: Option<Res<PersistenceConfig>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut quads: Option<ResMut<crate::rendering::chunk_quads::ChunkQuads>>,
    generation: Option<Res<WorldGeneration>>,
    mut state: WorldState,
) {
    // A leave outranks a dimension change, including one already saving.
    if session.leave || session.requested.is_some() {
        session.travel = None;
        if matches!(session.phase, Phase::Settling | Phase::Saving) {
            session.travelling = None;
            session.notice = Some(SAVING_NOTICE.to_owned());
        }
    }
    if session.phase == Phase::Active
        && let Some(travel) = session.travel.take()
    {
        let target = match travel {
            Travel::Portal => state.dimension.0.other(),
            Travel::Respawn => Dimension::Overworld,
        };
        if let Some(streaming) = streaming.as_deref_mut() {
            streaming.halt();
        }
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.begin_closing();
        }
        session.travelling = Some((travel, target));
        session.phase = Phase::Settling;
    }
    if session.phase == Phase::Arriving {
        // The chain that resumes the world takes it from here.
        if let Some(arrival) = session.arrival.as_mut().and_then(check_ready) {
            session.arrival = None;
            commands.insert_resource(PendingArrival {
                arrival: Some(arrival),
                placed: false,
                bed_missing: false,
            });
        }
        return;
    }
    // A leave or a different world asked for while one is being played. A
    // leave asked for with no world loaded has nothing to do.
    if session.phase == Phase::Active && (session.leave || session.requested.is_some()) {
        // Nothing may change behind the final save: the clock stops, and no
        // job starts that would hand a chunk back after its snapshot.
        if let Some(streaming) = streaming.as_deref_mut() {
            streaming.halt();
        }
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.begin_closing();
        }
        session.notice = Some(SAVING_NOTICE.to_owned());
        session.phase = Phase::Settling;
    }
    if session.phase != Phase::Loading {
        session.leave = false;
    }
    session.travel = None;
    if session.phase == Phase::Settling {
        // Generation jobs deliver new chunks and population jobs hold chunks
        // outside `WorldChunks`, writing into all four when they finish. The
        // save starts after the last of them so it sees every chunk once.
        let settled = streaming.as_deref().is_none_or(|streaming| {
            streaming.generating_job_count() == 0 && streaming.populating_job_count() == 0
        });
        if !settled {
            return;
        }
        // Write the player, mobs and every dirty chunk before unloading.
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.request_final_save();
        }
        session.retry_in = 0.0;
        session.phase = Phase::Saving;
        return;
    }
    if session.phase == Phase::Saving {
        if let Some(persistence) = persistence
            .as_deref_mut()
            .filter(|persistence| persistence.storage().is_some())
        {
            if !persistence.is_idle() {
                return;
            }
            // A drain writes each chunk once and ends even when a write
            // failed, so idle alone does not mean the world is on disk.
            if let Some(error) = persistence.save_error() {
                // Leave a failing disk alone for a moment between attempts.
                if session.retry_in <= 0.0 {
                    session.notice = Some(format!("Could not save the world, retrying: {error}"));
                    session.retry_in = SAVE_RETRY_SECONDS;
                } else {
                    session.retry_in -= time.delta_secs();
                    if session.retry_in <= 0.0 {
                        persistence.request_final_save();
                    }
                }
                return;
            }
            if persistence.has_unsaved_chunks() {
                persistence.request_final_save();
                return;
            }
            session.retry_in = 0.0;
        }
        if let Some(streaming) = streaming.as_deref_mut() {
            streaming.despawn_rendered(&mut commands, &mut meshes, quads.as_deref_mut());
        }
        if let Some((travel, target)) = session.travelling {
            // Only this dimension's state goes; the player, the clock, the
            // weather and the storage stay.
            commands.remove_resource::<WorldStreaming>();
            for entity in &state.entities {
                if !state.kept.contains(entity) {
                    commands.entity(entity).despawn();
                }
            }
            state.chunks.clear();
            state.light.clear();
            let world_time = state.ticks.time();
            *state.ticks = BlockTicks::default();
            state.ticks.rebase_time(0, world_time);
            state.ticks.set_dimension(target);
            *state.particles = BlockParticles::default();
            *state.inventory = InventorySession::default();
            *state.workbench = ActiveWorkbench::default();
            *state.focus = BlockFocus::default();
            if let Some(perf) = state.streaming_perf.as_deref_mut() {
                *perf = StreamingDiagnostics::default();
            }

            let source = state.dimension.0;
            state.dimension.0 = target;
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.set_dimension(target);
            }
            let mut entity = None;
            // `Minecraft.respawn`: back to the bed the player woke up in.
            let bed = (travel == Travel::Respawn)
                .then(|| state.sleep.single().ok().and_then(|sleep| sleep.spawn))
                .flatten();
            if let Ok((mut transform, mut velocity, mut collision, mut interpolation)) =
                state.player.single_mut()
            {
                match travel {
                    Travel::Portal => {
                        let (x, z) = source.scale_position_to(
                            target,
                            f64::from(transform.translation.x),
                            f64::from(transform.translation.z),
                        );
                        transform.translation.x = x as f32;
                        transform.translation.z = z as f32;
                        entity = Some(DVec3::new(
                            x,
                            f64::from(transform.translation.y) + PORTAL_Y_DRIFT,
                            z,
                        ));
                    }
                    // The spawn chunk is built around here; the height is
                    // settled once it exists.
                    Travel::Respawn => {
                        let (x, z) =
                            bed.map_or((8.5, 8.5), |bed| (bed.x as f32 + 0.5, bed.z as f32 + 0.5));
                        transform.translation.x = x;
                        transform.translation.z = z;
                    }
                }
                velocity.0 = Vec3::ZERO;
                *collision = CollisionState::default();
                interpolation.previous_position = transform.translation;
            }
            let storage = persistence
                .as_deref()
                .and_then(|persistence| persistence.storage().cloned());
            let seed = storage.as_ref().map_or(0, |storage| storage.seed());
            let generator = generation.map_or_else(
                || dimension_generator(target, seed),
                |generation| Arc::clone(&generation.0),
            );
            // Beta's teleporter draws the rotation it tries first from an
            // unseeded random.
            let rotation = time.elapsed().subsec_nanos();
            session.arrival = Some(AsyncComputeTaskPool::get().spawn(async move {
                match (entity, bed) {
                    (Some(entity), _) => {
                        teleport(storage.as_deref(), &*generator, target, entity, rotation)
                    }
                    (None, Some(bed)) => respawn_at_bed(storage.as_deref(), target, bed),
                    (None, None) => Arrival {
                        chunks: Vec::new(),
                        changes: Vec::new(),
                        feet: None,
                        bed_missing: false,
                    },
                }
            }));
            session.phase = Phase::Arriving;
            return;
        }
        commands.remove_resource::<WorldStreaming>();
        commands.remove_resource::<WorldPersistence>();
        commands.remove_resource::<WorldGeneration>();
        for entity in &state.entities {
            commands.entity(entity).despawn();
        }
        state.chunks.clear();
        state.light.clear();
        *state.ticks = BlockTicks::default();
        *state.particles = BlockParticles::default();
        if let Some(rain) = state.rain_particles.as_deref_mut() {
            *rain = RainParticles::default();
        }
        if let Some(weather) = state.weather.as_deref_mut() {
            *weather = WorldWeather::default();
        }
        if let Some(flash) = state.sky_flash.as_deref_mut() {
            flash.0 = 0;
        }
        *state.inventory = InventorySession::default();
        *state.workbench = ActiveWorkbench::default();
        *state.focus = BlockFocus::default();
        state.pause.open = false;
        *state.settings_return = SettingsReturn::default();
        state.dimension.0 = Dimension::Overworld;
        // The world's difficulty goes with it.
        if let Some(client) = state.client_difficulty.as_deref_mut()
            && let Some(difficulty) = client.0.take()
            && let Some(settings) = state.settings.as_deref_mut()
        {
            settings.difficulty = difficulty;
        }
        // The next report should not mix this world's timings into the next.
        if let Some(perf) = state.streaming_perf.as_deref_mut() {
            *perf = StreamingDiagnostics::default();
        }
        if let Some(perf) = state.entity_perf.as_deref_mut() {
            *perf = EntityDiagnostics::default();
        }
        session.notice = None;
        session.phase = Phase::Empty;
    }

    if session.phase != Phase::Empty {
        return;
    }
    let Some(choice) = session.requested.take() else {
        return;
    };
    let Some(config) = config else {
        warn!("Cannot load a world without persistence configured");
        session.notice = Some("Could not open the world: saving is not set up".to_owned());
        return;
    };
    let storage = match choice {
        WorldChoice::Existing(root) => WorldStorage::open(root),
        WorldChoice::New {
            name,
            seed,
            difficulty,
            format,
        } => WorldStorage::create_in_format(
            &config.saves_directory,
            seed,
            &name,
            Some(difficulty),
            format,
        ),
    };
    match storage {
        Ok(storage) => {
            commands.insert_resource(PendingWorld(Some(storage)));
            session.notice = Some(LOADING_NOTICE.to_owned());
            session.load_started = Some(Instant::now());
            session.phase = Phase::Loading;
        }
        Err(error) => {
            warn!("Could not open the world: {error}");
            session.notice = Some(format!("Could not open the world: {error}"));
        }
    }
}

fn finish_load(
    mut commands: Commands,
    mut session: ResMut<WorldSession>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    commands.remove_resource::<PendingWorld>();
    if let Some(started) = session.load_started.take() {
        info!(
            "world opened in {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    session.notice = None;
    session.phase = Phase::Active;
    next_screen.set(AppScreen::Playing);
}

fn dimension_generator(dimension: Dimension, seed: u64) -> Arc<dyn ChunkGenerator> {
    match dimension {
        Dimension::Overworld => Arc::new(OverworldGenerator::new(seed)),
        Dimension::Nether => Arc::new(NetherGenerator::new(seed)),
    }
}

/// `Teleporter.placeInPortal` in `dimension`, which is not loaded. `entity`
/// is the player's position there.
///
/// A generator never makes a portal block, so the search for an existing
/// portal only reads chunks that were saved, and the final save before this
/// put every one of them on disk. Building a portal needs real terrain: the
/// surroundings are generated, saved chunks taking precedence, and handed back
/// to become the first chunks of the new dimension.
fn teleport(
    storage: Option<&WorldStorage>,
    generator: &dyn ChunkGenerator,
    dimension: Dimension,
    entity: DVec3,
    rotation: u32,
) -> Arrival {
    let load = |position| storage.and_then(|storage| storage.load_chunk_in(dimension, position));
    let mut saved = PortalArea::default();
    for position in portal::chunks_within(entity.x, entity.z, portal::SEARCH_RADIUS, 0) {
        if let Some(chunk) = load(position).filter(portal::has_portal) {
            saved.insert(position, chunk);
        }
    }
    if let Some(feet) = portal::find_exit(&saved, entity) {
        return Arrival {
            chunks: Vec::new(),
            changes: Vec::new(),
            feet: Some(feet),
            bed_missing: false,
        };
    }

    let center = ChunkPosition::from_block(entity.x.floor() as i32, entity.z.floor() as i32);
    let mut generated = generate_area(generator, center, ARRIVAL_CHUNK_RADIUS);
    let mut loaded = std::collections::HashSet::new();
    for (position, chunk) in &mut generated {
        if let Some(stored) = load(*position) {
            *chunk = stored;
            loaded.insert(*position);
        }
    }
    let mut area = PortalArea::new(generated);
    let feet = portal::place_in_portal(&mut area, entity, rotation);
    let (chunks, changes) = area.into_parts();
    let edited: std::collections::HashSet<_> = changes
        .iter()
        .map(|change| ChunkPosition::from_block(change.position.x, change.position.z))
        .collect();
    Arrival {
        chunks: chunks
            .into_iter()
            .map(|(position, chunk)| {
                let unsaved = !loaded.contains(&position) || edited.contains(&position);
                (position, chunk, unsaved)
            })
            .collect(),
        changes,
        feet,
        bed_missing: false,
    }
}

/// `EntityPlayer.func_25060_a` in `dimension`, which is not loaded: the final
/// save before this put the bed's chunks on disk, so they are read from
/// there. The world then streams in around wherever the player ends up.
fn respawn_at_bed(storage: Option<&WorldStorage>, dimension: Dimension, bed: IVec3) -> Arrival {
    let mut chunks = WorldChunks::default();
    for position in bed_chunks(bed) {
        if let Some(chunk) = storage.and_then(|storage| storage.load_chunk_in(dimension, position))
        {
            chunks.insert(position, chunk);
        }
    }
    let feet = bed_respawn_feet(&chunks, bed);
    Arrival {
        chunks: Vec::new(),
        changes: Vec::new(),
        feet: feet.map(|feet| feet.as_dvec3()),
        bed_missing: feet.is_none(),
    }
}

/// Put what the teleporter built into the empty world and stand the player
/// in the portal, before streaming starts around them.
fn admit_arrival(
    mut commands: Commands,
    mut pending: ResMut<PendingArrival>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut player: Query<
        (
            &mut Transform,
            &mut Velocity,
            &mut CollisionState,
            &mut PlayerInterpolation,
        ),
        With<Player>,
    >,
) {
    let Some(arrival) = pending.arrival.take() else {
        return;
    };
    pending.placed = arrival.feet.is_some();
    pending.bed_missing = arrival.bed_missing;
    // The world spawn is where the world is built instead.
    if arrival.bed_missing
        && let Ok((mut transform, ..)) = player.single_mut()
    {
        transform.translation.x = 8.5;
        transform.translation.z = 8.5;
    }
    for (position, chunk, unsaved) in arrival.chunks {
        if unsaved && let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(position);
        }
        crate::world::streaming::admit_chunk(
            &mut commands,
            &mut chunks,
            ticks.as_deref_mut(),
            position,
            chunk,
        );
    }
    if let Some(ticks) = ticks.as_deref_mut() {
        for change in arrival.changes {
            ticks.block_changed(change.position, change.previous, change.previous_metadata);
        }
    }
    if let Some(feet) = arrival.feet
        && let Ok((mut transform, mut velocity, mut collision, mut interpolation)) =
            player.single_mut()
    {
        // `setLocationAndAngles(x, y, z, rotationYaw, 0)`: the yaw is kept
        // and the pitch levelled.
        let (yaw, ..) = transform.rotation.to_euler(EulerRot::YXZ);
        transform.rotation = Quat::from_rotation_y(yaw);
        transform.translation = Vec3::new(
            feet.x as f32,
            feet.y as f32 + EntitySize::PLAYER.y_offset,
            feet.z as f32,
        );
        velocity.0 = Vec3::ZERO;
        *collision = CollisionState::default();
        interpolation.previous_position = transform.translation;
    }
}

fn finish_arrival(
    mut commands: Commands,
    mut session: ResMut<WorldSession>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    chunks: Res<WorldChunks>,
    pending: Option<Res<PendingArrival>>,
    mut chat: Option<ResMut<ChatHistory>>,
    mut player: Query<(&mut Transform, &mut PlayerInterpolation, &mut PlayerSleep), With<Player>>,
) {
    let placed = pending.as_ref().is_some_and(|pending| pending.placed);
    let bed_missing = pending.as_ref().is_some_and(|pending| pending.bed_missing);
    if let Some((Travel::Respawn, _)) = session.travelling
        && let Ok((mut transform, mut interpolation, mut sleep)) = player.single_mut()
    {
        if !placed {
            *transform = crate::player::default_spawn_transform(&chunks);
            interpolation.previous_position = transform.translation;
        }
        if bed_missing {
            sleep.spawn = None;
            if let Some(chat) = chat.as_deref_mut() {
                chat.push(BED_MISSING_MESSAGE);
            }
        }
    }
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.resume();
    }
    commands.remove_resource::<PendingArrival>();
    session.travelling = None;
    session.phase = Phase::Active;
}

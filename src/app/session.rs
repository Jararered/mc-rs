//! Which world is loaded, and loading or leaving one while the game runs.
//!
//! The game starts with no world. Choosing one on the title screen calls
//! [`WorldSession::request_load`]; leaving to the title screen calls
//! [`WorldSession::request_leave`], which saves the world and unloads it.
//!
//! Loading reuses the startup systems that used to run once: the chosen
//! storage is installed, the spawn area is built, and the player is spawned,
//! in that order, before the game switches to [`AppScreen::Playing`].

use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::app::settings::Difficulty;
use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::app::state::SettingsReturn;
use crate::entity::DroppedItem;
use crate::entity::explosion::PrimedTnt;
use crate::entity::falling_block::FallingBlock;
use crate::entity::mobs::Mob;
use crate::entity::particles::block::BlockParticles;
use crate::entity::projectiles::Arrow;
use crate::entity::projectiles::Fireball;
use crate::entity::shadow::ShadowOwner;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::player::Player;
use crate::player::interaction::overlay::BlockFocus;
use crate::rendering::sky::SkyAnchor;
use crate::rendering::weather::LightningFlash;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::WorldChunks;
use crate::world::generation::WorldGeneration;
use crate::world::lighting::LightCache;
use crate::world::persistence::PendingWorld;
use crate::world::persistence::PersistenceConfig;
use crate::world::persistence::SaveFormat;
use crate::world::persistence::WorldPersistence;
use crate::world::persistence::WorldStorage;
use crate::world::persistence::activate_pending_world;
use crate::world::streaming::WorldStreaming;
use crate::world::streaming::setup_streaming;
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
    /// Saving before unloading. `true` once the save has been requested.
    Leaving(bool),
}

#[derive(Resource, Default)]
pub struct WorldSession {
    phase: Phase,
    requested: Option<WorldChoice>,
    leave: bool,
}

impl WorldSession {
    /// A world is loaded and being played.
    pub fn is_active(&self) -> bool {
        self.phase == Phase::Active
    }

    /// A world is loading, saving or unloading, so another cannot start yet.
    pub fn is_busy(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Leaving(_))
    }

    /// The world waiting to load, if one was requested and has not started.
    pub fn pending_choice(&self) -> Option<&WorldChoice> {
        self.requested.as_ref()
    }

    /// Play `choice`, saving and unloading the current world first if needed.
    pub fn request_load(&mut self, choice: WorldChoice) {
        self.requested = Some(choice);
        self.leave = self.phase == Phase::Active;
    }

    /// Save and unload the current world, dropping any pending load.
    pub fn request_leave(&mut self) {
        self.requested = None;
        self.leave = self.phase == Phase::Active;
    }
}

pub struct SessionPlugin;

impl Plugin for SessionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSession>()
            .init_resource::<PauseMenu>()
            .init_resource::<SettingsReturn>()
            .add_systems(Update, drive_session)
            .add_systems(
                Update,
                (
                    activate_pending_world,
                    setup_streaming,
                    crate::player::spawn_player,
                    finish_load,
                )
                    .chain()
                    .after(drive_session)
                    .run_if(resource_exists::<PendingWorld>),
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
    weather: Option<ResMut<'w, WorldWeather>>,
    inventory: ResMut<'w, InventorySession>,
    workbench: ResMut<'w, ActiveWorkbench>,
    focus: ResMut<'w, BlockFocus>,
    pause: ResMut<'w, PauseMenu>,
    settings_return: ResMut<'w, SettingsReturn>,
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
            With<LightningFlash>,
            With<ShadowOwner>,
            With<Player>,
            With<SkyAnchor>,
        )>,
    >,
}

fn drive_session(
    mut commands: Commands,
    mut session: ResMut<WorldSession>,
    config: Option<Res<PersistenceConfig>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut state: WorldState,
) {
    if session.phase == Phase::Active && session.leave {
        session.leave = false;
        session.phase = Phase::Leaving(false);
    }
    if let Phase::Leaving(requested) = session.phase {
        if !requested {
            // Write the player, mobs and every dirty chunk before unloading.
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.request_save();
            }
            session.phase = Phase::Leaving(true);
            return;
        }
        let saved = persistence
            .as_deref()
            .is_none_or(|persistence| persistence.storage().is_none() || persistence.is_idle());
        // Population jobs hold chunks outside `WorldChunks`; wait for them so
        // the save sees every chunk.
        let settled = streaming
            .as_deref()
            .is_none_or(|streaming| streaming.populating_job_count() == 0);
        if !saved || !settled {
            return;
        }
        if let Some(streaming) = streaming.as_deref_mut() {
            streaming.despawn_rendered(&mut commands, &mut meshes);
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
        if let Some(weather) = state.weather.as_deref_mut() {
            *weather = WorldWeather::default();
        }
        *state.inventory = InventorySession::default();
        *state.workbench = ActiveWorkbench::default();
        *state.focus = BlockFocus::default();
        state.pause.open = false;
        *state.settings_return = SettingsReturn::default();
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
            session.phase = Phase::Loading;
        }
        Err(error) => warn!("Could not open the world: {error}"),
    }
}

fn finish_load(
    mut commands: Commands,
    mut session: ResMut<WorldSession>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    commands.remove_resource::<PendingWorld>();
    session.phase = Phase::Active;
    next_screen.set(AppScreen::Playing);
}

//! The systems that open a world and drive its autosave each frame.

use super::PendingWorld;
use super::PersistenceConfig;
use super::SaveBatch;
use super::WorldPersistence;
use super::player::StoredPlayer;
use super::player::full_player_health;
use super::storage::WorldStorage;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::player::Player;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::ActiveDimension;
use crate::world::dimension::Dimension;
use crate::world::lighting::LightCache;
use bevy::prelude::*;
use std::collections::HashMap;

pub(super) fn setup_persistence(
    mut commands: Commands,
    config: Res<PersistenceConfig>,
    mut tick: Option<ResMut<crate::world::tick::WorldTick>>,
    mut weather: Option<ResMut<crate::world::weather::WorldWeather>>,
    mut settings: Option<ResMut<crate::app::settings::GameSettings>>,
    mut client_difficulty: Option<ResMut<crate::world::difficulty::ClientDifficulty>>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
) {
    match WorldStorage::open_latest_or_create(&config.saves_directory, config.seed) {
        Ok(storage) => install_world(
            &mut commands,
            storage,
            config.autosave_seconds,
            block_ticks.as_deref_mut(),
            tick.as_deref_mut(),
            weather.as_deref_mut(),
            settings.as_deref_mut(),
            client_difficulty.as_deref_mut(),
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
    mut client_difficulty: Option<ResMut<crate::world::difficulty::ClientDifficulty>>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
) {
    match pending.0.take() {
        Some(storage) => install_world(
            &mut commands,
            storage,
            config.autosave_seconds,
            block_ticks.as_deref_mut(),
            tick.as_deref_mut(),
            weather.as_deref_mut(),
            settings.as_deref_mut(),
            client_difficulty.as_deref_mut(),
        ),
        None => commands.insert_resource(WorldPersistence::disabled()),
    }
}

/// Makes `storage` the live world: its clock, weather and difficulty become the
/// game's, and a [`WorldPersistence`] starts saving into it.
pub(super) fn install_world(
    commands: &mut Commands,
    storage: WorldStorage,
    autosave_seconds: f32,
    block_ticks: Option<&mut BlockTicks>,
    tick: Option<&mut crate::world::tick::WorldTick>,
    weather: Option<&mut crate::world::weather::WorldWeather>,
    settings: Option<&mut crate::app::settings::GameSettings>,
    client_difficulty: Option<&mut crate::world::difficulty::ClientDifficulty>,
) {
    let manifest = storage.manifest();
    // A world saved in the Nether opens there.
    let dimension = storage
        .load_player()
        .map_or_else(Dimension::default, |player| player.dimension);
    storage.set_dimension(dimension);
    commands.insert_resource(ActiveDimension(dimension));
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
    if let Some(block_ticks) = block_ticks {
        // Chunks carry their pending ticks as delays from now. Without this
        // the ones loaded before the first world tick would all be overdue.
        let previous = block_ticks.time();
        block_ticks.rebase_time(previous, manifest.world_time);
        block_ticks.set_dimension(dimension);
    }
    if let Some(weather) = weather {
        *weather = manifest.weather;
    }
    if let (Some(settings), Some(difficulty)) = (settings, manifest.difficulty) {
        // The world's difficulty is the game's while it is loaded; the
        // player's own option is kept aside for `settings.json`.
        if let Some(client) = client_difficulty {
            client.0.get_or_insert(settings.difficulty);
        }
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
pub(super) fn flush_persistence(
    mut persistence: ResMut<WorldPersistence>,
    mut chunks: ResMut<WorldChunks>,
    player: Query<
        (
            &Transform,
            Option<&Hotbar>,
            Option<&Inventory>,
            Option<&crate::entity::Flying>,
            &crate::player::FlySpeed,
            &crate::player::GameMode,
            Option<&crate::player::PlayerHealth>,
            Option<&crate::player::PlayerSurvival>,
            Option<&crate::player::sleep::PlayerSleep>,
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
    light: Option<Res<LightCache>>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    mut exit: MessageReader<AppExit>,
    pause: Option<Res<crate::app::state::PauseMenu>>,
    screen: Option<Res<State<crate::app::state::AppScreen>>>,
    (settings, client_difficulty): (
        Option<Res<crate::app::settings::GameSettings>>,
        Option<Res<crate::world::difficulty::ClientDifficulty>>,
    ),
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
        // A world that records its difficulty follows the settings screen.
        if let Some(settings) = settings.as_deref()
            && client_difficulty.is_some_and(|client| client.0.is_some())
        {
            storage.set_difficulty(settings.difficulty);
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
        for (_, transform, falling, tnt, velocity, minecart, cargo, boat) in &bodies {
            if let Some(body) = crate::entity::SavedBody::capture(
                transform, falling, tnt, velocity, minecart, cargo, boat,
            ) {
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
                |(
                    transform,
                    hotbar,
                    inventory,
                    flying,
                    fly_speed,
                    game_mode,
                    health,
                    survival,
                    sleep,
                )| {
                    (
                        transform,
                        hotbar,
                        inventory,
                        flying.is_some(),
                        fly_speed.0,
                        *game_mode,
                        health.map_or(full_player_health(), |health| health.current),
                        survival,
                        sleep,
                    )
                },
            ),
            &items,
            block_ticks.as_deref(),
            light.as_deref(),
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
    let dimension = persistence
        .storage()
        .map_or_else(Dimension::default, |storage| storage.dimension());
    let record = if persistence.player_pending {
        player.single().ok().map(
            |(
                transform,
                hotbar,
                inventory,
                flying,
                fly_speed,
                game_mode,
                health,
                survival,
                sleep,
            )| {
                StoredPlayer::from_transform(transform)
                    .with_dimension(dimension)
                    .with_flying(flying.is_some(), fly_speed.0)
                    .with_game_mode(*game_mode)
                    .with_health(health.map_or(full_player_health(), |health| health.current))
                    .with_survival(survival)
                    .with_sleep(sleep)
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
    persistence.pump(
        &chunks,
        &items,
        &ticks,
        light.as_deref(),
        record.as_ref(),
        paused,
    );
    if !persistence.has_work() && !persistence.player_pending && !persistence.write_in_flight() {
        if std::mem::take(&mut persistence.manifest_pending) && persistence.storage.is_some() {
            // Close the drain with one manifest write, after every chunk, so
            // the clock and weather it records are never ahead of the chunks.
            persistence.submit(SaveBatch {
                positions: Vec::new(),
                chunks: Vec::new(),
                player: None,
                manifest: true,
            });
        } else {
            persistence.finish_drain();
        }
    }
}

/// The live dropped items, grouped by the chunk they would be saved into.
pub(super) fn dropped_items_by_chunk(
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

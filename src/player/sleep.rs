//! Sleeping in a bed: Beta's `BlockBed.blockActivated`,
//! `EntityPlayer.sleepInBedAt` and `wakeUpPlayer`, the night skip in
//! `World.tick`, and the bed spawn point `Minecraft.respawn` returns to.
//!
//! Not copied: `SpawnerAnimals.performSleepSpawning` (the monster that wakes
//! a sleeper), the lying third-person pose, and the bed-relative turn of the
//! camera; the player keeps looking where they looked.

use bevy::prelude::*;

use super::Player;
use super::PlayerHealth;
use super::PlayerInterpolation;
use crate::app::state::AppScreen;
use crate::block::bed;
use crate::block::blocks::Block;
use crate::chat::ChatHistory;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::combat::Source;
use crate::entity::explosion::Explosion;
use crate::physics::PhysicsSet;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::ActiveDimension;
use crate::world::dimension::Dimension;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::DAY_LENGTH;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;

/// `tile.bed.noSleep`.
pub const NO_SLEEP_MESSAGE: &str = "You can only sleep at night";
/// `tile.bed.notValid`.
pub const BED_MISSING_MESSAGE: &str = "Your home bed was missing or obstructed";
/// Beta sets the spawn point silently; this game says so.
pub const SPAWN_SET_MESSAGE: &str = "Spawn point set";

/// Ticks of sleep before the night is skipped, and the timer's resting value
/// while asleep.
pub const FULLY_ASLEEP_TICKS: u8 = 100;
/// The timer runs on this far after waking, fading the screen back in.
const WAKE_FADE_TICKS: u8 = 10;
/// `EntityPlayer.sleepInBedAt` puts the sleeper this far above the bed's
/// floor.
const LYING_HEIGHT: f32 = 0.9375;
/// `World.isDaytime`: fewer than four light levels are taken from the sky.
const DAYTIME_SKYLIGHT_SUBTRACTED: u8 = 4;

/// `EntityPlayer`'s `sleeping`, `sleepTimer`, `bedChunkCoordinates` and
/// `playerSpawnCoordinate`. Only the spawn point is saved; a world saved
/// mid-sleep opens with the player awake, as Beta wakes one on load.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct PlayerSleep {
    pub sleeping: bool,
    /// Counts to [`FULLY_ASLEEP_TICKS`] while asleep, then ten more after
    /// waking in the morning.
    pub timer: u8,
    /// The bed's foot half, as `BlockBed.blockActivated` resolves it.
    pub bed: Option<IVec3>,
    /// The bed the player last woke up in.
    pub spawn: Option<IVec3>,
    /// Health as of the last sleeping tick; a drop wakes the player.
    last_health: u8,
}

impl PlayerSleep {
    pub fn with_spawn(spawn: Option<IVec3>) -> Self {
        Self { spawn, ..default() }
    }

    /// `GuiIngame`'s sleep fade, `0..=1`.
    pub fn fade(&self) -> f32 {
        let timer = f32::from(self.timer);
        let full = f32::from(FULLY_ASLEEP_TICKS);
        if timer > full {
            1.0 - (timer - full) / f32::from(WAKE_FADE_TICKS)
        } else {
            timer / full
        }
    }
}

/// A player right-clicked a bed block.
#[derive(Message, Debug, Clone, Copy)]
pub struct BedUse {
    pub player: Entity,
    pub position: IVec3,
}

/// Bed use and the sleep timer. It runs in a headless app with no screen
/// state, like [`super::SurvivalPlugin`].
pub struct SleepPlugin;

impl Plugin for SleepPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BedUse>().add_systems(
            Update,
            (use_bed, tick_sleep)
                .chain()
                .after(PhysicsSet::ApplyInput)
                .before(PhysicsSet::Integrate)
                .run_if(|screen: Option<Res<State<AppScreen>>>| {
                    screen.is_none_or(|screen| *screen.get() == AppScreen::Playing)
                }),
        );
    }
}

/// `World.isDaytime`.
fn is_daytime(dimension: Dimension, tick: &WorldTick, weather: Option<&WorldWeather>) -> bool {
    let (rain, thunder) = match weather {
        Some(weather) if dimension.has_weather() => {
            (weather.rain_strength, weather.weighted_thunder())
        }
        _ => (0.0, 0.0),
    };
    dimension.skylight_subtracted_in_weather(tick.world_time(), 1.0, rain, thunder)
        < DAYTIME_SKYLIGHT_SUBTRACTED
}

/// `EntityPlayer.func_25060_a`: where a player respawning at `bed` stands,
/// or `None` when the bed is gone or boxed in. The chunks around it must be
/// loaded; see [`bed_chunks`].
pub fn bed_respawn_feet(chunks: &WorldChunks, bed: IVec3) -> Option<Vec3> {
    if chunks.block_at(bed.x, bed.y, bed.z) != Some(Block::Bed) {
        return None;
    }
    bed::nearest_empty(chunks, bed, 0).map(|cell| {
        Vec3::new(
            cell.x as f32 + 0.5,
            cell.y as f32 + 0.1,
            cell.z as f32 + 0.5,
        )
    })
}

/// The chunks `Minecraft.respawn` prepares before it looks at a bed: those
/// within three blocks of it.
pub fn bed_chunks(bed: IVec3) -> impl Iterator<Item = ChunkPosition> {
    let min = ChunkPosition::from_block(bed.x - 3, bed.z - 3);
    let max = ChunkPosition::from_block(bed.x + 3, bed.z + 3);
    (min.x..=max.x).flat_map(move |x| (min.z..=max.z).map(move |z| ChunkPosition { x, z }))
}

#[allow(clippy::too_many_arguments)]
fn use_bed(
    mut uses: MessageReader<BedUse>,
    tick: Res<WorldTick>,
    dimension: Option<Res<ActiveDimension>>,
    weather: Option<Res<WorldWeather>>,
    mut chunks: ResMut<WorldChunks>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut chat: Option<ResMut<ChatHistory>>,
    mut explosions: Option<ResMut<Messages<Explosion>>>,
    mut player: Query<
        (
            &mut Transform,
            &mut Velocity,
            &mut PlayerSleep,
            &PlayerHealth,
            &mut PlayerInterpolation,
        ),
        With<Player>,
    >,
) {
    let dimension = dimension.map_or_else(Dimension::default, |dimension| dimension.0);
    for BedUse {
        player: user,
        position,
    } in uses.read().copied()
    {
        if chunks.block_at(position.x, position.y, position.z) != Some(Block::Bed) {
            continue;
        }
        let mut foot = position;
        let mut metadata = chunks.metadata_at(foot.x, foot.y, foot.z);
        if !bed::is_foot(metadata) {
            foot += bed::head_to_foot(metadata);
            if chunks.block_at(foot.x, foot.y, foot.z) != Some(Block::Bed) {
                continue;
            }
            metadata = chunks.metadata_at(foot.x, foot.y, foot.z);
        }

        if !dimension.can_respawn() {
            // Beta steps on from the foot half, past the bed, and blows up
            // there. The other half goes when it finds its partner missing.
            let mut remove = |cell: IVec3| {
                if chunks.block_at(cell.x, cell.y, cell.z) != Some(Block::Bed) {
                    return;
                }
                let previous_metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
                let Some(previous) = chunks.set_block(cell.x, cell.y, cell.z, Block::Air) else {
                    return;
                };
                if let Some(ticks) = block_ticks.as_deref_mut() {
                    ticks.block_changed(cell, previous, previous_metadata);
                }
                if let Some(streaming) = streaming.as_deref_mut() {
                    streaming.request_block_update(cell.x, cell.y, cell.z);
                }
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                }
            };
            remove(foot);
            let beyond = foot + bed::head_to_foot(metadata);
            remove(beyond);
            if let Some(explosions) = explosions.as_deref_mut() {
                explosions.write(Explosion {
                    center: beyond.as_vec3() + Vec3::splat(0.5),
                    strength: 5.0,
                    flaming: true,
                    source: Source::Environment,
                });
            }
            continue;
        }

        let Ok((mut transform, mut velocity, mut sleep, health, mut interpolation)) =
            player.get_mut(user)
        else {
            continue;
        };
        // `sleepInBedAt`. A bed still marked occupied has no sleeper to find
        // with one player in the world, so the mark is simply taken over.
        if sleep.sleeping || health.current == 0 {
            continue;
        }
        if is_daytime(dimension, &tick, weather.as_deref()) {
            if let Some(chat) = chat.as_deref_mut() {
                chat.push(NO_SLEEP_MESSAGE);
            }
            continue;
        }
        let offset = (transform.translation - foot.as_vec3()).abs();
        if offset.x > 3.0 || offset.y > 2.0 || offset.z > 3.0 {
            continue;
        }
        let (x, z) = match bed::direction(metadata) {
            0 => (0.5, 0.9),
            1 => (0.1, 0.5),
            2 => (0.5, 0.1),
            _ => (0.9, 0.5),
        };
        transform.translation = foot.as_vec3() + Vec3::new(x, LYING_HEIGHT, z);
        interpolation.previous_position = transform.translation;
        velocity.0 = Vec3::ZERO;
        sleep.sleeping = true;
        sleep.timer = 0;
        sleep.bed = Some(foot);
        sleep.last_health = health.current;
        set_occupied(&mut chunks, persistence.as_deref_mut(), foot, true);
    }
}

/// `BlockBed.setBedOccupied`.
fn set_occupied(
    chunks: &mut WorldChunks,
    persistence: Option<&mut WorldPersistence>,
    bed: IVec3,
    occupied: bool,
) {
    let metadata = chunks.metadata_at(bed.x, bed.y, bed.z);
    let marked = if occupied {
        metadata | bed::OCCUPIED
    } else {
        metadata & !bed::OCCUPIED
    };
    if marked != metadata && chunks.set_metadata(bed.x, bed.y, bed.z, marked) {
        if let Some(persistence) = persistence {
            persistence.mark_dirty(ChunkPosition::from_block(bed.x, bed.z));
        }
    }
}

/// `EntityPlayer.wakeUpPlayer(reset, _, set_spawn)`.
fn wake(
    sleep: &mut PlayerSleep,
    transform: &mut Transform,
    interpolation: &mut PlayerInterpolation,
    chunks: &mut WorldChunks,
    persistence: Option<&mut WorldPersistence>,
    chat: Option<&mut ChatHistory>,
    reset: bool,
    set_spawn: bool,
) {
    if let Some(bed) = sleep.bed
        && chunks.block_at(bed.x, bed.y, bed.z) == Some(Block::Bed)
    {
        set_occupied(chunks, persistence, bed, false);
        let cell = bed::nearest_empty(chunks, bed, 0).unwrap_or(bed + IVec3::Y);
        transform.translation = Vec3::new(
            cell.x as f32 + 0.5,
            cell.y as f32 + 0.1 + EntitySize::PLAYER.y_offset,
            cell.z as f32 + 0.5,
        );
        interpolation.previous_position = transform.translation;
    }
    sleep.sleeping = false;
    sleep.timer = if reset { 0 } else { FULLY_ASLEEP_TICKS };
    if set_spawn && sleep.bed.is_some() {
        sleep.spawn = sleep.bed;
        if let Some(chat) = chat {
            chat.push(SPAWN_SET_MESSAGE);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tick_sleep(
    mut tick: ResMut<WorldTick>,
    dimension: Option<Res<ActiveDimension>>,
    mut weather: Option<ResMut<WorldWeather>>,
    mut chunks: ResMut<WorldChunks>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut chat: Option<ResMut<ChatHistory>>,
    mut player: Query<
        (
            &mut Transform,
            &mut PlayerSleep,
            &PlayerHealth,
            &mut PlayerInterpolation,
        ),
        With<Player>,
    >,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0
        || player
            .iter()
            .all(|(_, sleep, ..)| !sleep.sleeping && sleep.timer == 0)
    {
        return;
    }
    let dimension = dimension.map_or_else(Dimension::default, |dimension| dimension.0);
    for _ in 0..ticks {
        // `World.tick`: with everyone fully asleep the night is skipped
        // (`isAllPlayersFullyAsleep`, then `wakeUpAllPlayers`).
        if player
            .iter()
            .all(|(_, sleep, ..)| sleep.sleeping && sleep.timer >= FULLY_ASLEEP_TICKS)
        {
            let previous = tick.world_time();
            let morning = previous + DAY_LENGTH;
            let morning = morning - morning % DAY_LENGTH;
            if let Some(ticks) = block_ticks.as_deref_mut() {
                ticks.rebase_time(previous, morning);
            }
            tick.set_world_time(morning);
            if let Some(weather) = weather.as_deref_mut() {
                weather.stop_precipitation();
            }
            for (mut transform, mut sleep, _, mut interpolation) in &mut player {
                wake(
                    &mut sleep,
                    &mut transform,
                    &mut interpolation,
                    &mut chunks,
                    persistence.as_deref_mut(),
                    chat.as_deref_mut(),
                    false,
                    true,
                );
            }
        }
        for (mut transform, mut sleep, health, mut interpolation) in &mut player {
            // `EntityPlayer.onUpdate`.
            if sleep.sleeping {
                sleep.timer = (sleep.timer + 1).min(FULLY_ASLEEP_TICKS);
                let in_bed = sleep
                    .bed
                    .is_some_and(|bed| chunks.block_at(bed.x, bed.y, bed.z) == Some(Block::Bed));
                // `attackEntityFrom` wakes a sleeper too.
                let hurt = health.current < sleep.last_health;
                sleep.last_health = health.current;
                if !in_bed || hurt {
                    wake(
                        &mut sleep,
                        &mut transform,
                        &mut interpolation,
                        &mut chunks,
                        persistence.as_deref_mut(),
                        None,
                        true,
                        false,
                    );
                } else if is_daytime(dimension, &tick, weather.as_deref()) {
                    wake(
                        &mut sleep,
                        &mut transform,
                        &mut interpolation,
                        &mut chunks,
                        persistence.as_deref_mut(),
                        chat.as_deref_mut(),
                        false,
                        true,
                    );
                }
            } else if sleep.timer > 0 {
                sleep.timer += 1;
                if sleep.timer >= FULLY_ASLEEP_TICKS + WAKE_FADE_TICKS {
                    sleep.timer = 0;
                }
            }
        }
    }
}

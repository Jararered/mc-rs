//! ECS wiring: run each world tick's block updates, then hand their changes
//! to streaming and persistence and their effects to entities.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::falling_block;
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::random::ItemRng;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::streaming::positions_in_radius;
use crate::world::tick::WorldTick;

use super::BlockTicks;
use super::RANDOM_TICK_RADIUS;
use super::TickEffect;

/// Block updates run after player input and physics have written this
/// frame's edits, and before streaming picks up the remeshes they request.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockTickSet;

pub struct BlockTicksPlugin;

impl Plugin for BlockTicksPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BlockTicks>()
            .init_resource::<LightCache>()
            .init_resource::<WorldTick>()
            .configure_sets(Update, BlockTickSet.after(PhysicsSet::Integrate))
            .add_systems(
                Update,
                (
                    run_block_ticks,
                    falling_block::tick_falling_blocks,
                    apply_tick_effects,
                )
                    .chain()
                    .in_set(BlockTickSet)
                    .run_if(block_ticks_should_run),
            );
    }
}

fn block_ticks_should_run(screen: Option<Res<State<AppScreen>>>) -> bool {
    screen.is_none_or(|screen| *screen.get() == AppScreen::Playing)
}

/// Chunks random ticks reach this frame: loaded and finished chunks within
/// [`RANDOM_TICK_RADIUS`] of the player, capped by the render distance.
/// With streaming, a chunk also waits for its first mesh job to light it, so
/// random ticks never relight chunks on the main thread.
fn random_tick_chunks(
    chunks: &WorldChunks,
    streaming: Option<&WorldStreaming>,
    light: &LightCache,
    center: ChunkPosition,
    radius: i32,
) -> Vec<ChunkPosition> {
    positions_in_radius(center, radius)
        .into_iter()
        .filter(|&position| {
            chunks.contains(position)
                && streaming.is_none_or(|streaming| {
                    light.contains(position) && streaming.neighborhood_finished(chunks, position)
                })
        })
        .collect()
}

pub(super) fn run_block_ticks(
    tick: Res<WorldTick>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    mut ticks: ResMut<BlockTicks>,
    mut chunks: ResMut<WorldChunks>,
    mut light: ResMut<LightCache>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    settings: Option<Res<GameSettings>>,
    player: Query<&Transform, With<Player>>,
) {
    let count = tick.ticks_this_frame();
    ticks.set_raining(weather.as_ref().is_some_and(|w| w.is_raining()));
    ticks.set_weather_penalty(weather.as_ref().map_or(0, |w| w.skylight_penalty()));
    let now = tick.world_time();
    ticks.process_events(&mut chunks, &mut light, now);

    if count > 0 {
        let random_chunks = player.single().map_or_else(
            |_| Vec::new(),
            |player| {
                let radius = settings.as_ref().map_or(RANDOM_TICK_RADIUS, |settings| {
                    settings.render_distance.min(RANDOM_TICK_RADIUS)
                });
                random_tick_chunks(
                    &chunks,
                    streaming.as_deref(),
                    &light,
                    ChunkPosition::from_world(player.translation.x, player.translation.z),
                    radius,
                )
            },
        );
        // `WorldTick` has already advanced past this frame's ticks.
        for step in 0..u64::from(count) {
            let time = now.wrapping_sub(u64::from(count) - 1 - step);
            ticks.tick(&mut chunks, &mut light, time, &random_chunks);
        }
    }

    let changes = ticks.take_changes();
    if changes.is_empty() {
        return;
    }
    if let Some(persistence) = persistence.as_deref_mut() {
        for change in &changes {
            persistence.mark_dirty(ChunkPosition::from_block(
                change.position.x,
                change.position.z,
            ));
        }
    }
    if let Some(streaming) = streaming.as_deref_mut() {
        streaming.request_block_changes(changes.iter().filter(|change| change.needs_remesh()).map(
            |change| {
                (
                    change.position.x,
                    change.position.y,
                    change.position.z,
                    change.changes_light(),
                )
            },
        ));
    }
}

fn apply_tick_effects(
    mut commands: Commands,
    mut ticks: ResMut<BlockTicks>,
    mut rng: Local<ItemRng>,
) {
    for effect in ticks.take_effects() {
        match effect {
            TickEffect::Drop {
                position,
                block,
                metadata,
            } => {
                for stack in natural_drops_with_metadata(block, metadata, &mut *rng) {
                    spawn_block_drop(&mut commands, &mut rng, position, stack);
                }
            }
            TickEffect::FallingBlock { position, block } => {
                falling_block::spawn_falling_block(&mut commands, position, block);
            }
            TickEffect::PrimedTnt { position, fuse } => {
                crate::entity::explosion::prime_tnt(
                    &mut commands,
                    position.as_vec3() + bevy::math::Vec3::new(0.5, 0.0, 0.5),
                    fuse,
                );
            }
        }
    }
}

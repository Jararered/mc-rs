//! ECS wiring: run each world tick's block updates, then hand their changes
//! to streaming and persistence and their effects to entities.

use bevy::audio::AudioPlayer;
use bevy::audio::AudioSource;
use bevy::audio::PlaybackSettings;
use bevy::prelude::*;
use std::collections::HashMap;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::drops::items::spawn_dispensed_item;
use crate::entity::falling_block;
use crate::entity::minecart;
use crate::entity::minecart::Minecart;
use crate::entity::projectile;
use crate::entity::projectile::Projectile;
use crate::entity::tnt;
use crate::entity::tnt::PrimedTnt;
use crate::item::ItemId;
use crate::physics::Aabb;
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
use super::RedstoneOccupant;
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
                    minecart::tick_minecarts,
                    run_block_ticks,
                    falling_block::tick_falling_blocks,
                    tnt::tick_primed_tnt,
                    projectile::tick_projectiles,
                    apply_tick_effects,
                    falling_block::sync_falling_block_rendering,
                    tnt::sync_tnt_rendering,
                    minecart::sync_minecart_rendering,
                    projectile::sync_projectile_rendering,
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
    mut ticks: ResMut<BlockTicks>,
    mut chunks: ResMut<WorldChunks>,
    mut light: ResMut<LightCache>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    settings: Option<Res<GameSettings>>,
    player: Query<(&Transform, &EntitySize), With<Player>>,
    dropped: Query<(&Transform, &EntitySize), With<DroppedItem>>,
    carts: Query<(&Transform, &EntitySize), With<Minecart>>,
    primed: Query<(&Transform, &EntitySize), With<PrimedTnt>>,
    projectiles: Query<(&Transform, &EntitySize), With<Projectile>>,
) {
    let count = tick.ticks_this_frame();
    let now = tick.world_time();
    let occupant = |transform: &Transform, size: &EntitySize, living| {
        let box_ = size.aabb(transform.translation);
        RedstoneOccupant {
            min: box_.min.to_array(),
            max: box_.max.to_array(),
            living,
            minecart: false,
        }
    };
    let mut occupants: Vec<_> = player.iter().map(|(t, s)| occupant(t, s, true)).collect();
    occupants.extend(dropped.iter().map(|(t, s)| occupant(t, s, false)));
    occupants.extend(carts.iter().map(|(t, s)| {
        let mut body = occupant(t, s, false);
        body.minecart = true;
        body
    }));
    occupants.extend(primed.iter().map(|(t, s)| occupant(t, s, false)));
    occupants.extend(projectiles.iter().map(|(t, s)| occupant(t, s, false)));
    ticks.set_occupants(occupants);
    ticks.process_events(&mut chunks, &mut light, now);

    if count > 0 {
        let random_chunks = player.single().map_or_else(
            |_| Vec::new(),
            |(player, _)| {
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
    mut sounds: Option<ResMut<Assets<AudioSource>>>,
    mut note_cache: Local<HashMap<(u8, u8), Handle<AudioSource>>>,
    mut bodies: Query<(&mut Transform, &EntitySize)>,
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
                tnt::spawn_primed_tnt(&mut commands, position, fuse);
            }
            TickEffect::Dispense {
                position,
                facing,
                stack,
            } => {
                if matches!(stack.item(), ItemId::Arrow | ItemId::Egg | ItemId::Snowball) {
                    projectile::spawn_projectile(&mut commands, position, facing, stack.item());
                } else {
                    spawn_dispensed_item(&mut commands, &mut rng, position, facing, stack);
                }
            }
            TickEffect::DropStack { position, stack } => {
                spawn_chest_drops(&mut commands, &mut rng, position, [stack]);
            }
            TickEffect::PistonPush {
                position,
                direction,
            } => {
                let block = Aabb::from_block(position.x, position.y, position.z);
                for (mut transform, size) in &mut bodies {
                    let bounds = size.aabb(transform.translation);
                    if !bounds.intersects(block) {
                        continue;
                    }
                    let distance = if direction.x > 0 {
                        block.max.x - bounds.min.x
                    } else if direction.x < 0 {
                        bounds.max.x - block.min.x
                    } else if direction.y > 0 {
                        block.max.y - bounds.min.y
                    } else if direction.y < 0 {
                        bounds.max.y - block.min.y
                    } else if direction.z > 0 {
                        block.max.z - bounds.min.z
                    } else {
                        bounds.max.z - block.min.z
                    };
                    transform.translation += direction.as_vec3() * (distance + 0.001);
                }
            }
            TickEffect::Note {
                position: _,
                instrument,
                pitch,
            } => {
                if let Some(sounds) = sounds.as_deref_mut() {
                    let sound = note_cache.entry((instrument, pitch)).or_insert_with(|| {
                        sounds.add(AudioSource {
                            bytes: synth_note(instrument, pitch).into(),
                        })
                    });
                    commands.spawn((AudioPlayer::new(sound.clone()), PlaybackSettings::DESPAWN));
                }
            }
        }
    }
}

/// In-memory PCM WAV, avoiding non-distributable reference sound assets.
fn synth_note(instrument: u8, pitch: u8) -> Vec<u8> {
    const RATE: u32 = 22_050;
    const SAMPLES: usize = 8_820;
    let mut data = Vec::with_capacity(44 + SAMPLES * 2);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36 + (SAMPLES * 2) as u32).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16_u32.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    data.extend_from_slice(&1_u16.to_le_bytes()); // mono
    data.extend_from_slice(&RATE.to_le_bytes());
    data.extend_from_slice(&(RATE * 2).to_le_bytes());
    data.extend_from_slice(&2_u16.to_le_bytes());
    data.extend_from_slice(&16_u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&((SAMPLES * 2) as u32).to_le_bytes());
    let frequency = 440.0 * 2.0_f32.powf((f32::from(pitch) - 12.0) / 12.0);
    for sample in 0..SAMPLES {
        let t = sample as f32 / RATE as f32;
        let phase = t * frequency * std::f32::consts::TAU;
        let carrier = match instrument {
            1 => (phase.sin() + (phase * 0.5).sin() * 0.4) * 0.7,
            2 => (phase * 3.75).sin() * 0.7 + (phase * 1.33).cos() * 0.3,
            3 => phase.sin() * 0.7 + (phase * 2.0).sin() * 0.3,
            4 => (phase.sin() + (phase * 2.0).sin() * 0.25) * 0.7,
            _ => phase.sin(),
        };
        let envelope =
            (1.0 - sample as f32 / SAMPLES as f32).powi(3) * (sample as f32 / 180.0).min(1.0);
        let value = (carrier * envelope * 9_000.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    data
}

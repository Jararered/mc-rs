//! ECS wiring: run each world tick's block updates, then hand their changes
//! to streaming and persistence and their effects to entities.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::boat;
use crate::entity::boat::Boat;
use crate::entity::creature::Living;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::items::dispenser_direction;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::drops::items::spawn_dispensed_item;
use crate::entity::explosion::PrimedTnt;
use crate::entity::falling_block;
use crate::entity::minecart;
use crate::entity::minecart::Minecart;
use crate::entity::mount;
use crate::entity::projectiles::ArrowDamage;
use crate::entity::projectiles::Projectile;
use crate::entity::projectiles::spawn_arrow_with;
use crate::entity::thrown::ThrownKind;
use crate::entity::thrown::spawn_thrown;
use crate::item::Item;
use crate::physics::Aabb;
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::random::ItemRng;
use crate::random::JavaRandom;
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
                    minecart::bump_carts,
                    boat::tick_boats,
                    boat::bump_boats,
                    run_block_ticks,
                    falling_block::tick_falling_blocks,
                    apply_tick_effects,
                )
                    .chain()
                    .in_set(BlockTickSet)
                    .run_if(crate::world::tick::playing),
            )
            .add_systems(
                Update,
                (mount::snap_riders, mount::release_orphans)
                    .chain()
                    .after(BlockTickSet)
                    .after(minecart::tick_minecarts)
                    .after(boat::tick_boats)
                    .after(crate::entity::creature::tick_creatures)
                    .run_if(crate::world::tick::playing),
            );
    }
}

/// Chunks random ticks reach this frame: loaded and finished chunks within
/// [`RANDOM_TICK_RADIUS`] of the player, capped by the render distance.
/// With streaming, a chunk also waits for its first mesh job to light it. A
/// tick that reads light across the border of an unlit neighbor gets an
/// estimate, so nothing is relit on the main thread.
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
    environment: crate::world::dimension::Environment,
    mut ticks: ResMut<BlockTicks>,
    mut chunks: ResMut<WorldChunks>,
    mut light: ResMut<LightCache>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    settings: Option<Res<GameSettings>>,
    player: Query<(&Transform, &EntitySize), With<Player>>,
    dropped: Query<(&Transform, &EntitySize), With<DroppedItem>>,
    creatures: Query<(&Transform, &EntitySize), With<Living>>,
    primed: Query<(&Transform, &EntitySize), With<PrimedTnt>>,
    carts: Query<(&Transform, &EntitySize), With<Minecart>>,
    boats: Query<(&Transform, &EntitySize), With<Boat>>,
    projectiles: Query<(&Transform, &EntitySize), With<Projectile>>,
) {
    let count = tick.ticks_this_frame();
    ticks.set_dimension(environment.dimension());
    ticks.set_raining(environment.is_raining());
    let (rain, thunder) = environment.weather_strength();
    ticks.set_weather_strength(rain, thunder);
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
    occupants.extend(creatures.iter().map(|(t, s)| occupant(t, s, true)));
    occupants.extend(primed.iter().map(|(t, s)| occupant(t, s, false)));
    occupants.extend(boats.iter().map(|(t, s)| occupant(t, s, false)));
    // `EnumMobType.everything`: an arrow lying on a wooden plate holds it down.
    occupants.extend(projectiles.iter().map(|(t, s)| occupant(t, s, false)));
    occupants.extend(carts.iter().map(|(t, s)| {
        let mut body = occupant(t, s, false);
        body.minecart = true;
        body
    }));
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
                crate::entity::explosion::prime_tnt(
                    &mut commands,
                    position.as_vec3() + bevy::math::Vec3::new(0.5, 0.0, 0.5),
                    fuse,
                );
            }
            TickEffect::Dispense {
                position,
                facing,
                stack,
            } => {
                // `BlockDispenser.dispenseItem`: arrows, eggs and snowballs
                // leave along (dx, 0.1, dz) at 1.1 blocks per tick with a
                // spread of 6. A dispensed arrow can be picked up.
                let direction = dispenser_direction(facing);
                let mouth = position.as_vec3() + Vec3::new(0.5, 0.5, 0.5) + direction * 0.6;
                let heading = Vec3::new(direction.x, 0.1, direction.z);
                if stack.item() == Item::Arrow {
                    spawn_arrow_with(
                        &mut commands,
                        mouth,
                        heading,
                        1.1,
                        6.0,
                        None,
                        true,
                        ArrowDamage::Flat(4),
                        &mut JavaRandom::new(rng.next_u64()),
                    );
                } else if let Some(kind) = ThrownKind::from_item(stack.item()) {
                    spawn_thrown(
                        &mut commands,
                        kind,
                        mouth,
                        heading,
                        1.1,
                        6.0,
                        None,
                        &mut JavaRandom::new(rng.next_u64()),
                    );
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
            // Note blocks have no sound to play until audio is implemented.
            TickEffect::Note { .. } => {}
        }
    }
}

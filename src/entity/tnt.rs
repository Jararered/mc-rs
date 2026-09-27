//! Beta primed TNT: 80-tick fuse, gravity, and resistance-aware explosion rays.
use std::collections::HashSet;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::definition;
use crate::block::id::Id;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::drops::blocks::DropRoll;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::physics::Aabb;
use crate::physics::move_entity;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::meshing::dropped_block_meshes;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::textures::TerrainMaterial;
use crate::world::tick::WorldTick;

#[derive(Component, Clone, Copy, Debug)]
pub struct PrimedTnt {
    pub fuse: u8,
    pub motion: Vec3,
    pub on_ground: bool,
}

#[derive(Component)]
pub(crate) struct TntVisual;

pub fn spawn_primed_tnt(commands: &mut Commands, position: IVec3, fuse: u8) {
    let center = position.as_vec3() + Vec3::splat(0.5);
    commands.spawn((
        Name::new("Primed TNT"),
        PrimedTnt {
            fuse,
            motion: Vec3::new(0.0, 0.2, 0.0),
            on_ground: false,
        },
        EntitySize {
            width: 0.98,
            height: 0.98,
            y_offset: 0.49,
        },
        PreviousTick(center),
        Transform::from_translation(center),
        Visibility::default(),
    ));
}

/// Beta `Explosion.doExplosionA/B`: trace 16-cube shell rays, attenuating
/// through each block's explosion resistance. Returns blocks actually removed.
pub fn explode_tnt(
    chunks: &mut WorldChunks,
    ticks: &mut BlockTicks,
    center: Vec3,
    seed: u64,
) -> Vec<(IVec3, Id, u8)> {
    let mut rng = JavaRandom::new(seed);
    let mut hit = HashSet::new();
    for x in 0..16 {
        for y in 0..16 {
            for z in 0..16 {
                if !(x == 0 || y == 0 || z == 0 || x == 15 || y == 15 || z == 15) {
                    continue;
                }
                let dir = Vec3::new(
                    x as f32 / 7.5 - 1.0,
                    y as f32 / 7.5 - 1.0,
                    z as f32 / 7.5 - 1.0,
                )
                .normalize();
                let mut strength = 4.0 * (0.7 + rng.next_float() * 0.6);
                let mut point = center;
                while strength > 0.0 {
                    let cell = point.floor().as_ivec3();
                    let Some(block) = chunks.block_at(cell.x, cell.y, cell.z) else {
                        break;
                    };
                    if block != Id::Air {
                        // Block.resistance = hardness * 5 for ordinary blocks.
                        let resistance = if matches!(block, Id::Bedrock | Id::Obsidian) {
                            6000.0
                        } else if matches!(
                            block,
                            Id::Water | Id::FlowingWater | Id::Lava | Id::FlowingLava
                        ) {
                            500.0
                        } else {
                            definition::properties(block).hardness.max(0.0) * 5.0
                        };
                        strength -= (resistance + 0.3) * 0.3;
                    }
                    if strength > 0.0 && block != Id::Air {
                        hit.insert(cell);
                    }
                    point += dir * 0.3;
                    strength -= 0.225;
                }
            }
        }
    }
    let mut hit: Vec<_> = hit.into_iter().collect();
    hit.sort_by_key(|cell| (cell.x, cell.y, cell.z));
    let mut removed = Vec::with_capacity(hit.len());
    for cell in hit {
        let Some(block) = chunks.block_at(cell.x, cell.y, cell.z) else {
            continue;
        };
        if block == Id::Air || block == Id::Bedrock {
            continue;
        }
        let meta = chunks.metadata_at(cell.x, cell.y, cell.z);
        if block == Id::Dispenser {
            if let Some(dispenser) = chunks.dispenser_at(cell.x, cell.y, cell.z) {
                let contents: Vec<_> = dispenser.slots.into_iter().flatten().collect();
                for stack in contents {
                    ticks.drop_stack(cell, stack);
                }
            }
        }
        if chunks.set_block(cell.x, cell.y, cell.z, Id::Air).is_some() {
            ticks.block_changed(cell, block, meta);
            if block == Id::Tnt {
                ticks.prime_tnt(cell, 10 + rng.next_int(21) as u8);
            }
            removed.push((cell, block, meta));
        }
    }
    removed
}

pub(crate) fn tick_primed_tnt(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: ResMut<BlockTicks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut primed: Query<(Entity, &mut PrimedTnt, &mut Transform, &mut PreviousTick)>,
    mut drops: Local<ItemRng>,
    mut player: Query<
        (&Transform, &mut PlayerHealth, &mut Velocity),
        (With<Player>, Without<PrimedTnt>),
    >,
) {
    for (entity, mut tnt, mut transform, mut previous) in &mut primed {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }
        for _ in 0..tick.ticks_this_frame() {
            previous.0 = transform.translation;
            tnt.motion.y -= 0.04;
            let bounds = Aabb::new(
                transform.translation - Vec3::splat(0.49),
                transform.translation + Vec3::splat(0.49),
            );
            let moved = move_entity(bounds, tnt.motion, 0.0, tnt.on_ground, &chunks);
            transform.translation = (moved.aabb.min + moved.aabb.max) * 0.5;
            tnt.on_ground = moved.collision.on_ground;
            if moved.collision.collided_x {
                tnt.motion.x = 0.0;
            }
            if moved.collision.collided_y {
                tnt.motion.y = 0.0;
            }
            if moved.collision.collided_z {
                tnt.motion.z = 0.0;
            }
            tnt.motion *= 0.98;
            if tnt.on_ground {
                tnt.motion.x *= 0.7;
                tnt.motion.z *= 0.7;
                tnt.motion.y *= -0.5;
            }
            if tnt.fuse == 0 {
                for (body, mut health, mut velocity) in &mut player {
                    let delta = body.translation - transform.translation;
                    let distance = delta.length();
                    if distance < 8.0 {
                        let impact = (1.0 - distance / 8.0).max(0.0);
                        health.current = health
                            .current
                            .saturating_sub(((impact * impact + impact) * 16.0 + 1.0) as u8);
                        velocity.0 += delta.normalize_or_zero() * impact * 20.0;
                    }
                }
                let seed = tick.world_time()
                    ^ u64::from(transform.translation.x.to_bits()) << 32
                    ^ u64::from(transform.translation.z.to_bits());
                for (cell, block, meta) in
                    explode_tnt(&mut chunks, &mut ticks, transform.translation, seed)
                {
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                    }
                    if let Some(streaming) = streaming.as_deref_mut() {
                        streaming.request_block_update(cell.x, cell.y, cell.z);
                    }
                    if drops.next_int(10) < 3 {
                        for stack in natural_drops_with_metadata(block, meta, &mut *drops) {
                            spawn_block_drop(&mut commands, &mut drops, cell, stack);
                        }
                    }
                }
                commands.entity(entity).despawn();
                break;
            }
            tnt.fuse -= 1;
        }
    }
}

pub(crate) fn sync_tnt_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    terrain: Option<Res<TerrainMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    new_tnt: Query<Entity, (With<PrimedTnt>, Without<TntVisual>)>,
    visuals: Query<(&Transform, &PreviousTick, &Children), (With<PrimedTnt>, With<TntVisual>)>,
    mut pieces: Query<&mut Transform, Without<PrimedTnt>>,
) {
    let Some(terrain) = terrain else { return };
    for entity in &new_tnt {
        let built = dropped_block_meshes(
            Id::Tnt,
            settings.as_ref().is_some_and(|s| s.graphics.fancy_leaves()),
            [0.55, 0.8, 0.4],
            [0.28, 0.71, 0.09],
        );
        let child = commands
            .spawn((
                Mesh3d(meshes.add(built.body.into_mesh())),
                MeshMaterial3d(terrain.0.clone()),
                Transform::default(),
                NoFrustumCulling,
            ))
            .id();
        commands.entity(entity).add_child(child).insert(TntVisual);
    }
    for (transform, previous, children) in &visuals {
        let slide = previous.0.lerp(transform.translation, tick.partial()) - transform.translation;
        for child in children.iter() {
            if let Ok(mut piece) = pieces.get_mut(child) {
                piece.translation = slide;
            }
        }
    }
}

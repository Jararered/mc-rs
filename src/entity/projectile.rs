//! Dispenser-fired arrows, eggs and snowballs with Beta's ballistic rates.
use bevy::prelude::*;

use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::drops::items::spawn_block_drop;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::physics::raycast_blocks;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;

#[derive(Component, Clone, Copy, Debug)]
pub struct Projectile {
    pub item: ItemId,
    pub motion: Vec3,
    pub age: u16,
}

#[derive(Component)]
pub(crate) struct ProjectileVisual;

pub fn spawn_projectile(commands: &mut Commands, position: IVec3, facing: u8, item: ItemId) {
    let direction = match facing {
        2 => Vec3::NEG_Z,
        3 => Vec3::Z,
        4 => Vec3::NEG_X,
        _ => Vec3::X,
    };
    let origin = position.as_vec3() + Vec3::splat(0.5) + direction * 0.6;
    commands.spawn((
        Name::new("Dispenser projectile"),
        Projectile {
            item,
            motion: direction * 1.1 + Vec3::Y * 0.1,
            age: 0,
        },
        EntitySize {
            width: 0.25,
            height: 0.25,
            y_offset: 0.125,
        },
        PreviousTick(origin),
        Transform::from_translation(origin),
        Visibility::default(),
    ));
}

/// Move a projectile through blocks for one world tick. Returns the impacted cell.
pub fn step_projectile(
    projectile: &mut Projectile,
    position: &mut Vec3,
    chunks: &WorldChunks,
) -> Option<IVec3> {
    projectile.age += 1;
    let length = projectile.motion.length();
    let hit = (length > 0.0)
        .then(|| raycast_blocks(chunks, *position, projectile.motion / length, length))
        .flatten();
    if let Some(hit) = hit {
        return Some(IVec3::new(hit.x, hit.y, hit.z));
    }
    *position += projectile.motion;
    projectile.motion *= 0.99;
    projectile.motion.y -= if projectile.item == ItemId::Arrow {
        0.05
    } else {
        0.03
    };
    None
}

pub(crate) fn tick_projectiles(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform, &mut PreviousTick)>,
    mut players: Query<
        (&Transform, &crate::entity::EntitySize, &mut PlayerHealth),
        (With<Player>, Without<Projectile>),
    >,
    mut rng: Local<ItemRng>,
) {
    for (entity, mut projectile, mut transform, mut previous) in &mut projectiles {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }
        for _ in 0..tick.ticks_this_frame() {
            previous.0 = transform.translation;
            let mut center = transform.translation;
            let hit = step_projectile(&mut projectile, &mut center, &chunks);
            transform.translation = center;
            let mut collided = false;
            for (body, size, mut health) in &mut players {
                let bounds = size.aabb(body.translation);
                if center.x >= bounds.min.x
                    && center.x <= bounds.max.x
                    && center.y >= bounds.min.y
                    && center.y <= bounds.max.y
                    && center.z >= bounds.min.z
                    && center.z <= bounds.max.z
                {
                    if projectile.item == ItemId::Arrow {
                        health.current = health.current.saturating_sub(4);
                    }
                    collided = true;
                }
            }
            if collided || hit.is_some() || projectile.age > 100 {
                if projectile.item == ItemId::Arrow
                    && !collided
                    && let Some(cell) = hit
                    && let Ok(stack) = ItemStack::new(ItemId::Arrow, 1)
                {
                    spawn_block_drop(&mut commands, &mut rng, cell, stack);
                }
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}

pub(crate) fn sync_projectile_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    new_projectiles: Query<(Entity, &Projectile), Without<ProjectileVisual>>,
    visuals: Query<
        (&Transform, &PreviousTick, &Children),
        (With<Projectile>, With<ProjectileVisual>),
    >,
    mut pieces: Query<&mut Transform, Without<Projectile>>,
) {
    if let Some(materials) = materials.as_deref_mut() {
        for (entity, projectile) in &new_projectiles {
            let color = match projectile.item {
                ItemId::Arrow => Color::srgb(0.7, 0.65, 0.52),
                ItemId::Egg => Color::srgb(0.9, 0.86, 0.72),
                _ => Color::WHITE,
            };
            let mesh = meshes.add(Cuboid::from_size(if projectile.item == ItemId::Arrow {
                Vec3::new(0.06, 0.06, 0.35)
            } else {
                Vec3::splat(0.22)
            }));
            let child = commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: color,
                        ..default()
                    })),
                    Transform::default(),
                ))
                .id();
            commands
                .entity(entity)
                .add_child(child)
                .insert(ProjectileVisual);
        }
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

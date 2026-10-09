//! `RenderBoat` and `ModelBoat`.
//!
//! The boat is the five boxes of `item/boat.png`, turned to its yaw and
//! rocking after a hit.

use bevy::mesh::MeshTag;
use bevy::prelude::*;

use super::CreatureAssets;
use super::CreatureMaterial;
use super::creature_tag;
use super::entity_brightness;
use super::models::Cuboid;
use super::models::cuboid_mesh;
use crate::entity::PreviousTick;
use crate::entity::boat::BOAT_SIZE;
use crate::entity::boat::Boat;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;

/// The frame `RenderBoat` poses: the model hangs below it, upside down.
#[derive(Component)]
pub(super) struct BoatModel {
    /// The box meshes' parent, turned to hang the model the right way up.
    boxes: Entity,
}

/// One box of the model.
#[derive(Component)]
pub(super) struct BoatBox;

/// One box of `ModelBoat`: its skin offset, origin, extent, rotation point
/// and rotation, in model pixels and radians.
pub type BoatPart = ([u8; 2], [f32; 3], [u8; 3], [f32; 3], [f32; 3]);

/// `ModelBoat`: the floor, then the four sides.
pub const BOXES: [BoatPart; 5] = [
    (
        [0, 8],
        [-12.0, -8.0, -3.0],
        [24, 16, 4],
        [0.0, 4.0, 0.0],
        [1.570_796_4, 0.0, 0.0],
    ),
    (
        [0, 0],
        [-10.0, -7.0, -1.0],
        [20, 6, 2],
        [-11.0, 4.0, 0.0],
        [0.0, 4.712_389, 0.0],
    ),
    (
        [0, 0],
        [-10.0, -7.0, -1.0],
        [20, 6, 2],
        [11.0, 4.0, 0.0],
        [0.0, 1.570_796_4, 0.0],
    ),
    (
        [0, 0],
        [-10.0, -7.0, -1.0],
        [20, 6, 2],
        [0.0, 4.0, -9.0],
        [0.0, std::f32::consts::PI, 0.0],
    ),
    (
        [0, 0],
        [-10.0, -7.0, -1.0],
        [20, 6, 2],
        [0.0, 4.0, 9.0],
        [0.0, 0.0, 0.0],
    ),
];

/// `ModelRenderer.render`: rotate about Z, then Y, then X.
fn zyx(rotation: [f32; 3]) -> Quat {
    Quat::from_rotation_z(rotation[2])
        * Quat::from_rotation_y(rotation[1])
        * Quat::from_rotation_x(rotation[0])
}

pub(super) fn add_boat_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut box_meshes: Local<Vec<Handle<Mesh>>>,
    boats: Query<Entity, Added<Boat>>,
) {
    if boats.is_empty() {
        return;
    }
    let Some(skin) = assets.skins.get("item/boat.png") else {
        return;
    };
    if box_meshes.is_empty() {
        box_meshes.extend(BOXES.iter().map(|(texture, origin, size, _, _)| {
            meshes.add(cuboid_mesh(&Cuboid {
                texture: *texture,
                origin: Vec3::from_array(*origin),
                size: *size,
                inflate: 0.0,
                mirror: false,
            }))
        }));
    }
    for entity in &boats {
        // `glScalef(-1, -1, 1)` and the 1/16 of the model's pixels.
        let boxes = commands
            .spawn((
                Transform {
                    rotation: Quat::from_rotation_z(std::f32::consts::PI),
                    scale: Vec3::splat(1.0 / 16.0),
                    ..default()
                },
                Visibility::Inherited,
            ))
            .id();
        for ((_, _, _, pivot, rotation), mesh) in BOXES.iter().zip(box_meshes.iter()) {
            commands.spawn((
                BoatBox,
                Mesh3d(mesh.clone()),
                MeshMaterial3d::<CreatureMaterial>(skin.clone()),
                MeshTag::default(),
                Transform {
                    translation: Vec3::from_array(*pivot),
                    rotation: zyx(*rotation),
                    ..default()
                },
                Visibility::Inherited,
                ChildOf(boxes),
            ));
        }
        let root = commands
            .spawn((
                Name::new("Boat model"),
                BoatModel { boxes },
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
            ))
            .id();
        commands.entity(boxes).insert(ChildOf(root));
    }
}

/// `RenderBoat.doRender`'s turn for a boat: about Y by its yaw, then the
/// rock a hit leaves, about X. Angles are in degrees.
pub fn boat_rotation(yaw: f32, rock: f32) -> Quat {
    Quat::from_rotation_y((180.0 - yaw).to_radians()) * Quat::from_rotation_x(rock.to_radians())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_boats(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    environment: crate::world::dimension::Environment,
    boats: Query<(&Boat, &Transform, &PreviousTick, &Children), Without<BoatModel>>,
    mut models: Query<(&BoatModel, &mut Transform), Without<Boat>>,
    mut tags: Query<&mut MeshTag, With<BoatBox>>,
    children: Query<&Children>,
) {
    let partial = tick.partial();
    let subtracted = environment.skylight_subtracted(partial);
    let ambient = environment.ambient_light();
    for (boat, body, previous, kids) in &boats {
        let position = previous.0.lerp(body.translation, partial);
        let mut turn = boat.yaw - boat.prev_yaw;
        while turn >= 180.0 {
            turn -= 360.0;
        }
        while turn < -180.0 {
            turn += 360.0;
        }
        let yaw = boat.prev_yaw + turn * partial;
        let hit = boat.time_since_hit as f32 - partial;
        let damage = (boat.damage as f32 - partial).max(0.0);
        let rock = if hit > 0.0 {
            hit.sin() * hit * damage / 10.0 * boat.rock_direction as f32
        } else {
            0.0
        };
        let brightness = entity_brightness(
            &chunks,
            light.as_deref(),
            body.translation - Vec3::Y * BOAT_SIZE.y_offset,
            BOAT_SIZE.height,
            subtracted,
            ambient,
        );
        for kid in kids.iter() {
            let Ok((model, mut pose)) = models.get_mut(kid) else {
                continue;
            };
            pose.set_if_neq(Transform {
                translation: position - body.translation,
                rotation: boat_rotation(yaw, rock),
                scale: Vec3::ONE,
            });
            if let Ok(boxes) = children.get(model.boxes) {
                for part in boxes.iter() {
                    if let Ok(mut tag) = tags.get_mut(part) {
                        tag.set_if_neq(creature_tag(brightness, false, 0.0, 1.0));
                    }
                }
            }
        }
    }
}

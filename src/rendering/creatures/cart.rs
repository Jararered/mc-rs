//! `RenderMinecart` and `ModelMinecart`.
//!
//! The cart is the six boxes of `item/cart.png`, set on the rail by
//! [`get_pos`] and [`get_pos_offset`] so it follows slopes and bends, rocking
//! after a hit. A chest or furnace cart carries that block at three quarters
//! of its size.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use super::CreatureAssets;
use super::CreatureMaterial;
use super::creature_tag;
use super::entity_brightness;
use super::models::Cuboid;
use super::models::cuboid_mesh;
use crate::app::settings::GameSettings;
use crate::entity::PreviousTick;
use crate::entity::minecart::Minecart;
use crate::entity::minecart::get_pos;
use crate::entity::minecart::get_pos_offset;
use crate::rendering::meshing::dropped_block_meshes;
use crate::rendering::textures::TerrainMaterial;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;

/// The frame `RenderMinecart` poses: the model hangs below it, upside down.
#[derive(Component)]
pub(super) struct CartModel {
    /// The box meshes' parent, turned to hang the model the right way up.
    boxes: Entity,
}

/// One box of the model.
#[derive(Component)]
pub(super) struct CartBox;

/// The chest or furnace a cart carries.
#[derive(Component)]
struct CartCargo;

/// How far along the track the facing and slope are sampled.
const SAMPLE: f32 = 0.3;

/// `ModelMinecart`: each box's skin offset, extent, rotation point and
/// rotation, in model pixels and radians.
const BOXES: [([u8; 2], [f32; 3], [u8; 3], [f32; 3], [f32; 3]); 6] = [
    (
        [0, 10],
        [-10.0, -8.0, -1.0],
        [20, 16, 2],
        [0.0, 4.0, 0.0],
        [1.570_796_4, 0.0, 0.0],
    ),
    (
        [0, 0],
        [-8.0, -9.0, -1.0],
        [16, 8, 2],
        [-9.0, 4.0, 0.0],
        [0.0, 4.712_389, 0.0],
    ),
    (
        [0, 0],
        [-8.0, -9.0, -1.0],
        [16, 8, 2],
        [9.0, 4.0, 0.0],
        [0.0, 1.570_796_4, 0.0],
    ),
    (
        [0, 0],
        [-8.0, -9.0, -1.0],
        [16, 8, 2],
        [0.0, 4.0, -7.0],
        [0.0, std::f32::consts::PI, 0.0],
    ),
    (
        [0, 0],
        [-8.0, -9.0, -1.0],
        [16, 8, 2],
        [0.0, 4.0, 7.0],
        [0.0, 0.0, 0.0],
    ),
    // The inner floor, which `render` lowers by 0.1 pixel.
    (
        [44, 10],
        [-9.0, -7.0, -1.0],
        [18, 14, 1],
        [0.0, 4.1, 0.0],
        [-1.570_796_4, 0.0, 0.0],
    ),
];

/// `ModelRenderer.render`: rotate about Z, then Y, then X.
fn zyx(rotation: [f32; 3]) -> Quat {
    Quat::from_rotation_z(rotation[2])
        * Quat::from_rotation_y(rotation[1])
        * Quat::from_rotation_x(rotation[0])
}

pub(super) fn add_cart_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    settings: Option<Res<GameSettings>>,
    terrain: Option<Res<TerrainMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut box_meshes: Local<Vec<Handle<Mesh>>>,
    carts: Query<(Entity, &Minecart), Added<Minecart>>,
) {
    if carts.is_empty() {
        return;
    }
    let Some(skin) = assets.skins.get("item/cart.png") else {
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
    let fancy = settings.is_some_and(|settings| settings.graphics.fancy_leaves());
    for (entity, cart) in &carts {
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
                CartBox,
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
                Name::new("Minecart model"),
                CartModel { boxes },
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
            ))
            .id();
        commands.entity(boxes).insert(ChildOf(root));
        if let (Some(block), Some(terrain)) = (cart.kind.block(), terrain.as_deref()) {
            let built = dropped_block_meshes(block, 0, fancy, [0.55, 0.8, 0.4], [0.28, 0.71, 0.09]);
            commands.spawn((
                CartCargo,
                Mesh3d(meshes.add(built.body.into_mesh())),
                MeshMaterial3d(terrain.0.clone()),
                // Three quarters scale, lifted 0.3125 in that scaled frame,
                // and turned a quarter.
                Transform {
                    translation: Vec3::new(0.0, 0.75 * 0.3125, 0.0),
                    rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    scale: Vec3::splat(0.75),
                },
                Visibility::Inherited,
                NoFrustumCulling,
                ChildOf(root),
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_carts(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    environment: crate::world::dimension::Environment,
    carts: Query<(&Minecart, &Transform, &PreviousTick, &Children), Without<CartModel>>,
    mut models: Query<(&CartModel, &mut Transform), Without<Minecart>>,
    mut tags: Query<&mut MeshTag, With<CartBox>>,
    children: Query<&Children>,
) {
    let partial = tick.partial();
    let subtracted = environment.skylight_subtracted(partial);
    let ambient = environment.ambient_light();
    for (cart, body, previous, kids) in &carts {
        let slid = previous.0.lerp(body.translation, partial);
        // The rail decides where the cart sits and which way it leans; off
        // the rails it keeps its own position and heading.
        let mut position = slid;
        let mut yaw = cart.yaw;
        let mut pitch = 0.0;
        if let Some(on_rail) = get_pos(&chunks, slid) {
            let ahead = get_pos_offset(&chunks, slid, SAMPLE).unwrap_or(on_rail);
            let behind = get_pos_offset(&chunks, slid, -SAMPLE).unwrap_or(on_rail);
            position = Vec3::new(on_rail.x, (ahead.y + behind.y) / 2.0, on_rail.z);
            let along = behind - ahead;
            if along.length() != 0.0 {
                let along = along.normalize();
                yaw = along.z.atan2(along.x).to_degrees();
                pitch = along.y.atan() * 73.0;
            }
        }
        let hit = cart.time_since_hit as f32 - partial;
        let damage = (cart.damage as f32 - partial).max(0.0);
        let rock = if hit > 0.0 {
            hit.sin() * hit * damage / 10.0 * cart.rock_direction as f32
        } else {
            0.0
        };
        let rotation = Quat::from_rotation_y((180.0 - yaw).to_radians())
            * Quat::from_rotation_z((-pitch).to_radians())
            * Quat::from_rotation_x(rock.to_radians());
        let brightness = entity_brightness(
            &chunks,
            light.as_deref(),
            body.translation - Vec3::Y * 0.35,
            0.7,
            subtracted,
            ambient,
        );
        for kid in kids.iter() {
            let Ok((model, mut pose)) = models.get_mut(kid) else {
                continue;
            };
            pose.set_if_neq(Transform {
                translation: position - body.translation,
                rotation,
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

//! Beta's first-person arm and held stack, rendered in their own depth pass.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::inventory::Hotbar;
use crate::item::ItemData;
use crate::item::ItemStack;
use crate::rendering::appearance::Shape;
use crate::rendering::appearance::block_appearance;
use crate::rendering::appearance::item_tile;
use crate::world::tick::WorldTick;

use super::mesh;
use crate::player::CameraBobbing;
use crate::player::Player;
use crate::player::camera_bob_pose;
use crate::player::update_camera_bobbing;

const ARM_LAYER: usize = 1;
/// Beta `EntityPlayer.swingItem` counts eight ticks.
const SWING_TICKS: i32 = 8;
/// `ItemRenderer.updateEquippedItem` moves at most this much per tick.
const EQUIP_STEP: f32 = 0.4;

#[derive(Resource)]
pub(crate) struct ArmAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    terrain: Handle<StandardMaterial>,
    items: Handle<StandardMaterial>,
    held_meshes: HashMap<VisualKey, Handle<Mesh>>,
}

#[derive(Component)]
struct ArmCamera;

#[derive(Component)]
struct FirstPersonArm {
    swinging: bool,
    /// Beta `swingProgressInt`. `-1` is the tick a swing is armed.
    swing_tick: i32,
    prev_swing: f32,
    swing: f32,
    prev_equip: f32,
    equip: f32,
    displayed: Option<VisualKey>,
}

#[derive(Component)]
struct HeldModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct VisualKey {
    id: u16,
    data: u16,
}

impl VisualKey {
    fn from_stack(stack: ItemStack) -> Self {
        Self {
            id: stack.item().as_u16(),
            data: if matches!(stack.definition().data, ItemData::Subtype(_)) {
                stack.data()
            } else {
                0
            },
        }
    }
}

impl Default for FirstPersonArm {
    fn default() -> Self {
        Self {
            swinging: false,
            swing_tick: 0,
            prev_swing: 0.0,
            swing: 0.0,
            prev_equip: 1.0,
            equip: 1.0,
            displayed: None,
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_asset::<StandardMaterial>()
        .add_systems(PreStartup, prepare_arm)
        .add_systems(OnEnter(AppScreen::Playing), show_arm_camera)
        .add_systems(OnExit(AppScreen::Playing), hide_arm_camera)
        .add_systems(
            Update,
            animate_arm
                .after(update_camera_bobbing)
                .run_if(in_state(AppScreen::Playing)),
        );
}

fn show_arm_camera(mut cameras: Query<&mut Camera, With<ArmCamera>>) {
    for mut camera in &mut cameras {
        camera.is_active = true;
    }
}

fn hide_arm_camera(mut cameras: Query<&mut Camera, With<ArmCamera>>) {
    for mut camera in &mut cameras {
        camera.is_active = false;
    }
}

fn prepare_arm(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Reference skins are local development files, never bundled game content.
    // A colored cuboid still renders if no skin is available.
    let skin = std::path::Path::new("assets/mob/char.png")
        .exists()
        .then(|| asset_server.load("mob/char.png"));
    commands.insert_resource(ArmAssets {
        mesh: meshes.add(right_arm_mesh()),
        material: materials.add(StandardMaterial {
            base_color: if skin.is_some() {
                Color::WHITE
            } else {
                Color::srgb_u8(190, 141, 106)
            },
            base_color_texture: skin,
            unlit: true,
            cull_mode: None,
            ..default()
        }),
        terrain: held_material(&mut materials, &asset_server, "terrain.png"),
        items: held_material(&mut materials, &asset_server, "gui/items.png"),
        held_meshes: HashMap::new(),
    });
}

fn held_material(
    materials: &mut Assets<StandardMaterial>,
    server: &AssetServer,
    path: &'static str,
) -> Handle<StandardMaterial> {
    let texture = std::path::Path::new("assets")
        .join(path)
        .exists()
        .then(|| server.load(path));
    materials.add(StandardMaterial {
        base_color_texture: texture,
        unlit: true,
        alpha_mode: AlphaMode::Mask(0.5),
        cull_mode: None,
        ..default()
    })
}

pub(crate) fn spawn(parent: &mut ChildSpawnerCommands, assets: &ArmAssets, fov: f32) {
    parent
        .spawn((
            Name::new("First-person arm camera"),
            ArmCamera,
            Camera3d::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            Camera {
                order: 1,
                is_active: false,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::from(PerspectiveProjection {
                fov,
                near: 0.01,
                far: 10.0,
                ..default()
            }),
            RenderLayers::layer(ARM_LAYER),
        ))
        .with_children(|camera| {
            camera.spawn((
                Name::new("Right arm"),
                FirstPersonArm::default(),
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Visibility::Visible,
                RenderLayers::layer(ARM_LAYER),
                Transform::from_matrix(arm_pose(0.0)),
            ));
            camera.spawn((
                Name::new("Held stack"),
                HeldModel,
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.terrain.clone()),
                Visibility::Hidden,
                RenderLayers::layer(ARM_LAYER),
                Transform::default(),
            ));
        });
}

/// `EntityLiving.getSwingProgress`. A wrap from the end of a swing back to 0
/// interpolates forward through the last slice instead of reversing.
pub fn interpolated_swing(previous: f32, current: f32, partial: f32) -> f32 {
    let mut delta = current - previous;
    if delta < 0.0 {
        delta += 1.0;
    }
    previous + delta * partial
}

fn animate_arm(
    tick: Res<WorldTick>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    settings: Res<GameSettings>,
    players: Query<(&CameraBobbing, &Hotbar), With<Player>>,
    mut arms: Query<(&mut FirstPersonArm, &mut Transform, &mut Visibility)>,
    mut held: Query<
        (
            &mut Mesh3d,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Transform,
            &mut Visibility,
        ),
        (With<HeldModel>, Without<FirstPersonArm>),
    >,
    mut assets: ResMut<ArmAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cameras: Query<&mut Projection, With<ArmCamera>>,
    mut wait_for_release: Local<bool>,
    mut last_frame: Local<Option<u32>>,
    frame: Res<bevy::diagnostic::FrameCount>,
) {
    if last_frame.is_none_or(|last| last.wrapping_add(1) != frame.0) {
        *wait_for_release = true;
    }
    *last_frame = Some(frame.0);

    if settings.is_changed() {
        for mut projection in &mut cameras {
            if let Projection::Perspective(perspective) = projection.as_mut() {
                perspective.fov = settings.fov_radians();
            }
        }
    }

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    // The click that grabbed the cursor must not also swing the arm.
    if !locked {
        *wait_for_release = true;
    } else if *wait_for_release {
        *wait_for_release = mouse.pressed(MouseButton::Left);
    }
    let carried = *wait_for_release;
    let Ok((bobbing, hotbar)) = players.single() else {
        return;
    };
    let walk_pose = if settings.view_bobbing {
        camera_bob_pose(bobbing)
    } else {
        Mat4::IDENTITY
    };
    let desired = hotbar.selected_stack().map(VisualKey::from_stack);
    let Ok((mut held_mesh, mut held_material, mut held_transform, mut held_visibility)) =
        held.single_mut()
    else {
        return;
    };
    for (mut arm, mut transform, mut arm_visibility) in &mut arms {
        let start_swing = locked
            && ((!carried
                && (mouse.just_pressed(MouseButton::Left)
                    || (mouse.pressed(MouseButton::Left) && !arm.swinging)))
                || mouse.just_pressed(MouseButton::Right));
        if !locked {
            arm.swinging = false;
            arm.swing_tick = 0;
        } else if start_swing {
            // `swingItem` arms the counter at -1 so the next tick lands on 0.
            arm.swinging = true;
            arm.swing_tick = -1;
        }

        for _ in 0..tick.ticks_this_frame() {
            arm.prev_swing = arm.swing;
            if arm.swinging {
                arm.swing_tick += 1;
                if arm.swing_tick >= SWING_TICKS {
                    arm.swing_tick = 0;
                    arm.swinging = false;
                }
            } else {
                arm.swing_tick = 0;
            }
            arm.swing = if arm.swing_tick < 0 {
                0.0
            } else {
                arm.swing_tick as f32 / SWING_TICKS as f32
            };

            arm.prev_equip = arm.equip;
            let target = if arm.displayed == desired { 1.0 } else { 0.0 };
            arm.equip += (target - arm.equip).clamp(-EQUIP_STEP, EQUIP_STEP);
            if arm.equip < 0.1 && arm.displayed != desired {
                arm.displayed = desired;
                if let Some(key) = desired {
                    let mesh = assets
                        .held_meshes
                        .entry(key)
                        .or_insert_with(|| meshes.add(mesh_for(key)));
                    held_mesh.0 = mesh.clone();
                    held_material.0 = if key.id < 256 {
                        assets.terrain.clone()
                    } else {
                        assets.items.clone()
                    };
                }
            }
        }

        let partial = tick.partial();
        let progress = interpolated_swing(arm.prev_swing, arm.swing, partial);
        let equip = arm.prev_equip + (arm.equip - arm.prev_equip) * partial;
        arm_visibility.set_if_neq(if arm.displayed.is_none() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        held_visibility.set_if_neq(if arm.displayed.is_some() {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        transform.set_if_neq(Transform::from_matrix(walk_pose * arm_pose(progress)));
        held_transform.set_if_neq(Transform::from_matrix(
            walk_pose
                * held_pose(
                    progress,
                    equip,
                    arm.displayed == Some(VisualKey { id: 346, data: 0 }),
                    arm.displayed.is_some_and(|key| {
                        key.id < 256
                            && block_appearance(key.id as u8, key.data).shape != Shape::Flat
                    }),
                ),
        ));
    }
}

fn mesh_for(key: VisualKey) -> Mesh {
    if key.id < 256 {
        let look = block_appearance(key.id as u8, key.data);
        if look.shape != Shape::Flat {
            return mesh::block_mesh(key.id as u8, look);
        }
        mesh::sprite_mesh(look.top, look.tint, true)
    } else {
        mesh::sprite_mesh(item_tile(key.id, key.data).unwrap_or(0), [255; 3], false)
    }
}

fn held_pose(progress: f32, equip: f32, rod: bool, modeled: bool) -> Mat4 {
    let p = progress.clamp(0.0, 1.0);
    let root = p.sqrt() * std::f32::consts::PI;
    let swing = (p * std::f32::consts::PI).sin();
    let curve = (p * p * std::f32::consts::PI).sin();
    let mut pose = Mat4::from_translation(Vec3::new(
        -root.sin() * 0.4,
        (root * 2.).sin() * 0.2,
        -swing * 0.2,
    )) * Mat4::from_translation(Vec3::new(0.56, -0.52 - (1. - equip) * 0.6, -0.72))
        * Mat4::from_rotation_y(45_f32.to_radians())
        * Mat4::from_rotation_y((-curve * 20.).to_radians())
        * Mat4::from_rotation_z((-root.sin() * 20.).to_radians())
        * Mat4::from_rotation_x((-root.sin() * 80.).to_radians())
        * Mat4::from_scale(Vec3::splat(0.4));
    if rod {
        pose *= Mat4::from_rotation_y(std::f32::consts::PI);
    }
    if !modeled {
        pose *= Mat4::from_translation(Vec3::new(0., -0.3, 0.))
            * Mat4::from_scale(Vec3::splat(1.5))
            * Mat4::from_rotation_y(50_f32.to_radians())
            * Mat4::from_rotation_z(335_f32.to_radians())
            * Mat4::from_translation(Vec3::new(-0.9375, -0.0625, 0.));
    }
    pose
}

/// ModelRenderer(40, 16).addBox(-3, -2, -2, 4, 12, 4), with
/// its pivot (-5, 2, 0). UVs follow the six classic 64x32 skin rectangles.
fn right_arm_mesh() -> Mesh {
    let x0 = -3.0 / 16.0;
    let x1 = 1.0 / 16.0;
    let y0 = -2.0 / 16.0;
    let y1 = 10.0 / 16.0;
    let z0 = -2.0 / 16.0;
    let z1 = 2.0 / 16.0;
    let faces: [([[f32; 3]; 4], [f32; 3], [f32; 4]); 6] = [
        (
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [1.0, 0.0, 0.0],
            [48.0, 20.0, 52.0, 32.0],
        ),
        (
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            [-1.0, 0.0, 0.0],
            [40.0, 20.0, 44.0, 32.0],
        ),
        (
            [[x1, y0, z1], [x0, y0, z1], [x0, y0, z0], [x1, y0, z0]],
            [0.0, -1.0, 0.0],
            [44.0, 16.0, 48.0, 20.0],
        ),
        (
            [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]],
            [0.0, 1.0, 0.0],
            [48.0, 16.0, 52.0, 20.0],
        ),
        (
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [0.0, 0.0, -1.0],
            [44.0, 20.0, 48.0, 32.0],
        ),
        (
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [0.0, 0.0, 1.0],
            [52.0, 20.0, 56.0, 32.0],
        ),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (corners, normal, [u0, v0, u1, v1]) in faces {
        let base = positions.len() as u32;
        positions.extend(corners);
        normals.extend([normal; 4]);
        // TexturedQuad assigns its first vertex the far U, near V.
        uvs.extend([
            [(u1 - 0.1) / 64.0, (v0 + 0.1) / 32.0],
            [(u0 + 0.1) / 64.0, (v0 + 0.1) / 32.0],
            [(u0 + 0.1) / 64.0, (v1 - 0.1) / 32.0],
            [(u1 - 0.1) / 64.0, (v1 - 0.1) / 32.0],
        ]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// Exact empty-hand transform sequence from Beta's `ItemRenderer`, including
/// the swing translation, yaw/roll and the arm's ModelBiped pivot.
fn arm_pose(progress: f32) -> Mat4 {
    let p = progress.clamp(0.0, 1.0);
    let root = p.sqrt() * std::f32::consts::PI;
    let swing = (p * std::f32::consts::PI).sin();
    let curve = (p * p * std::f32::consts::PI).sin();
    Mat4::from_translation(Vec3::new(
        -root.sin() * 0.3,
        (root * 2.0).sin() * 0.4,
        -swing * 0.4,
    )) * Mat4::from_translation(Vec3::new(0.64, -0.6, -0.72))
        * Mat4::from_rotation_y(45.0_f32.to_radians())
        * Mat4::from_rotation_y((root.sin() * 70.0).to_radians())
        * Mat4::from_rotation_z((-curve * 20.0).to_radians())
        * Mat4::from_translation(Vec3::new(-1.0, 3.6, 3.5))
        * Mat4::from_rotation_z(120.0_f32.to_radians())
        * Mat4::from_rotation_x(200.0_f32.to_radians())
        * Mat4::from_rotation_y((-135.0_f32).to_radians())
        * Mat4::from_translation(Vec3::new(5.6, 0.0, 0.0))
        * Mat4::from_translation(Vec3::new(-5.0 / 16.0, 2.0 / 16.0, 0.0))
}

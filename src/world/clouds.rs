//! Beta cloud sheet from `environment/clouds.png`.
//!
//! Fast graphics draws the flat blended plane from `RenderGlobal.renderClouds`.
//! Fancy and Ultra draw the 4-block-tall cells from `renderCloudsFancy`. Both
//! sit at world Y 108.33, drift on X by 0.03 blocks per tick, and take their
//! color from `World.drawClouds`.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageAddressMode;
use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::light::NotShadowCaster;
use bevy::math::Affine2;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::player::Player;

use super::sky::daylight_factor;
use super::tick::WorldTick;

/// `WorldProvider.getCloudHeight() + 0.33`.
pub const CLOUD_HEIGHT: f32 = 108.33;
const SCROLL_PER_TICK: f64 = 0.03;
const SCROLL_PERIOD: f64 = 2048.0;
const FAST_UV_SCALE: f32 = 1.0 / 2048.0;
const FANCY_UV_SCALE: f32 = 1.0 / 256.0;
const FANCY_WORLD_SCALE: f32 = 12.0;
const FANCY_CELL: f32 = 8.0;
const FANCY_THICKNESS: f32 = 4.0;
const FANCY_INSET: f32 = 9.765625e-4;
const CLOUD_ALPHA: f32 = 0.8;

#[derive(Component)]
struct FastClouds;

#[derive(Component)]
struct FancyClouds;

#[derive(Resource)]
struct CloudsSpawned;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (ensure_clouds, update_clouds).chain());
}

/// `World.drawClouds` with no rain or thunder. `daylight` is the celestial factor.
pub fn cloud_color(daylight: f32) -> [f32; 3] {
    let day = daylight.clamp(0.0, 1.0);
    [day * 0.9 + 0.1, day * 0.9 + 0.1, day * 0.85 + 0.15]
}

/// Distance the sheet has drifted on X, wrapped so the value stays small.
pub fn cloud_scroll_blocks(world_time: u64, partial: f32) -> f32 {
    let distance = (world_time as f64 + f64::from(partial)) * SCROLL_PER_TICK;
    distance.rem_euclid(SCROLL_PERIOD) as f32
}

fn wrap_period(value: f64) -> f64 {
    value.rem_euclid(SCROLL_PERIOD)
}

pub fn fast_cloud_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let mut x = -256.0;
    while x < 256.0 {
        let mut z = -256.0;
        while z < 256.0 {
            let base = positions.len() as u32;
            positions.extend([
                [x, 0.0, z + 32.0],
                [x + 32.0, 0.0, z + 32.0],
                [x + 32.0, 0.0, z],
                [x, 0.0, z],
            ]);
            uvs.extend([
                [x * FAST_UV_SCALE, (z + 32.0) * FAST_UV_SCALE],
                [(x + 32.0) * FAST_UV_SCALE, (z + 32.0) * FAST_UV_SCALE],
                [(x + 32.0) * FAST_UV_SCALE, z * FAST_UV_SCALE],
                [x * FAST_UV_SCALE, z * FAST_UV_SCALE],
            ]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            z += 32.0;
        }
        x += 32.0;
    }
    let colors = vec![[1.0, 1.0, 1.0, 1.0]; positions.len()];
    cloud_mesh(positions, uvs, colors, indices)
}

pub fn fancy_cloud_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let mut push = |quad: [[f32; 3]; 4], quad_uv: [[f32; 2]; 4], shade: f32| {
        let base = positions.len() as u32;
        positions.extend(quad);
        uvs.extend(quad_uv);
        colors.extend([[shade, shade, shade, 1.0]; 4]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    let wx = |pre: f32| pre * FANCY_WORLD_SCALE;
    let uv = |pre: f32| pre * FANCY_UV_SCALE;

    for cell_x in -2..=3 {
        for cell_z in -2..=3 {
            let x0 = cell_x as f32 * FANCY_CELL;
            let z0 = cell_z as f32 * FANCY_CELL;
            let x1 = x0 + FANCY_CELL;
            let z1 = z0 + FANCY_CELL;
            let bottom = [
                [wx(x0), 0.0, wx(z1)],
                [wx(x1), 0.0, wx(z1)],
                [wx(x1), 0.0, wx(z0)],
                [wx(x0), 0.0, wx(z0)],
            ];
            let bottom_uv = [
                [uv(x0), uv(z1)],
                [uv(x1), uv(z1)],
                [uv(x1), uv(z0)],
                [uv(x0), uv(z0)],
            ];
            push(bottom, bottom_uv, 0.7);
            let top_y = FANCY_THICKNESS - FANCY_INSET;
            let top = [
                [wx(x0), top_y, wx(z1)],
                [wx(x1), top_y, wx(z1)],
                [wx(x1), top_y, wx(z0)],
                [wx(x0), top_y, wx(z0)],
            ];
            push(top, bottom_uv, 1.0);

            if cell_x > -1 {
                for step in 0..FANCY_CELL as i32 {
                    let edge = x0 + step as f32;
                    push(
                        [
                            [wx(edge), 0.0, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z0)],
                            [wx(edge), 0.0, wx(z0)],
                        ],
                        [
                            [uv(edge + 0.5), uv(z1)],
                            [uv(edge + 0.5), uv(z1)],
                            [uv(edge + 0.5), uv(z0)],
                            [uv(edge + 0.5), uv(z0)],
                        ],
                        0.9,
                    );
                }
            }
            if cell_x <= 1 {
                for step in 0..FANCY_CELL as i32 {
                    let edge = x0 + step as f32 + 1.0 - FANCY_INSET;
                    push(
                        [
                            [wx(edge), 0.0, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z0)],
                            [wx(edge), 0.0, wx(z0)],
                        ],
                        [
                            [uv(x0 + step as f32 + 0.5), uv(z1)],
                            [uv(x0 + step as f32 + 0.5), uv(z1)],
                            [uv(x0 + step as f32 + 0.5), uv(z0)],
                            [uv(x0 + step as f32 + 0.5), uv(z0)],
                        ],
                        0.9,
                    );
                }
            }
            if cell_z > -1 {
                for step in 0..FANCY_CELL as i32 {
                    let edge = z0 + step as f32;
                    push(
                        [
                            [wx(x0), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), 0.0, wx(edge)],
                            [wx(x0), 0.0, wx(edge)],
                        ],
                        [
                            [uv(x0), uv(edge + 0.5)],
                            [uv(x1), uv(edge + 0.5)],
                            [uv(x1), uv(edge + 0.5)],
                            [uv(x0), uv(edge + 0.5)],
                        ],
                        0.8,
                    );
                }
            }
            if cell_z <= 1 {
                for step in 0..FANCY_CELL as i32 {
                    let edge = z0 + step as f32 + 1.0 - FANCY_INSET;
                    push(
                        [
                            [wx(x0), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), 0.0, wx(edge)],
                            [wx(x0), 0.0, wx(edge)],
                        ],
                        [
                            [uv(x0), uv(z0 + step as f32 + 0.5)],
                            [uv(x1), uv(z0 + step as f32 + 0.5)],
                            [uv(x1), uv(z0 + step as f32 + 0.5)],
                            [uv(x0), uv(z0 + step as f32 + 0.5)],
                        ],
                        0.8,
                    );
                }
            }
        }
    }
    cloud_mesh(positions, uvs, colors, indices)
}

fn cloud_mesh(
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
) -> Mesh {
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

fn cloud_material(texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(1.0, 1.0, 1.0).with_alpha(CLOUD_ALPHA),
        base_color_texture: Some(texture),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

fn ensure_clouds(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing: Query<(), With<FastClouds>>,
) {
    if !existing.is_empty() {
        return;
    }
    if !Path::new("assets/environment/clouds.png").exists() {
        return;
    }
    let texture = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            let mut descriptor = ImageSamplerDescriptor::linear();
            descriptor.address_mode_u = ImageAddressMode::Repeat;
            descriptor.address_mode_v = ImageAddressMode::Repeat;
            descriptor.address_mode_w = ImageAddressMode::Repeat;
            settings.sampler = ImageSampler::Descriptor(descriptor);
        })
        .load("environment/clouds.png".to_string());
    let fast_material = materials.add(cloud_material(texture.clone()));
    let fancy_material = materials.add(cloud_material(texture));
    commands.insert_resource(CloudsSpawned);
    commands.spawn((
        Name::new("Fast clouds"),
        FastClouds,
        Mesh3d(meshes.add(fast_cloud_mesh())),
        MeshMaterial3d(fast_material),
        Transform::from_xyz(0.0, CLOUD_HEIGHT, 0.0),
        NotShadowCaster,
        NoFrustumCulling,
    ));
    commands.spawn((
        Name::new("Fancy clouds"),
        FancyClouds,
        Mesh3d(meshes.add(fancy_cloud_mesh())),
        MeshMaterial3d(fancy_material),
        Transform::from_xyz(0.0, CLOUD_HEIGHT, 0.0),
        Visibility::Hidden,
        NotShadowCaster,
        NoFrustumCulling,
    ));
}

fn update_clouds(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    assets: Option<Res<CloudsSpawned>>,
    player: Query<&Transform, (With<Player>, Without<FastClouds>, Without<FancyClouds>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fast: Query<
        (
            &mut Transform,
            &mut Visibility,
            &MeshMaterial3d<StandardMaterial>,
        ),
        (With<FastClouds>, Without<FancyClouds>, Without<Player>),
    >,
    mut fancy: Query<
        (
            &mut Transform,
            &mut Visibility,
            &MeshMaterial3d<StandardMaterial>,
        ),
        (With<FancyClouds>, Without<FastClouds>, Without<Player>),
    >,
) {
    if assets.is_none() {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };
    let fancy_mode = settings.graphics.fancy_leaves();
    let color = cloud_color(daylight_factor(super::sky::celestial_angle(
        tick.world_time(),
        tick.partial(),
    )));
    let tint = Color::srgb(color[0], color[1], color[2]).with_alpha(CLOUD_ALPHA);
    let scroll = f64::from(cloud_scroll_blocks(tick.world_time(), tick.partial()));
    let player_x = f64::from(player.translation.x);
    let player_z = f64::from(player.translation.z);

    if let Ok((mut transform, mut visibility, material)) = fast.single_mut() {
        *visibility = if fancy_mode {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        transform.translation = Vec3::new(player.translation.x, CLOUD_HEIGHT, player.translation.z);
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base_color = tint;
            let wrapped_x = wrap_period(player_x + scroll) as f32;
            let wrapped_z = wrap_period(player_z) as f32;
            mat.uv_transform = Affine2::from_translation(Vec2::new(
                wrapped_x * FAST_UV_SCALE,
                wrapped_z * FAST_UV_SCALE,
            ));
        }
    }

    if let Ok((mut transform, mut visibility, material)) = fancy.single_mut() {
        *visibility = if fancy_mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let scaled_x = wrap_period((player_x + scroll) / f64::from(FANCY_WORLD_SCALE));
        let scaled_z = wrap_period(player_z / f64::from(FANCY_WORLD_SCALE) + 0.33);
        let frac_x = (scaled_x - scaled_x.floor()) as f32;
        let frac_z = (scaled_z - scaled_z.floor()) as f32;
        transform.translation = Vec3::new(
            player.translation.x - frac_x * FANCY_WORLD_SCALE,
            CLOUD_HEIGHT,
            player.translation.z - frac_z * FANCY_WORLD_SCALE,
        );
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base_color = tint;
            mat.uv_transform = Affine2::from_translation(Vec2::new(
                scaled_x.floor() as f32 * FANCY_UV_SCALE,
                scaled_z.floor() as f32 * FANCY_UV_SCALE,
            ));
        }
    }
}

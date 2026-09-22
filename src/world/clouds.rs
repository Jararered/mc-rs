//! Clouds from `environment/clouds.png`, anchored in the world.
//!
//! Fast graphics draws the flat sheet from `RenderGlobal.renderClouds`. Fancy
//! and Ultra draw the 4-block columns from `renderCloudsFancy`, including the
//! sides of each column. Empty texels are cut out and the clouds themselves
//! are opaque. Both sit at world Y 108.33 and drift on X by 0.03 blocks per
//! tick. Color comes from `World.drawClouds`.

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
use crate::physics::PhysicsSet;
use crate::player::Player;

use super::sky::daylight_factor;
use super::tick::WorldTick;

/// `WorldProvider.getCloudHeight() + 0.33`.
pub const CLOUD_HEIGHT: f32 = 108.33;
const SCROLL_PER_TICK: f64 = 0.03;
const SCROLL_PERIOD: f64 = 2048.0;
/// One texel of the 256² sheet is 8 blocks, so the whole texture is 2048 across.
const CLOUD_EXTENT: f32 = (SCROLL_PERIOD as f32) * 0.5;
const FANCY_UV_SCALE: f32 = 1.0 / 256.0;
const FANCY_WORLD_SCALE: f32 = 12.0;
const FANCY_CELL: f32 = 8.0;
const FANCY_THICKNESS: f32 = 4.0;
const FANCY_INSET: f32 = 9.765625e-4;

#[derive(Component)]
struct FastClouds;

#[derive(Component)]
struct FancyClouds;

#[derive(Resource)]
struct CloudsSpawned;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (ensure_clouds, update_clouds)
            .chain()
            .after(PhysicsSet::Integrate),
    );
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

/// UV translation that keeps the pattern fixed in the world.
///
/// Mesh UVs run 0..1 across the 2048-block sheet centered on the entity, so
/// the offset subtracts that half-extent and then adds the player position.
pub fn cloud_uv_offset(player_x: f32, player_z: f32, scroll: f32) -> Vec2 {
    let u = wrap_uv(f64::from(player_x) + f64::from(scroll) - f64::from(CLOUD_EXTENT));
    let v = wrap_uv(f64::from(player_z) - f64::from(CLOUD_EXTENT));
    Vec2::new(u, v)
}

fn wrap_uv(blocks: f64) -> f32 {
    (wrap_blocks(blocks) / SCROLL_PERIOD) as f32
}

fn wrap_blocks(blocks: f64) -> f64 {
    blocks.rem_euclid(SCROLL_PERIOD)
}

/// Fancy column anchor from `renderCloudsFancy`. The mesh is a window of
/// columns around the player; this snaps that window to the world grid and
/// shifts the texture so the columns stay put as the player walks.
pub fn fancy_cloud_anchor(player_x: f32, player_z: f32, scroll: f32) -> (Vec3, Vec2) {
    let scaled_x =
        wrap_blocks((f64::from(player_x) + f64::from(scroll)) / f64::from(FANCY_WORLD_SCALE));
    let scaled_z =
        wrap_blocks(f64::from(player_z) / f64::from(FANCY_WORLD_SCALE) + f64::from(0.33f32));
    let frac_x = (scaled_x - scaled_x.floor()) as f32;
    let frac_z = (scaled_z - scaled_z.floor()) as f32;
    let place = Vec3::new(
        player_x - frac_x * FANCY_WORLD_SCALE,
        CLOUD_HEIGHT,
        player_z - frac_z * FANCY_WORLD_SCALE,
    );
    let uv = Vec2::new(
        scaled_x.floor() as f32 * FANCY_UV_SCALE,
        scaled_z.floor() as f32 * FANCY_UV_SCALE,
    );
    (place, uv)
}

const SHEET_UV: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

fn sheet_corners(y: f32) -> [[f32; 3]; 4] {
    let h = CLOUD_EXTENT;
    [[-h, y, h], [h, y, h], [h, y, -h], [-h, y, -h]]
}

pub fn fast_cloud_mesh() -> Mesh {
    let positions = sheet_corners(0.0).to_vec();
    let colors = vec![[1.0, 1.0, 1.0, 1.0]; positions.len()];
    cloud_mesh(positions, SHEET_UV.to_vec(), colors, vec![0, 1, 2, 0, 2, 3])
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

    // `renderCloudsFancy`: cells -2..=3, each 8 pre-scale units, drawn at 12
    // blocks per unit. Sides are one strip per texel so each cloud column has
    // a top, a bottom, and walls.
    for cell_x in -2..=3 {
        for cell_z in -2..=3 {
            let x0 = cell_x as f32 * FANCY_CELL;
            let z0 = cell_z as f32 * FANCY_CELL;
            let x1 = x0 + FANCY_CELL;
            let z1 = z0 + FANCY_CELL;
            let cap_uv = [
                [uv(x0), uv(z1)],
                [uv(x1), uv(z1)],
                [uv(x1), uv(z0)],
                [uv(x0), uv(z0)],
            ];
            push(
                [
                    [wx(x0), 0.0, wx(z1)],
                    [wx(x1), 0.0, wx(z1)],
                    [wx(x1), 0.0, wx(z0)],
                    [wx(x0), 0.0, wx(z0)],
                ],
                cap_uv,
                0.7,
            );
            let top_y = FANCY_THICKNESS - FANCY_INSET;
            push(
                [
                    [wx(x0), top_y, wx(z1)],
                    [wx(x1), top_y, wx(z1)],
                    [wx(x1), top_y, wx(z0)],
                    [wx(x0), top_y, wx(z0)],
                ],
                cap_uv,
                1.0,
            );

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
                    let column = x0 + step as f32 + 0.5;
                    push(
                        [
                            [wx(edge), 0.0, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z1)],
                            [wx(edge), FANCY_THICKNESS, wx(z0)],
                            [wx(edge), 0.0, wx(z0)],
                        ],
                        [
                            [uv(column), uv(z1)],
                            [uv(column), uv(z1)],
                            [uv(column), uv(z0)],
                            [uv(column), uv(z0)],
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
                    let column = z0 + step as f32 + 0.5;
                    push(
                        [
                            [wx(x0), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), FANCY_THICKNESS, wx(edge)],
                            [wx(x1), 0.0, wx(edge)],
                            [wx(x0), 0.0, wx(edge)],
                        ],
                        [
                            [uv(x0), uv(column)],
                            [uv(x1), uv(column)],
                            [uv(x1), uv(column)],
                            [uv(x0), uv(column)],
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
        base_color: Color::srgb(1.0, 1.0, 1.0),
        base_color_texture: Some(texture),
        unlit: true,
        // Cut out empty texels. Passing pixels stay fully opaque, so the
        // column sides read as solid blocks rather than a glassy sheet.
        alpha_mode: AlphaMode::Mask(0.5),
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
            // Repeat wraps the slow drift. Filtering stays nearest, matching
            // the engine-wide sampler in `main`.
            let mut descriptor = ImageSamplerDescriptor {
                lod_max_clamp: 0.0,
                ..ImageSamplerDescriptor::nearest()
            };
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
        Visibility::default(),
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
    let tint = Color::srgb(color[0], color[1], color[2]);
    let scroll = cloud_scroll_blocks(tick.world_time(), tick.partial());
    let uv_offset = Affine2::from_translation(cloud_uv_offset(
        player.translation.x,
        player.translation.z,
        scroll,
    ));
    let place = Vec3::new(player.translation.x, CLOUD_HEIGHT, player.translation.z);
    let (fancy_place, fancy_uv) =
        fancy_cloud_anchor(player.translation.x, player.translation.z, scroll);

    if let Ok((mut transform, mut visibility, material)) = fast.single_mut() {
        *visibility = if fancy_mode {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        transform.translation = place;
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base_color = tint;
            mat.uv_transform = uv_offset;
        }
    }

    if let Ok((mut transform, mut visibility, material)) = fancy.single_mut() {
        *visibility = if fancy_mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.translation = fancy_place;
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base_color = tint;
            mat.uv_transform = Affine2::from_translation(fancy_uv);
        }
    }
}

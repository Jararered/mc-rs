//! Clouds from `environment/clouds.png`, anchored in the world.
//!
//! Fast graphics draws the flat sheet from `RenderGlobal.renderClouds`. Fancy
//! and Ultra draw the 4-block columns from `renderCloudsFancy`, including the
//! sides of each column. Empty texels are cut out and the clouds themselves
//! are opaque. Both sit at `GameSettings::cloud_height + 0.33`, the client's
//! stand-in for Beta's `WorldProvider.getCloudHeight()`, and drift on X by
//! 0.03 blocks per tick. Color comes from `World.drawClouds`.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageAddressMode;
use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::light::NotShadowCaster;
use bevy::material::OpaqueRendererMethod;
use bevy::math::Affine2;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::DEFAULT_CLOUD_HEIGHT;
use crate::app::settings::GameSettings;
use crate::physics::PhysicsSet;
use crate::player::Player;

use super::sky::daylight_factor;
use super::textures::InstanceTint;
use super::textures::TintedMaterial;
use super::textures::tint_tag;
use super::tick::WorldTick;

/// Both cloud renderers add this to `getCloudHeight()` before drawing.
pub const CLOUD_RENDER_OFFSET: f32 = 0.33;
/// World Y of the sheet at the default setting, `getCloudHeight() + 0.33`.
pub const CLOUD_HEIGHT: f32 = DEFAULT_CLOUD_HEIGHT + CLOUD_RENDER_OFFSET;
const SCROLL_PER_TICK: f64 = 0.03;
const SCROLL_PERIOD: f64 = 2048.0;
/// One texel of the 256² sheet is 8 blocks, so the whole texture is 2048
/// across. The fast sheet spans two periods so it can slide by up to half a
/// period to keep the texture world-locked, and still reach 1024 blocks past
/// the player on every side.
const CLOUD_EXTENT: f32 = SCROLL_PERIOD as f32;
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

/// World Y of the sheet for a `GameSettings::cloud_height` value.
pub fn cloud_render_y(cloud_height: f32) -> f32 {
    cloud_height + CLOUD_RENDER_OFFSET
}

/// Where the fast sheet sits so the pattern stays fixed in the world.
///
/// Sheet UVs run `(local + CLOUD_EXTENT) / SCROLL_PERIOD`, and the extent is a
/// whole period, so a world point `x` samples `(x - sheet_x) / period`. The
/// sheet is placed at `-scroll` modulo one period, as close to the player as
/// that allows, which samples `(x + scroll) / period` without any UV offset.
pub fn fast_cloud_anchor(player_x: f32, player_z: f32, scroll: f32, cloud_y: f32) -> Vec3 {
    let half = SCROLL_PERIOD * 0.5;
    let nearest = |blocks: f64| (blocks + half).rem_euclid(SCROLL_PERIOD) - half;
    let x = f64::from(player_x) - nearest(f64::from(player_x) + f64::from(scroll));
    let z = f64::from(player_z) - nearest(f64::from(player_z));
    Vec3::new(x as f32, cloud_y, z as f32)
}

/// Texture coordinate of a world point under the fast sheet placed at `sheet`.
pub fn fast_cloud_uv(sheet: Vec3, world_x: f32, world_z: f32) -> Vec2 {
    let period = SCROLL_PERIOD as f32;
    Vec2::new(
        (world_x - sheet.x + CLOUD_EXTENT) / period,
        (world_z - sheet.z + CLOUD_EXTENT) / period,
    )
}

fn wrap_blocks(blocks: f64) -> f64 {
    blocks.rem_euclid(SCROLL_PERIOD)
}

/// Fancy column anchor from `renderCloudsFancy`. The mesh is a window of
/// columns around the player; this snaps that window to the world grid and
/// shifts the texture so the columns stay put as the player walks.
pub fn fancy_cloud_anchor(player_x: f32, player_z: f32, scroll: f32, cloud_y: f32) -> (Vec3, Vec2) {
    let scaled_x =
        wrap_blocks((f64::from(player_x) + f64::from(scroll)) / f64::from(FANCY_WORLD_SCALE));
    let scaled_z =
        wrap_blocks(f64::from(player_z) / f64::from(FANCY_WORLD_SCALE) + f64::from(0.33f32));
    let frac_x = (scaled_x - scaled_x.floor()) as f32;
    let frac_z = (scaled_z - scaled_z.floor()) as f32;
    let place = Vec3::new(
        player_x - frac_x * FANCY_WORLD_SCALE,
        cloud_y,
        player_z - frac_z * FANCY_WORLD_SCALE,
    );
    let uv = Vec2::new(
        scaled_x.floor() as f32 * FANCY_UV_SCALE,
        scaled_z.floor() as f32 * FANCY_UV_SCALE,
    );
    (place, uv)
}

/// Two texture periods across, repeated by the sampler.
const SHEET_UV: [[f32; 2]; 4] = [[0.0, 2.0], [2.0, 2.0], [2.0, 0.0], [0.0, 0.0]];

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

/// Daylight tint comes from each cloud entity's `MeshTag`, so the material
/// itself only changes when the fancy texel window moves.
fn cloud_material(texture: Handle<Image>) -> TintedMaterial {
    TintedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            base_color_texture: Some(texture),
            unlit: true,
            // Cut out empty texels. Passing pixels stay fully opaque, so the
            // column sides read as solid blocks rather than a glassy sheet.
            alpha_mode: AlphaMode::Mask(0.5),
            // The tint is applied by a forward fragment shader, so clouds stay
            // out of the Ultra deferred G-buffer.
            opaque_render_method: OpaqueRendererMethod::Forward,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: InstanceTint {},
    }
}

fn ensure_clouds(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    settings: Res<GameSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TintedMaterial>>,
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
    let cloud_y = cloud_render_y(settings.cloud_height);
    commands.insert_resource(CloudsSpawned);
    commands.spawn((
        Name::new("Fast clouds"),
        FastClouds,
        Mesh3d(meshes.add(fast_cloud_mesh())),
        MeshMaterial3d(fast_material),
        tint_tag(Color::WHITE),
        Transform::from_xyz(0.0, cloud_y, 0.0),
        Visibility::default(),
        NotShadowCaster,
        NoFrustumCulling,
    ));
    commands.spawn((
        Name::new("Fancy clouds"),
        FancyClouds,
        Mesh3d(meshes.add(fancy_cloud_mesh())),
        MeshMaterial3d(fancy_material),
        tint_tag(Color::WHITE),
        Transform::from_xyz(0.0, cloud_y, 0.0),
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
    mut materials: ResMut<Assets<TintedMaterial>>,
    mut fast: Query<
        (&mut Transform, &mut Visibility, &mut MeshTag),
        (With<FastClouds>, Without<FancyClouds>, Without<Player>),
    >,
    mut fancy: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &MeshMaterial3d<TintedMaterial>,
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
    let tint = tint_tag(Color::srgb(color[0], color[1], color[2]));
    let scroll = cloud_scroll_blocks(tick.world_time(), tick.partial());
    let cloud_y = cloud_render_y(settings.cloud_height);
    let (fancy_place, fancy_uv) =
        fancy_cloud_anchor(player.translation.x, player.translation.z, scroll, cloud_y);

    if let Ok((mut transform, mut visibility, mut tag)) = fast.single_mut() {
        visibility.set_if_neq(if fancy_mode {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        transform.translation =
            fast_cloud_anchor(player.translation.x, player.translation.z, scroll, cloud_y);
        tag.set_if_neq(tint.clone());
    }

    if let Ok((mut transform, mut visibility, mut tag, material)) = fancy.single_mut() {
        visibility.set_if_neq(if fancy_mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        transform.translation = fancy_place;
        tag.set_if_neq(tint);
        // The texel window shifts only when the player or the drift crosses
        // a 12-block column, so this rarely writes the material.
        let moved = materials
            .get(&material.0)
            .is_some_and(|current| current.base.uv_transform.translation != fancy_uv);
        if moved && let Some(mut current) = materials.get_mut(&material.0) {
            current.base.uv_transform = Affine2::from_translation(fancy_uv);
        }
    }
}

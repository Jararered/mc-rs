//! Clouds from `environment/clouds.png`, anchored in the world.
//!
//! Fast graphics draws the flat sheet from `RenderGlobal.renderClouds`. Fancy
//! and Ultra draw the 4-block columns from `renderCloudsFancy`, including the
//! sides of each column. Empty texels are cut out and the clouds themselves
//! are opaque. Both sit at `GameSettings::cloud_height + 0.33`, the client's
//! stand-in for Beta's `WorldProvider.getCloudHeight()`, and drift on X by
//! 0.03 blocks per tick. Color comes from `World.drawClouds`.
//!
//! Both sheets are sized from `GameSettings::render_distance` and rebuilt when
//! it changes, so clouds reach exactly as far as the loaded world. The world
//! fog is already opaque at the render distance, which hides the sheet's edge:
//! a sheet point that many blocks out is at least that far from the eye, and
//! the fog color there is the sky's horizon color.

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
const FANCY_UV_SCALE: f32 = 1.0 / 256.0;
const FANCY_WORLD_SCALE: f32 = 12.0;
const FANCY_CELL: f32 = 8.0;
/// One fancy cell is `FANCY_CELL` texels drawn at `FANCY_WORLD_SCALE` blocks.
const FANCY_CELL_BLOCKS: f32 = FANCY_CELL * FANCY_WORLD_SCALE;
const FANCY_THICKNESS: f32 = 4.0;
const FANCY_INSET: f32 = 9.765625e-4;

#[derive(Component)]
struct FastClouds;

#[derive(Component)]
struct FancyClouds;

#[derive(Resource)]
struct CloudsSpawned {
    /// Render distance, in chunks, the current meshes were built for.
    distance: i32,
}

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

/// Half the cloud sheet's width in blocks, on every side of the player. This
/// is the loaded view radius, so a sheet reaches exactly as far as the world.
pub fn cloud_half_extent_blocks(render_chunks: i32) -> f32 {
    super::sky::view_distance_blocks(render_chunks)
}

/// Fancy cells needed on each side of the player to cover `half_extent`.
///
/// The window is snapped to the `FANCY_WORLD_SCALE` grid, which can leave the
/// player up to that far from the window's own center, so the coverage has to
/// clear `half_extent` by a whole unit of it.
pub fn fancy_cloud_cells(half_extent: f32) -> i32 {
    ((half_extent + FANCY_WORLD_SCALE) / FANCY_CELL_BLOCKS)
        .ceil()
        .max(1.0) as i32
}

/// Where the fast sheet sits: centered on the player at the cloud height.
///
/// The pattern is world-locked by UVs rather than by placement. The sheet's
/// baked UVs are `local / SCROLL_PERIOD` and its material carries
/// `fast_cloud_uv_offset`, so a world point samples `(world + scroll) / period`
/// however the sheet moves. Beta slid the sheet a whole period instead, which
/// is why its extent had to be two periods wide.
pub fn fast_cloud_anchor(player_x: f32, player_z: f32, cloud_y: f32) -> Vec3 {
    Vec3::new(player_x, cloud_y, player_z)
}

/// Texture offset the fast sheet's material carries.
///
/// Wrapped into `0..1` so the total stays small next to the baked UVs: a texel
/// is `1 / 256` of the period, far above the rounding of a value this size.
pub fn fast_cloud_uv_offset(player_x: f32, player_z: f32, scroll: f32) -> Vec2 {
    Vec2::new(
        (f64::from(player_x + scroll) / SCROLL_PERIOD).rem_euclid(1.0) as f32,
        (f64::from(player_z) / SCROLL_PERIOD).rem_euclid(1.0) as f32,
    )
}

/// Texture coordinate of a world point under the fast sheet placed at `sheet`,
/// with `offset` as the material's `uv_transform` translation. This mirrors
/// what the shader sums from the baked vertex UVs.
pub fn fast_cloud_uv(sheet: Vec3, offset: Vec2, world_x: f32, world_z: f32) -> Vec2 {
    let period = SCROLL_PERIOD as f32;
    offset + Vec2::new((world_x - sheet.x) / period, (world_z - sheet.z) / period)
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

/// UVs are `local / SCROLL_PERIOD`; `fast_cloud_uv_offset` supplies the rest
/// through the material, so they stay small at any render distance.
pub fn fast_cloud_mesh(half_extent: f32) -> Mesh {
    let period = SCROLL_PERIOD as f32;
    let edge = half_extent / period;
    let positions = [
        [-half_extent, 0.0, half_extent],
        [half_extent, 0.0, half_extent],
        [half_extent, 0.0, -half_extent],
        [-half_extent, 0.0, -half_extent],
    ]
    .to_vec();
    let uvs = [[-edge, edge], [edge, edge], [edge, -edge], [-edge, -edge]].to_vec();
    let colors = vec![[1.0, 1.0, 1.0, 1.0]; positions.len()];
    cloud_mesh(positions, uvs, colors, vec![0, 1, 2, 0, 2, 3])
}

pub fn fancy_cloud_mesh(cells: i32) -> Mesh {
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

    // `renderCloudsFancy`: cells of 8 pre-scale units, drawn at 12 blocks per
    // unit, `-cells..=cells` around the player so the window covers the render
    // distance. Sides are one strip per texel so each cloud column has a top, a
    // bottom, and walls. Beta's own `> -1` / `<= 1` wall bounds only make sense
    // for its fixed 6-cell window, so they are the window's own edges here.
    for cell_x in -cells..=cells {
        for cell_z in -cells..=cells {
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

            if cell_x > -cells {
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
            if cell_x < cells {
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
            if cell_z > -cells {
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
            if cell_z < cells {
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
    let half_extent = cloud_half_extent_blocks(settings.render_distance);
    commands.insert_resource(CloudsSpawned {
        distance: settings.render_distance,
    });
    commands.spawn((
        Name::new("Fast clouds"),
        FastClouds,
        Mesh3d(meshes.add(fast_cloud_mesh(half_extent))),
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
        Mesh3d(meshes.add(fancy_cloud_mesh(fancy_cloud_cells(half_extent)))),
        MeshMaterial3d(fancy_material),
        tint_tag(Color::WHITE),
        Transform::from_xyz(0.0, cloud_y, 0.0),
        Visibility::Hidden,
        NotShadowCaster,
        NoFrustumCulling,
    ));
}

/// Moves a material's `uv_transform`, skipping the write when it already holds
/// `offset`. The fast sheet's offset changes with the drift, so it writes
/// every frame; the fancy window moves only when a 12-block column goes by.
fn set_uv_offset(
    materials: &mut Assets<TintedMaterial>,
    handle: &Handle<TintedMaterial>,
    offset: Vec2,
) {
    let moved = materials
        .get(handle)
        .is_some_and(|current| current.base.uv_transform.translation != offset);
    if moved && let Some(mut current) = materials.get_mut(handle) {
        current.base.uv_transform = Affine2::from_translation(offset);
    }
}

fn update_clouds(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    spawned: Option<ResMut<CloudsSpawned>>,
    player: Query<&Transform, (With<Player>, Without<FastClouds>, Without<FancyClouds>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TintedMaterial>>,
    mut fast: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &mut Mesh3d,
            &MeshMaterial3d<TintedMaterial>,
        ),
        (With<FastClouds>, Without<FancyClouds>, Without<Player>),
    >,
    mut fancy: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &mut Mesh3d,
            &MeshMaterial3d<TintedMaterial>,
        ),
        (With<FancyClouds>, Without<FastClouds>, Without<Player>),
    >,
) {
    let Some(mut spawned) = spawned else {
        return;
    };
    let Ok(player) = player.single() else {
        return;
    };
    // The render distance setting is the only thing that resizes the sheets.
    if spawned.distance != settings.render_distance {
        let half_extent = cloud_half_extent_blocks(settings.render_distance);
        let fast_mesh = meshes.add(fast_cloud_mesh(half_extent));
        let fancy_mesh = meshes.add(fancy_cloud_mesh(fancy_cloud_cells(half_extent)));
        // Field 3 of either query is its `Mesh3d`.
        for mut entity in fast.iter_mut() {
            entity.3.0 = fast_mesh.clone();
        }
        for mut entity in fancy.iter_mut() {
            entity.3.0 = fancy_mesh.clone();
        }
        spawned.distance = settings.render_distance;
    }
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

    if let Ok((mut transform, mut visibility, mut tag, _, material)) = fast.single_mut() {
        visibility.set_if_neq(if fancy_mode {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
        transform
            .reborrow()
            .map_unchanged(|transform| &mut transform.translation)
            .set_if_neq(fast_cloud_anchor(
                player.translation.x,
                player.translation.z,
                cloud_y,
            ));
        if !fancy_mode {
            tag.set_if_neq(tint.clone());
            set_uv_offset(
                &mut materials,
                &material.0,
                fast_cloud_uv_offset(player.translation.x, player.translation.z, scroll),
            );
        }
    }

    if let Ok((mut transform, mut visibility, mut tag, _, material)) = fancy.single_mut() {
        visibility.set_if_neq(if fancy_mode {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        transform
            .reborrow()
            .map_unchanged(|transform| &mut transform.translation)
            .set_if_neq(fancy_place);
        if fancy_mode {
            tag.set_if_neq(tint);
            set_uv_offset(&mut materials, &material.0, fancy_uv);
        }
    }
}

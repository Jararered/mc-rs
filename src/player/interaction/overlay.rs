//! Hover outline and punching cracks, matching Beta `RenderGlobal`.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::gizmos::config::GizmoConfigStore;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::AppScreen;
use crate::physics::BlockHit;
use crate::physics::PhysicsSet;
use crate::rendering::textures::BlockMaterial;
use crate::rendering::textures::TerrainMaterial;
use crate::rendering::textures::atlas_tile_uvs;
use crate::world::chunk::WorldChunks;

use super::mining::destroy_stage;

/// Slight inflate so the outline and cracks sit outside the block, like Beta's
/// `expand(0.002)` / `glPolygonOffset(-3, -3)`.
const OVERLAY_EXPAND: f32 = 0.002;
/// `terrain.png` row of the ten destroy-stage tiles (`240..249`).
const DESTROY_TILE_Y: u8 = 15;
/// Beta's selection box: `glColor4f(0, 0, 0, 0.4)` with the default
/// `glLineWidth(2)`, drawn as `GL_LINE_STRIP`/`GL_LINES` around the block
/// rather than a filled wireframe.
const OUTLINE_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.4);

/// Render layer of the selection outline. Only the world camera draws it: on
/// the default layer the 2D UI camera drew it too, collapsing the 3D lines
/// into a stray dot above the crosshair.
pub const SELECTION_LAYER: usize = 5;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct SelectionGizmos;

/// What the crosshair is pointing at, plus punching progress for the overlay.
#[derive(Resource, Clone, Debug, Default)]
pub struct BlockFocus {
    pub hit: Option<BlockHit>,
    pub mining_damage: f32,
}

impl BlockFocus {
    pub fn destroy_stage(&self) -> Option<u8> {
        destroy_stage(self.mining_damage)
    }
}

#[derive(Resource)]
struct BlockOverlays {
    cracks: OverlayLayer,
    crack_stage: Option<u8>,
    crack_texture: bool,
}

struct OverlayLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

pub(crate) fn overlay_plugin(app: &mut App) {
    // The group's systems need `GizmoPlugin`'s assets, which a headless app
    // does not have.
    if app
        .world()
        .contains_resource::<Assets<bevy::gizmos::GizmoAsset>>()
    {
        app.init_gizmo_group::<SelectionGizmos>();
    }
    app.init_resource::<BlockFocus>()
        .add_systems(Startup, configure_selection_gizmos)
        .add_systems(PostStartup, spawn_block_overlays)
        .add_systems(
            Update,
            (
                sync_crack_texture,
                update_block_overlays,
                draw_selection_outline,
            )
                .after(PhysicsSet::ApplyInput)
                .run_if(in_state(AppScreen::Playing)),
        )
        .add_systems(OnExit(AppScreen::Playing), hide_block_overlays);
}

fn configure_selection_gizmos(store: Option<ResMut<GizmoConfigStore>>) {
    if let Some(mut store) = store {
        store.config_mut::<SelectionGizmos>().0.render_layers =
            RenderLayers::layer(SELECTION_LAYER);
    }
}

fn spawn_block_overlays(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(mut meshes) = meshes else {
        return;
    };
    let Some(mut materials) = materials else {
        return;
    };

    let crack_mesh = meshes.add(destroy_overlay_mesh(0));
    let crack_material = materials.add(StandardMaterial {
        // Beta's drawBlockBreaking disables tessellator vertex colors and
        // blends the destroy tile with `glBlendFunc(GL_DST_COLOR,
        // GL_SRC_COLOR)`, i.e. `2 * texel * dst`, doubling and multiplying
        // into the block's own color rather than laying a flat tint over it.
        // `AlphaMode::Multiply` gives a single `texel * dst`; the texture is
        // pre-doubled in `sync_crack_texture` to match Beta's factor of two.
        // It also does not apply BlockGrass.colorMultiplier().
        base_color: Color::WHITE,
        unlit: true,
        alpha_mode: AlphaMode::Multiply,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    let crack_entity = commands
        .spawn((
            Name::new("Break overlay"),
            Mesh3d(crack_mesh.clone()),
            MeshMaterial3d(crack_material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();

    commands.insert_resource(BlockOverlays {
        cracks: OverlayLayer {
            entity: crack_entity,
            mesh: crack_mesh,
            material: crack_material,
        },
        crack_stage: None,
        crack_texture: false,
    });
}

fn sync_crack_texture(
    overlays: Option<ResMut<BlockOverlays>>,
    terrain: Option<Res<TerrainMaterial>>,
    block_materials: Option<Res<Assets<BlockMaterial>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let Some(mut overlays) = overlays else {
        return;
    };
    if overlays.crack_texture {
        return;
    }
    let Some(mut materials) = materials else {
        return;
    };
    let Some(mut images) = images else {
        return;
    };
    let Some(terrain) = terrain else {
        return;
    };
    let Some(source_handle) = block_materials
        .as_deref()
        .and_then(|block_materials| block_materials.get(&terrain.0))
        .and_then(|material| material.base.base_color_texture.clone())
    else {
        return;
    };
    let Some(source) = images.get(&source_handle) else {
        return;
    };
    if source.data.is_none() {
        return;
    }
    let mut overlay_image = source.clone();
    if let Some(data) = overlay_image.data.as_mut() {
        double_crack_intensity(data);
    }
    let overlay_handle = images.add(overlay_image);
    let Some(mut material) = materials.get_mut(&overlays.cracks.material) else {
        return;
    };
    material.base_color_texture = Some(overlay_handle);
    overlays.crack_texture = true;
}

/// The targeted block's selection box, rotated for torches and ladders by
/// the metadata in the world.
fn selection_bounds(chunks: Option<&WorldChunks>, hit: BlockHit) -> ([f32; 3], [f32; 3]) {
    let metadata = chunks.map_or(0, |chunks| chunks.metadata_at(hit.x, hit.y, hit.z));
    hit.block.selection_bounds_for(metadata)
}

fn update_block_overlays(
    focus: Res<BlockFocus>,
    chunks: Option<Res<WorldChunks>>,
    overlays: Option<ResMut<BlockOverlays>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut views: Query<(&mut Transform, &mut Visibility)>,
) {
    let mut meshes = meshes;
    let Some(mut overlays) = overlays else {
        return;
    };

    let block_transform = focus.hit.map(|hit| {
        let (min, max) = selection_bounds(chunks.as_deref(), hit);
        let min = Vec3::from_array(min);
        let max = Vec3::from_array(max);
        Transform::from_translation(
            Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + (min + max) * 0.5,
        )
        .with_scale(max - min + Vec3::splat(2.0 * OVERLAY_EXPAND))
    });

    let stage = focus.destroy_stage();
    if let Ok((mut transform, mut visibility)) = views.get_mut(overlays.cracks.entity) {
        match (stage, block_transform) {
            (Some(stage), Some(at)) => {
                if overlays.crack_stage != Some(stage)
                    && let Some(meshes) = meshes.as_mut()
                    && let Some(mut mesh) = meshes.get_mut(&overlays.cracks.mesh)
                {
                    *mesh = destroy_overlay_mesh(stage);
                    overlays.crack_stage = Some(stage);
                }
                transform.set_if_neq(at);
                visibility.set_if_neq(Visibility::Inherited);
            }
            _ => {
                overlays.crack_stage = None;
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

fn hide_block_overlays(overlays: Option<Res<BlockOverlays>>, mut visible: Query<&mut Visibility>) {
    let Some(overlays) = overlays else {
        return;
    };
    if let Ok(mut visibility) = visible.get_mut(overlays.cracks.entity) {
        visibility.set_if_neq(Visibility::Hidden);
    }
}

/// Beta's `drawSelectionBox`/`drawOutlinedBoundingBox`: thin `GL_LINES` traced
/// around the block's edges, not a filled volume. Gizmo lines are the direct
/// Bevy equivalent, including the matching default 2px line width.
///
/// `Gizmos` needs `GizmoPlugin` (part of `DefaultPlugins`), which headless
/// tests built on `MinimalPlugins` don't add, so this stays optional.
fn draw_selection_outline(
    focus: Res<BlockFocus>,
    chunks: Option<Res<WorldChunks>>,
    gizmos: Option<Gizmos<SelectionGizmos>>,
) {
    let Some(mut gizmos) = gizmos else {
        return;
    };
    let Some(hit) = focus.hit else {
        return;
    };
    let (min, max) = selection_bounds(chunks.as_deref(), hit);
    let min = Vec3::from_array(min);
    let max = Vec3::from_array(max);
    let center = Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + (min + max) * 0.5;
    let size = max - min + Vec3::splat(2.0 * OVERLAY_EXPAND);
    gizmos.primitive_3d(
        &Cuboid::from_size(size),
        Isometry3d::from_translation(center),
        OUTLINE_COLOR,
    );
}

/// Beta's crack blend is `2 * texel * dst`; `AlphaMode::Multiply` only gives
/// `texel * dst`, so double each channel here (saturating) to match. White
/// background texels stay white (the identity color under multiply), and
/// near-white/grey mask texels wash out toward white just as they clamp to
/// full brightness under Beta's doubling blend.
pub fn double_crack_intensity(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        pixel[0] = pixel[0].saturating_add(pixel[0]);
        pixel[1] = pixel[1].saturating_add(pixel[1]);
        pixel[2] = pixel[2].saturating_add(pixel[2]);
    }
}

/// A unit cube centered at the origin, UVs sampling destroy-stage `stage`.
pub fn destroy_overlay_mesh(stage: u8) -> Mesh {
    let (u0, v0, u1, v1) = atlas_tile_uvs(stage.min(9), DESTROY_TILE_Y);
    let faces: [([f32; 3], [[f32; 3]; 4], [[f32; 2]; 4]); 6] = [
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
            [[u0, v0], [u0, v1], [u1, v1], [u1, v0]],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
            [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [-0.5, -0.5, 0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, -0.5, -0.5],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
    ];

    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners, face_uvs) in faces {
        let start = positions.len() as u32;
        for (corner, uv) in corners.into_iter().zip(face_uvs) {
            positions.push(corner);
            normals.push(normal);
            uvs.push(uv);
        }
        indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
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

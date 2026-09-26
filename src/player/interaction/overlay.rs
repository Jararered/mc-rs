//! Hover outline and punching cracks, matching Beta `RenderGlobal`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::AppScreen;
use crate::block::properties::selection_bounds;
use crate::physics::BlockHit;
use crate::physics::PhysicsSet;
use crate::world::textures::BlockMaterial;
use crate::world::textures::TerrainMaterial;
use crate::world::textures::atlas_tile_uvs;

use super::mining::destroy_stage;

/// Slight inflate so the outline and cracks sit outside the block, like Beta's
/// `expand(0.002)` / `glPolygonOffset(-3, -3)`.
const OVERLAY_EXPAND: f32 = 0.002;
/// World-space thickness of the hover box. Keep the filled frame close to the
/// one-pixel selection border used by the original game.
pub const OUTLINE_THICKNESS: f32 = 1.0 / 32.0;
/// `terrain.png` row of the ten destroy-stage tiles (`240..249`).
const DESTROY_TILE_Y: u8 = 15;
/// Destroy stages store empty texels as white with alpha 1. Treat those as air.
const OVERLAY_ALPHA_CUTOFF: u8 = 16;
/// The reference destroy-stage tiles also contain opaque pale-grey background
/// texels. They are part of the mask, not visible crack marks.
const OVERLAY_LIGHT_GREY_CUTOFF: u8 = 192;

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
    outline: OverlayLayer,
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
    app.init_resource::<BlockFocus>()
        .add_systems(PostStartup, spawn_block_overlays)
        .add_systems(
            Update,
            (sync_crack_texture, update_block_overlays)
                .after(PhysicsSet::ApplyInput)
                .run_if(in_state(AppScreen::Playing)),
        )
        .add_systems(OnExit(AppScreen::Playing), hide_block_overlays);
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

    let outline_mesh = meshes.add(selection_outline_mesh());
    let outline_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.4),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 0.5,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    let outline_entity = commands
        .spawn((
            Name::new("Block outline"),
            Mesh3d(outline_mesh.clone()),
            MeshMaterial3d(outline_material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ))
        .id();

    let crack_mesh = meshes.add(destroy_overlay_mesh(0));
    let crack_material = materials.add(StandardMaterial {
        // Beta's drawBlockBreaking disables tessellator vertex colors and
        // renders the grayscale destroy tile neutrally at 50% opacity. In
        // particular, it does not apply BlockGrass.colorMultiplier().
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 0.25,
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
        outline: OverlayLayer {
            entity: outline_entity,
            mesh: outline_mesh,
            material: outline_material,
        },
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
        punch_nearly_transparent_texels(data);
    }
    let overlay_handle = images.add(overlay_image);
    let Some(mut material) = materials.get_mut(&overlays.cracks.material) else {
        return;
    };
    material.base_color_texture = Some(overlay_handle);
    overlays.crack_texture = true;
}

fn update_block_overlays(
    focus: Res<BlockFocus>,
    overlays: Option<ResMut<BlockOverlays>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    mut views: Query<(&mut Transform, &mut Visibility)>,
) {
    let mut meshes = meshes;
    let Some(mut overlays) = overlays else {
        return;
    };

    let block_transform = focus.hit.map(|hit| {
        let (min, max) = selection_bounds(hit.block);
        let min = Vec3::from_array(min);
        let max = Vec3::from_array(max);
        Transform::from_translation(
            Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32) + (min + max) * 0.5,
        )
        .with_scale(max - min + Vec3::splat(2.0 * OVERLAY_EXPAND))
    });

    if let Ok((mut transform, mut visibility)) = views.get_mut(overlays.outline.entity) {
        if let Some(at) = block_transform {
            *transform = at;
            *visibility = Visibility::Inherited;
        } else {
            *visibility = Visibility::Hidden;
        }
    }

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
                *transform = at;
                *visibility = Visibility::Inherited;
            }
            _ => {
                overlays.crack_stage = None;
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn hide_block_overlays(overlays: Option<Res<BlockOverlays>>, mut visible: Query<&mut Visibility>) {
    let Some(overlays) = overlays else {
        return;
    };
    for entity in [overlays.outline.entity, overlays.cracks.entity] {
        if let Ok(mut visibility) = visible.get_mut(entity) {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Empty destroy-stage texels are white or pale grey in `terrain.png`, sometimes
/// with opaque alpha. Force those to zero so the overlay only draws the cracks.
pub fn punch_nearly_transparent_texels(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let max_channel = pixel[0].max(pixel[1]).max(pixel[2]);
        let min_channel = pixel[0].min(pixel[1]).min(pixel[2]);
        let light_grey = max_channel >= OVERLAY_LIGHT_GREY_CUTOFF
            && max_channel.saturating_sub(min_channel) <= 8;
        if pixel[3] < OVERLAY_ALPHA_CUTOFF || light_grey {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = 0;
        }
    }
}

/// Filled frame of the 12 block edges, matching `drawOutlinedBoundingBox`.
pub fn selection_outline_mesh() -> Mesh {
    let h = 0.5;
    let t = OUTLINE_THICKNESS * 0.5;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    let mut push_box = |min: Vec3, max: Vec3| {
        let corners = [
            [min.x, min.y, min.z],
            [max.x, min.y, min.z],
            [max.x, min.y, max.z],
            [min.x, min.y, max.z],
            [min.x, max.y, min.z],
            [max.x, max.y, min.z],
            [max.x, max.y, max.z],
            [min.x, max.y, max.z],
        ];
        let faces: [([f32; 3], [usize; 4]); 6] = [
            ([0.0, 1.0, 0.0], [4, 7, 6, 5]),
            ([0.0, -1.0, 0.0], [0, 1, 2, 3]),
            ([1.0, 0.0, 0.0], [1, 5, 6, 2]),
            ([-1.0, 0.0, 0.0], [0, 3, 7, 4]),
            ([0.0, 0.0, 1.0], [3, 2, 6, 7]),
            ([0.0, 0.0, -1.0], [0, 4, 5, 1]),
        ];
        for (normal, verts) in faces {
            let start = positions.len() as u32;
            for index in verts {
                positions.push(corners[index]);
                normals.push(normal);
            }
            indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
        }
    };

    // 12 edges of the unit cube, each a small AABB so the stroke has width.
    for z in [-h, h] {
        for y in [-h, h] {
            push_box(
                Vec3::new(-h - t, y - t, z - t),
                Vec3::new(h + t, y + t, z + t),
            );
        }
    }
    for x in [-h, h] {
        for z in [-h, h] {
            push_box(
                Vec3::new(x - t, -h - t, z - t),
                Vec3::new(x + t, h + t, z + t),
            );
        }
    }
    for x in [-h, h] {
        for y in [-h, h] {
            push_box(
                Vec3::new(x - t, y - t, -h - t),
                Vec3::new(x + t, y + t, h + t),
            );
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_indices(Indices::U32(indices))
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

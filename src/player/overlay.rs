//! Hover outline and punching cracks, matching Beta `RenderGlobal`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::state::AppScreen;
use crate::physics::BlockHit;
use crate::physics::PhysicsSet;
use crate::world::textures::TerrainMaterial;
use crate::world::textures::atlas_tile_uvs;

use super::mining::destroy_stage;

/// Slight inflate so the outline and cracks sit outside the block, like Beta's
/// `expand(0.002)` / `glPolygonOffset(-3, -3)`.
const OVERLAY_EXPAND: f32 = 0.002;
/// `terrain.png` row of the ten destroy-stage tiles (`240..249`).
const DESTROY_TILE_Y: u8 = 15;

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
}

struct OverlayLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

pub(super) fn overlay_plugin(app: &mut App) {
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
        unlit: true,
        alpha_mode: AlphaMode::Mask(0.1),
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
    });
}

fn sync_crack_texture(
    overlays: Option<Res<BlockOverlays>>,
    terrain: Option<Res<TerrainMaterial>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    let Some(overlays) = overlays else {
        return;
    };
    let Some(terrain) = terrain else {
        return;
    };
    let Some(texture) = materials
        .get(&terrain.0)
        .and_then(|material| material.base_color_texture.clone())
    else {
        return;
    };
    let Some(mut material) = materials.get_mut(&overlays.cracks.material) else {
        return;
    };
    if material.base_color_texture.as_ref() != Some(&texture) {
        material.base_color_texture = Some(texture);
    }
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
        Transform::from_translation(Vec3::new(
            hit.x as f32 + 0.5,
            hit.y as f32 + 0.5,
            hit.z as f32 + 0.5,
        ))
        .with_scale(Vec3::splat(1.0 + 2.0 * OVERLAY_EXPAND))
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

/// Wire cube of the 12 block edges, matching `drawOutlinedBoundingBox`.
pub fn selection_outline_mesh() -> Mesh {
    let min = -0.5;
    let max = 0.5;
    let corners = [
        [min, min, min],
        [max, min, min],
        [max, min, max],
        [min, min, max],
        [min, max, min],
        [max, max, min],
        [max, max, max],
        [min, max, max],
    ];
    let edges: [[u32; 2]; 12] = [
        [0, 1],
        [1, 2],
        [2, 3],
        [3, 0],
        [4, 5],
        [5, 6],
        [6, 7],
        [7, 4],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ];
    let mut positions = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(24);
    for [a, b] in edges {
        let start = positions.len() as u32;
        positions.push(corners[a as usize]);
        positions.push(corners[b as usize]);
        indices.extend_from_slice(&[start, start + 1]);
    }
    Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
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

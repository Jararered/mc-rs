use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoCpuCulling;
use bevy::prelude::*;

use crate::rendering::meshing::ChunkMeshes;
use crate::rendering::textures::BlockMaterial;
use crate::rendering::textures::LEAF_WIGGLE_AMPLITUDE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::chunk::SECTIONS_PER_CHUNK;

/// Layers of a section in [`ChunkMeshes::into_layers`] order.
const LAYER_COUNT: usize = 5;
const LAYER_NAMES: [&str; LAYER_COUNT] = [
    "Opaque",
    "Grass overlay",
    "Cutout",
    "Water",
    "Alpha-masked geometry",
];
const CUTOUT_LAYER: usize = 2;

/// How section layers are frustum culled.
///
/// Bevy tests every mesh entity against each view's frustum in the main
/// world, for the camera and every shadow cascade, and its GPU preprocessing
/// pass then culls again. `Gpu` marks layers [`NoCpuCulling`] so only the GPU
/// pass runs.
///
/// That pays where the driver has `MULTI_DRAW_INDIRECT_COUNT`. Without it
/// (Metal) Bevy still issues one indirect draw per GPU-culled layer in every
/// view, with zero instances, so the main-world test stays on there.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChunkCulling {
    #[default]
    Cpu,
    Gpu,
}

impl ChunkCulling {
    /// `forced` is `gpu` or `cpu` to compare the two on one machine; any
    /// other value leaves the choice to the driver's features.
    pub fn choose(multi_draw_indirect_count: bool, forced: Option<&str>) -> Self {
        match forced {
            Some("gpu") => Self::Gpu,
            Some("cpu") => Self::Cpu,
            _ if multi_draw_indirect_count => Self::Gpu,
            _ => Self::Cpu,
        }
    }
}

/// Handles for each layer's material, in layer order.
#[derive(Clone)]
pub(super) struct ChunkMaterials(pub [Handle<BlockMaterial>; LAYER_COUNT]);

/// One uploaded layer mesh and the bounds computed before its vertex data
/// left the main world.
pub(super) struct LayerMesh {
    mesh: Mesh,
    aabb: Aabb,
}

/// Rebuilt layers for one section, produced on the compute pool.
pub(super) struct SectionMeshes {
    index: usize,
    layers: [Option<LayerMesh>; LAYER_COUNT],
}

impl SectionMeshes {
    pub(super) fn build(index: usize, meshes: ChunkMeshes) -> Self {
        let mut layer_index = 0;
        let layers = meshes.into_layers().map(|geometry| {
            // Leaf vertices move in the shader; keep them inside the culling box.
            let padding = if layer_index == CUTOUT_LAYER {
                LEAF_WIGGLE_AMPLITUDE * 1.5
            } else {
                0.0
            };
            layer_index += 1;
            let aabb = geometry.aabb(padding)?;
            Some(LayerMesh {
                mesh: geometry.into_mesh(),
                aabb,
            })
        });
        Self { index, layers }
    }
}

pub(super) struct RenderedChunk {
    entity: Entity,
    sections: [[Option<MeshLayer>; LAYER_COUNT]; SECTIONS_PER_CHUNK],
    /// Light fingerprints of the meshes currently shown, one per section.
    pub(super) fingerprints: [u64; SECTIONS_PER_CHUNK],
}

struct MeshLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
    bytes: usize,
}

impl RenderedChunk {
    /// GPU vertex and index bytes of every layer in this chunk.
    pub(super) fn mesh_bytes(&self) -> usize {
        self.sections
            .iter()
            .flatten()
            .flatten()
            .map(|layer| layer.bytes)
            .sum()
    }

    pub(super) fn layer_count(&self) -> usize {
        self.sections.iter().flatten().flatten().count()
    }
}

pub(super) fn spawn_chunk(commands: &mut Commands, position: ChunkPosition) -> RenderedChunk {
    let (x, z) = position.world_origin();
    let entity = commands
        .spawn((
            Name::new(format!("Chunk {}, {}", position.x, position.z)),
            position,
            Transform::from_xyz(x, 0.0, z),
            Visibility::default(),
        ))
        .id();
    RenderedChunk {
        entity,
        sections: Default::default(),
        fingerprints: [0; SECTIONS_PER_CHUNK],
    }
}

pub(super) fn apply_sections(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    rendered: &mut RenderedChunk,
    sections: Vec<SectionMeshes>,
    materials: &ChunkMaterials,
    culling: ChunkCulling,
) {
    for section in sections {
        for (layer_index, mesh) in section.layers.into_iter().enumerate() {
            apply_layer(
                commands,
                meshes,
                rendered.entity,
                &mut rendered.sections[section.index][layer_index],
                mesh,
                &materials.0[layer_index],
                LAYER_NAMES[layer_index],
                section.index,
                culling,
            );
        }
    }
}

/// Bevy 0.19's mesh allocator logs a use-after-free error if an empty mesh is
/// spawned or uploaded. Skip those layers until they have faces.
fn apply_layer(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    layer: &mut Option<MeshLayer>,
    mesh: Option<LayerMesh>,
    material: &Handle<BlockMaterial>,
    name: &'static str,
    section: usize,
    culling: ChunkCulling,
) {
    match (layer.take(), mesh) {
        (Some(mut existing), Some(built)) => {
            existing.bytes = mesh_bytes(&built.mesh);
            if let Some(mut current) = meshes.get_mut(existing.mesh.id()) {
                *current = built.mesh;
            }
            commands.entity(existing.entity).insert(built.aabb);
            *layer = Some(existing);
        }
        (Some(existing), None) => {
            commands.entity(existing.entity).despawn();
            meshes.remove(existing.mesh.id());
        }
        (None, Some(built)) => {
            let bytes = mesh_bytes(&built.mesh);
            let handle = meshes.add(built.mesh);
            let mut spawned = commands.spawn((
                Name::new(name),
                Mesh3d(handle.clone()),
                MeshMaterial3d(material.clone()),
                built.aabb,
                NoAutoAabb,
                Transform::from_xyz(0.0, (section * SECTION_HEIGHT) as f32, 0.0),
                ChildOf(parent),
            ));
            if culling == ChunkCulling::Gpu {
                // The GPU pass culls against the same `Aabb`.
                spawned.insert(NoCpuCulling);
            }
            let entity = spawned.id();
            *layer = Some(MeshLayer {
                entity,
                mesh: handle,
                bytes,
            });
        }
        (None, None) => {}
    }
}

fn mesh_bytes(mesh: &Mesh) -> usize {
    mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)
}

pub(super) fn despawn_rendered_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    rendered: RenderedChunk,
) {
    commands.entity(rendered.entity).despawn();
    for layer in rendered.sections.into_iter().flatten().flatten() {
        meshes.remove(layer.mesh.id());
    }
}

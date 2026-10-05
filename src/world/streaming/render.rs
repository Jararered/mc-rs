use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoCpuCulling;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::rendering::chunk_quads::ChunkQuads;
use crate::rendering::chunk_quads::QuadRange;
use crate::rendering::meshing::ChunkMeshes;
use crate::rendering::meshing::layer_bytes;
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

/// One built layer and the bounds computed before its geometry left the job.
pub(super) struct LayerMesh {
    geometry: LayerGeometry,
    aabb: Aabb,
}

/// A layer as the mesh job hands it over.
enum LayerGeometry {
    /// Quad records for the shared buffer (`rendering::chunk_quads`), header
    /// first, and how many quads follow it.
    Quads { bytes: Vec<u8>, quads: u32 },
    /// A mesh in the packed vertex format, for apps without the quad buffer.
    Mesh(Mesh),
}

/// Where layer geometry is kept: quad records with a renderer, meshes always.
pub(super) struct LayerStore<'a> {
    pub(super) meshes: &'a mut Assets<Mesh>,
    pub(super) quads: Option<&'a mut ChunkQuads>,
}

impl LayerStore<'_> {
    fn release(&mut self, drawn: LayerDrawn) {
        match drawn {
            LayerDrawn::Quads(range) => {
                if let Some(quads) = self.quads.as_deref_mut() {
                    quads.free(range);
                }
            }
            LayerDrawn::Mesh(mesh) => {
                self.meshes.remove(mesh.id());
            }
        }
    }
}

/// Rebuilt layers for one section, produced on the compute pool.
pub(super) struct SectionMeshes {
    index: usize,
    layers: [Option<LayerMesh>; LAYER_COUNT],
}

impl SectionMeshes {
    /// `quads` packs each layer as quad records instead of a mesh.
    pub(super) fn build(index: usize, meshes: ChunkMeshes, quads: bool) -> Self {
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
            let geometry = if quads {
                let records = geometry.into_quads();
                LayerGeometry::Quads {
                    quads: u32::try_from(records.len()).expect("layer quad count"),
                    bytes: layer_bytes(&records),
                }
            } else {
                LayerGeometry::Mesh(geometry.into_mesh())
            };
            Some(LayerMesh { geometry, aabb })
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
    drawn: LayerDrawn,
    bytes: usize,
}

/// What a layer entity draws from.
enum LayerDrawn {
    /// Its range of the quad buffer; the entity's mesh is a shared proxy.
    Quads(QuadRange),
    Mesh(Handle<Mesh>),
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

/// Returns false when a layer could not be stored as quad records, which
/// leaves that layer undrawn until the chunk is meshed again.
pub(super) fn apply_sections(
    commands: &mut Commands,
    store: &mut LayerStore,
    rendered: &mut RenderedChunk,
    sections: Vec<SectionMeshes>,
    materials: &ChunkMaterials,
    culling: ChunkCulling,
) -> bool {
    let mut stored = true;
    for section in sections {
        for (layer_index, mesh) in section.layers.into_iter().enumerate() {
            stored &= apply_layer(
                commands,
                store,
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
    stored
}

/// Bevy 0.19's mesh allocator logs a use-after-free error if an empty mesh is
/// spawned or uploaded. Skip those layers until they have faces.
fn apply_layer(
    commands: &mut Commands,
    store: &mut LayerStore,
    parent: Entity,
    layer: &mut Option<MeshLayer>,
    mesh: Option<LayerMesh>,
    material: &Handle<BlockMaterial>,
    name: &'static str,
    section: usize,
    culling: ChunkCulling,
) -> bool {
    let (existing, built) = (layer.take(), mesh);
    let Some(built) = built else {
        if let Some(existing) = existing {
            commands.entity(existing.entity).despawn();
            store.release(existing.drawn);
        }
        return true;
    };
    // A remesh in the vertex format replaces the asset at the same handle.
    let reused = match (&existing, &built.geometry) {
        (Some(existing), LayerGeometry::Mesh(_)) => match &existing.drawn {
            LayerDrawn::Mesh(handle) => Some(handle.clone()),
            LayerDrawn::Quads(_) => None,
        },
        _ => None,
    };
    let entity = existing.map(|existing| {
        if reused.is_none() {
            // Freed first, so the new records can take the same range.
            store.release(existing.drawn);
        }
        existing.entity
    });
    let (drawn, bytes, handle, tag) = match built.geometry {
        LayerGeometry::Mesh(mesh) => {
            let bytes = mesh_bytes(&mesh);
            let handle = match reused {
                Some(handle) => {
                    if let Some(mut current) = store.meshes.get_mut(handle.id()) {
                        *current = mesh;
                    }
                    handle
                }
                None => store.meshes.add(mesh),
            };
            (LayerDrawn::Mesh(handle.clone()), bytes, handle, None)
        }
        LayerGeometry::Quads { bytes, quads } => {
            let uploaded = store.quads.as_deref_mut().and_then(|store| {
                let range = store.upload(bytes)?;
                Some((store, range))
            });
            let Some((quad_store, range)) = uploaded else {
                if let Some(entity) = entity {
                    commands.entity(entity).despawn();
                }
                return false;
            };
            let handle = quad_store.proxy(store.meshes, quads);
            let (bytes, tag) = (range.bytes(), range.tag());
            (LayerDrawn::Quads(range), bytes, handle, Some(tag))
        }
    };
    let entity = match entity {
        Some(entity) => {
            let mut layer = commands.entity(entity);
            layer.insert((Mesh3d(handle), built.aabb));
            match tag {
                Some(tag) => layer.insert(MeshTag(tag)),
                None => layer.remove::<MeshTag>(),
            };
            entity
        }
        None => {
            let mut spawned = commands.spawn((
                Name::new(name),
                Mesh3d(handle),
                MeshMaterial3d(material.clone()),
                built.aabb,
                NoAutoAabb,
                Transform::from_xyz(0.0, (section * SECTION_HEIGHT) as f32, 0.0),
                ChildOf(parent),
            ));
            if let Some(tag) = tag {
                spawned.insert(MeshTag(tag));
            }
            if culling == ChunkCulling::Gpu {
                // The GPU pass culls against the same `Aabb`.
                spawned.insert(NoCpuCulling);
            }
            spawned.id()
        }
    };
    *layer = Some(MeshLayer {
        entity,
        drawn,
        bytes,
    });
    true
}

fn mesh_bytes(mesh: &Mesh) -> usize {
    mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)
}

pub(super) fn despawn_rendered_chunk(
    commands: &mut Commands,
    store: &mut LayerStore,
    rendered: RenderedChunk,
) {
    commands.entity(rendered.entity).despawn();
    for layer in rendered.sections.into_iter().flatten().flatten() {
        store.release(layer.drawn);
    }
}

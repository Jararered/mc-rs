use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoCpuCulling;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use crate::rendering::chunk_quads::ChunkQuads;
use crate::rendering::chunk_quads::QuadRange;
use crate::rendering::chunk_regions::SectionLayer;
use crate::rendering::meshing::ChunkMeshes;
use crate::rendering::meshing::FACINGS;
use crate::rendering::meshing::QuadRecord;
use crate::rendering::meshing::UNASSIGNED_FACING;
use crate::rendering::meshing::group_by_facing;
use crate::rendering::meshing::layer_bytes;
use crate::rendering::occlusion::SectionVisibility;
use crate::rendering::occlusion::VisibleMask;
use crate::rendering::textures::BlockMaterial;
use crate::rendering::textures::LEAF_WIGGLE_AMPLITUDE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::chunk::SECTIONS_PER_CHUNK;

/// Layers of a section in [`ChunkMeshes::into_layers`] order.
pub(super) const LAYER_COUNT: usize = 5;
pub(super) const LAYER_NAMES: [&str; LAYER_COUNT] = [
    "Opaque",
    "Grass overlay",
    "Cutout",
    "Water",
    "Alpha-masked geometry",
];
const CUTOUT_LAYER: usize = 2;
/// Blended and drawn from both sides, so its quads are never grouped by
/// facing or culled by it.
pub(super) const WATER_LAYER: usize = 3;

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
    Quads {
        bytes: Vec<u8>,
        quads: u32,
        facings: [u32; FACINGS],
        /// The records of a blended layer, kept to sort for the camera.
        translucent: Option<Vec<QuadRecord>>,
    },
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
    visibility: SectionVisibility,
}

impl SectionMeshes {
    /// `quads` packs each layer as quad records instead of a mesh.
    pub(super) fn build(
        index: usize,
        meshes: ChunkMeshes,
        quads: bool,
        visibility: SectionVisibility,
    ) -> Self {
        let mut layer_index = 0;
        let layers = meshes.into_layers().map(|geometry| {
            // Leaf vertices move in the shader; keep them inside the culling box.
            let padding = if layer_index == CUTOUT_LAYER {
                LEAF_WIGGLE_AMPLITUDE * 1.5
            } else {
                0.0
            };
            let layer = layer_index;
            layer_index += 1;
            let aabb = geometry.aabb(padding)?;
            let geometry = if quads {
                let mut records = geometry.into_quads();
                let quads = u32::try_from(records.len()).expect("layer quad count");
                let facings = if layer == WATER_LAYER {
                    let mut facings = [0; FACINGS];
                    facings[UNASSIGNED_FACING] = quads;
                    facings
                } else {
                    group_by_facing(&mut records)
                };
                LayerGeometry::Quads {
                    quads,
                    bytes: layer_bytes(&records, &facings),
                    facings,
                    translucent: (layer == WATER_LAYER && records.len() > 1).then_some(records),
                }
            } else {
                LayerGeometry::Mesh(geometry.into_mesh())
            };
            Some(LayerMesh { geometry, aabb })
        });
        Self {
            index,
            layers,
            visibility,
        }
    }
}

pub(super) struct RenderedChunk {
    entity: Entity,
    sections: [[Option<MeshLayer>; LAYER_COUNT]; SECTIONS_PER_CHUNK],
    /// Light fingerprints of the meshes currently shown, one per section.
    pub(super) fingerprints: [u64; SECTIONS_PER_CHUNK],
    /// Which faces of each section see each other, for cave culling.
    pub(super) visibility: [SectionVisibility; SECTIONS_PER_CHUNK],
    /// Sections whose layer entities are hidden by cave culling.
    hidden: VisibleMask,
}

struct MeshLayer {
    /// `None` for quad records a chunk region draws.
    entity: Option<Entity>,
    drawn: LayerDrawn,
    bytes: usize,
    /// Where a region finds the records, for quad layers.
    region: Option<SectionLayer>,
    /// A blended layer's records, re-sorted as the camera moves.
    pub(super) translucent: Option<Translucent>,
}

pub(super) struct Translucent {
    pub(super) records: Vec<QuadRecord>,
    pub(super) facings: [u32; FACINGS],
    /// The camera block the records were last sorted for.
    pub(super) sorted_for: Option<IVec3>,
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

    /// Every translucent layer, with its section and its records' range.
    pub(super) fn translucent_layers(
        &mut self,
    ) -> impl Iterator<Item = (usize, &QuadRange, &mut Translucent)> {
        self.sections
            .iter_mut()
            .enumerate()
            .flat_map(|(section, layers)| {
                layers.iter_mut().flatten().filter_map(move |layer| {
                    let LayerDrawn::Quads(range) = &layer.drawn else {
                        return None;
                    };
                    Some((section, range, layer.translucent.as_mut()?))
                })
            })
    }

    /// The quad layers of `section` that a chunk region draws, by layer
    /// index.
    pub(super) fn region_layers(
        &self,
        section: usize,
    ) -> impl Iterator<Item = (usize, &SectionLayer)> {
        self.sections[section]
            .iter()
            .enumerate()
            .filter_map(|(layer, mesh)| Some((layer, mesh.as_ref()?.region.as_ref()?)))
    }

    /// Show only the sections in `visible`. Returns the layer entities whose
    /// visibility changed, and whether each is now shown.
    pub(super) fn cull(&mut self, visible: VisibleMask) -> impl Iterator<Item = (Entity, bool)> {
        let hidden = !visible;
        let changed = self.hidden ^ hidden;
        self.hidden = hidden;
        self.sections
            .iter()
            .enumerate()
            .filter(move |(section, _)| changed >> section & 1 != 0)
            .flat_map(move |(section, layers)| {
                let shown = visible >> section & 1 != 0;
                layers
                    .iter()
                    .flatten()
                    .filter_map(move |layer| Some((layer.entity?, shown)))
            })
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
        visibility: [SectionVisibility::ALL; SECTIONS_PER_CHUNK],
        hidden: 0,
    }
}

/// Returns false when a layer could not be stored as quad records, which
/// leaves that layer undrawn until the chunk is meshed again. Sets
/// `graph_changed` when a section's visibility graph changed.
pub(super) fn apply_sections(
    commands: &mut Commands,
    store: &mut LayerStore,
    rendered: &mut RenderedChunk,
    sections: Vec<SectionMeshes>,
    materials: &ChunkMaterials,
    culling: ChunkCulling,
    regions: bool,
    graph_changed: &mut bool,
) -> bool {
    let mut stored = true;
    for section in sections {
        if rendered.visibility[section.index] != section.visibility {
            rendered.visibility[section.index] = section.visibility;
            *graph_changed = true;
        }
        let hidden = rendered.hidden >> section.index & 1 != 0;
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
                LayerOptions {
                    culling,
                    regions,
                    hidden,
                },
            );
        }
    }
    stored
}

/// Bevy 0.19's mesh allocator logs a use-after-free error if an empty mesh is
/// spawned or uploaded. Skip those layers until they have faces.
///
/// With `options.regions`, quad records get no entity: a chunk region draws
/// them (`super::regions`).
fn apply_layer(
    commands: &mut Commands,
    store: &mut LayerStore,
    parent: Entity,
    layer: &mut Option<MeshLayer>,
    mesh: Option<LayerMesh>,
    material: &Handle<BlockMaterial>,
    name: &'static str,
    section: usize,
    options: LayerOptions,
) -> bool {
    let (existing, built) = (layer.take(), mesh);
    let Some(built) = built else {
        if let Some(existing) = existing {
            if let Some(entity) = existing.entity {
                commands.entity(entity).despawn();
            }
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
    let mut entity = existing.and_then(|existing| {
        if reused.is_none() {
            // Freed first, so the new records can take the same range.
            store.release(existing.drawn);
        }
        existing.entity
    });
    let mut translucent_records = None;
    let (drawn, bytes, handle, tag, region) = match built.geometry {
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
            (
                LayerDrawn::Mesh(handle.clone()),
                bytes,
                Some(handle),
                None,
                None,
            )
        }
        LayerGeometry::Quads {
            bytes,
            quads,
            facings,
            translucent,
        } => {
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
            let region = SectionLayer {
                header: range.tag(),
                facings,
                aabb: built.aabb,
            };
            let (bytes, tag) = (range.bytes(), range.tag());
            translucent_records = translucent.map(|records| Translucent {
                records,
                facings,
                sorted_for: None,
            });
            if options.regions {
                if let Some(entity) = entity.take() {
                    commands.entity(entity).despawn();
                }
                (LayerDrawn::Quads(range), bytes, None, None, Some(region))
            } else {
                let handle = quad_store.proxy(store.meshes, quads);
                (
                    LayerDrawn::Quads(range),
                    bytes,
                    Some(handle),
                    Some(tag),
                    None,
                )
            }
        }
    };
    let entity = handle.map(|handle| match entity {
        Some(entity) => {
            let mut layer = commands.entity(entity);
            layer.insert((Mesh3d(handle), built.aabb));
            match tag {
                Some(tag) => layer.insert(MeshTag::new(tag)),
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
                if options.hidden {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                },
                ChildOf(parent),
            ));
            if let Some(tag) = tag {
                spawned.insert(MeshTag::new(tag));
            }
            if options.culling == ChunkCulling::Gpu {
                // The GPU pass culls against the same `Aabb`.
                spawned.insert(NoCpuCulling);
            }
            spawned.id()
        }
    });
    *layer = Some(MeshLayer {
        entity,
        drawn,
        bytes,
        region,
        translucent: translucent_records,
    });
    true
}

/// How layers of a chunk are drawn.
#[derive(Clone, Copy)]
pub(super) struct LayerOptions {
    pub(super) culling: ChunkCulling,
    /// Quad layers are drawn by chunk regions, not entities of their own.
    pub(super) regions: bool,
    /// Cave culling hides the section.
    pub(super) hidden: bool,
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

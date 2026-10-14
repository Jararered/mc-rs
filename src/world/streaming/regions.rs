//! The chunk regions streaming draws its quad layers through
//! ([`crate::rendering::chunk_regions`]): one entity per layer for each 8×8
//! chunk columns, and each one's draw list in the quad buffer.

use std::collections::HashMap;
use std::time::Instant;

use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoCpuCulling;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use super::StreamingDiagnostics;
use super::WorldStreaming;
use super::render::ChunkCulling;
use super::render::ChunkMaterials;
use super::render::LAYER_COUNT;
use super::render::LAYER_NAMES;
use super::render::WATER_LAYER;
use crate::player::PlayerCamera;
use crate::rendering::chunk_quads::ChunkQuadWrites;
use crate::rendering::chunk_quads::ChunkQuads;
use crate::rendering::chunk_quads::QuadRange;
use crate::rendering::chunk_regions::DrawList;
use crate::rendering::chunk_regions::region_aabb;
use crate::rendering::chunk_regions::region_of;
use crate::rendering::chunk_regions::region_origin;
use crate::rendering::chunk_regions::section_offset;
use crate::rendering::chunk_regions::section_origin;
use crate::rendering::chunk_regions::visible_facings;
use crate::rendering::meshing::FACINGS;
use crate::rendering::occlusion::VisibleSections;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::chunk::SECTIONS_PER_CHUNK;

/// Fewest entries a draw list's range holds, so small lists do not move
/// every time they grow.
const MIN_ENTRIES: usize = 16;
/// Every facing group, for water, which is drawn from both sides.
const ALL_FACINGS: u8 = (1 << FACINGS) - 1;

#[derive(Default)]
pub(super) struct ChunkRegions {
    regions: HashMap<IVec2, Region>,
    /// Sections the last build ordered, nearest first.
    order: Vec<(f32, ChunkPosition, usize)>,
    /// What the lists were last built for: `WorldStreaming::layer_revision`
    /// and the camera's clip-from-world matrix.
    built_for: Option<(u64, Mat4)>,
    stats: RegionStats,
}

/// What the last build drew.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegionStats {
    /// Region layer entities drawn.
    pub draws: usize,
    /// Runs of records over every draw list.
    pub entries: usize,
    /// Quads drawn, after cave, frustum, and facing culling.
    pub quads: u64,
}

#[derive(Default)]
struct Region {
    layers: [Option<RegionLayer>; LAYER_COUNT],
    lists: [DrawList; LAYER_COUNT],
    /// A rendered chunk lies in it.
    occupied: bool,
}

struct RegionLayer {
    entity: Entity,
    /// The draw list's records.
    range: QuadRange,
    /// Entries `range` holds.
    capacity: usize,
    /// Quads the proxy mesh holds.
    proxy: u32,
    /// The list last written to `range`.
    written: DrawList,
    shown: bool,
}

impl ChunkRegions {
    pub(super) fn stats(&self) -> RegionStats {
        self.stats
    }

    /// Despawn every region and free its draw lists.
    pub(super) fn clear(&mut self, commands: &mut Commands, mut quads: Option<&mut ChunkQuads>) {
        for (_, region) in self.regions.drain() {
            for layer in region.layers.into_iter().flatten() {
                commands.entity(layer.entity).despawn();
                if let Some(quads) = quads.as_deref_mut() {
                    quads.free(layer.range);
                }
            }
        }
        self.stats = RegionStats::default();
        self.built_for = None;
    }
}

/// Rebuild each region's draw lists for the camera, and keep one entity per
/// nonempty region layer.
pub(crate) fn draw_chunk_regions(
    mut commands: Commands,
    mut streaming: ResMut<WorldStreaming>,
    visible: Res<VisibleSections>,
    camera: Query<(&GlobalTransform, &Projection), With<PlayerCamera>>,
    quads: Option<ResMut<ChunkQuads>>,
    mut meshes: ResMut<Assets<Mesh>>,
    perf: Option<ResMut<StreamingDiagnostics>>,
) {
    let Some(mut quads) = quads else {
        return;
    };
    let Ok((transform, projection)) = camera.single() else {
        return;
    };
    let WorldStreaming {
        rendered,
        regions,
        materials,
        culling,
        layer_revision,
        ..
    } = streaming.bypass_change_detection();
    let Some(regions) = regions.as_mut() else {
        return;
    };
    // Bevy touches the projection every frame, so compare the matrix.
    let view = projection.get_clip_from_view() * transform.affine().inverse();
    if regions.built_for == Some((*layer_revision, view)) && !visible.is_changed() {
        return;
    }
    regions.built_for = Some((*layer_revision, view));
    let start = Instant::now();
    let eye = transform.translation();
    let frustum = projection.compute_frustum(transform);

    regions.order.clear();
    for region in regions.regions.values_mut() {
        region.occupied = false;
        region.lists.iter_mut().for_each(DrawList::clear);
    }
    for &position in rendered.keys() {
        regions
            .regions
            .entry(region_of(position))
            .or_default()
            .occupied = true;
        let mask = visible.mask(position);
        for section in 0..SECTIONS_PER_CHUNK {
            if mask >> section & 1 != 0 {
                let centre = section_origin(position, section) + SECTION_HEIGHT as f32 / 2.0;
                regions
                    .order
                    .push((centre.distance_squared(eye), position, section));
            }
        }
    }
    regions.order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    // Opaque layers nearest first, so the depth test rejects more; water
    // farthest first, so it blends over what is behind it.
    let order = std::mem::take(&mut regions.order);
    for water in [false, true] {
        for nth in 0..order.len() {
            let (_, position, section) = order[if water { order.len() - 1 - nth } else { nth }];
            let Some(chunk) = rendered.get(&position) else {
                continue;
            };
            let region = regions
                .regions
                .get_mut(&region_of(position))
                .expect("every rendered chunk's region was added");
            for (layer, drawn) in chunk.region_layers(section) {
                if (layer == WATER_LAYER) != water || !drawn.in_frustum(&frustum, position, section)
                {
                    continue;
                }
                let facings = if water {
                    ALL_FACINGS
                } else {
                    let (min, max) = drawn.world_bounds(position, section);
                    visible_facings(eye, min, max)
                };
                region.lists[layer].push(drawn, facings, section_offset(position, section));
            }
        }
    }
    regions.order = order;

    let mut stats = RegionStats::default();
    regions.regions.retain(|_, region| {
        if region.occupied {
            return true;
        }
        for layer in region.layers.iter_mut().filter_map(Option::take) {
            commands.entity(layer.entity).despawn();
            quads.free(layer.range);
        }
        false
    });
    for (&key, region) in &mut regions.regions {
        for (index, list) in region.lists.iter().enumerate() {
            let slot = &mut region.layers[index];
            if list.is_empty() {
                if let Some(layer) = slot.as_mut().filter(|layer| layer.shown) {
                    commands.entity(layer.entity).insert(Visibility::Hidden);
                    layer.shown = false;
                }
                continue;
            }
            if slot.is_none() {
                *slot = spawn_region_layer(
                    &mut commands,
                    &mut quads,
                    &mut meshes,
                    materials,
                    *culling,
                    key,
                    index,
                    list,
                );
            }
            let Some(layer) = slot.as_mut() else {
                continue;
            };
            update_region_layer(&mut commands, &mut quads, &mut meshes, layer, list);
            stats.draws += 1;
            stats.entries += list.len();
            stats.quads += u64::from(list.quads());
        }
    }
    regions.stats = stats;
    if let Some(mut perf) = perf {
        perf.regions.record(start.elapsed());
    }
}

fn spawn_region_layer(
    commands: &mut Commands,
    quads: &mut ChunkQuads,
    meshes: &mut Assets<Mesh>,
    materials: &ChunkMaterials,
    culling: ChunkCulling,
    region: IVec2,
    index: usize,
    list: &DrawList,
) -> Option<RegionLayer> {
    let capacity = list.len().next_power_of_two().max(MIN_ENTRIES);
    let Some(range) = quads.reserve(DrawList::records_for(capacity)) else {
        warn_once!("the chunk quad buffer is full; a chunk region is not drawn");
        return None;
    };
    let (mesh, proxy) = quads.region_proxy(meshes, list.quads());
    let mut entity = commands.spawn((
        Name::new(format!(
            "Chunk region {}, {}: {}",
            region.x, region.y, LAYER_NAMES[index]
        )),
        Mesh3d(mesh),
        MeshMaterial3d(materials.0[index].clone()),
        MeshTag::new(range.tag()),
        region_aabb(),
        NoAutoAabb,
        Transform::from_translation(region_origin(region)),
        Visibility::Hidden,
    ));
    if culling == ChunkCulling::Gpu {
        entity.insert(NoCpuCulling);
    }
    Some(RegionLayer {
        entity: entity.id(),
        range,
        capacity,
        proxy,
        written: DrawList::default(),
        shown: false,
    })
}

/// Give a region layer room for `list`, a proxy mesh that covers it, and
/// write the list if it changed.
fn update_region_layer(
    commands: &mut Commands,
    quads: &mut ChunkQuads,
    meshes: &mut Assets<Mesh>,
    layer: &mut RegionLayer,
    list: &DrawList,
) {
    let mut entity = commands.entity(layer.entity);
    if list.len() > layer.capacity {
        let capacity = list.len().next_power_of_two();
        let Some(range) = quads.reserve(DrawList::records_for(capacity)) else {
            warn_once!("the chunk quad buffer is full; a chunk region is not drawn");
            return;
        };
        quads.free(std::mem::replace(&mut layer.range, range));
        layer.capacity = capacity;
        layer.written = DrawList::default();
        entity.insert(MeshTag::new(layer.range.tag()));
    }
    // Shrink only well below the proxy, so a list near a size boundary does
    // not swap meshes every frame.
    if list.quads() > layer.proxy || list.quads().saturating_mul(4) < layer.proxy {
        let (mesh, proxy) = quads.region_proxy(meshes, list.quads());
        if proxy != layer.proxy {
            entity.insert(Mesh3d(mesh));
            layer.proxy = proxy;
        }
    }
    if *list != layer.written {
        quads.rewrite(&layer.range, list.bytes());
        layer.written.clone_from(list);
    }
    if !layer.shown {
        entity.insert(Visibility::Inherited);
        layer.shown = true;
    }
}

/// `PostUpdate`: after cave culling and transform propagation, before the
/// quad buffer grows for this frame's writes.
pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        draw_chunk_regions
            .in_set(ChunkQuadWrites)
            .after(crate::rendering::occlusion::cull_sections)
            .after(TransformSystems::Propagate)
            .run_if(resource_exists::<WorldStreaming>),
    );
}

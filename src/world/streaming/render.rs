use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::prelude::*;

use crate::world::chunk::ChunkPos;
use crate::world::meshing::ChunkMeshes;
use crate::world::textures::LEAF_WIGGLE_AMPLITUDE;
use crate::world::textures::LeafCutoutMaterial;

pub(super) struct RenderedChunk {
    entity: Entity,
    opaque: Option<MeshLayer>,
    grass_overlay: Option<MeshLayer>,
    cutout: Option<MeshLayer>,
    water: Option<MeshLayer>,
    masked: Option<MeshLayer>,
}

struct MeshLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
}

pub(super) fn spawn_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    position: ChunkPos,
    layers: ChunkMeshes,
    material: &Handle<StandardMaterial>,
    grass_overlay_material: &Handle<StandardMaterial>,
    cutout_material: &Handle<LeafCutoutMaterial>,
    water_material: &Handle<StandardMaterial>,
    mask_material: &Handle<StandardMaterial>,
) -> RenderedChunk {
    let (x, z) = position.world_origin();
    let entity = commands
        .spawn((
            Name::new(format!("Chunk {}, {}", position.x, position.z)),
            position,
            Transform::from_xyz(x, 0.0, z),
            Visibility::default(),
        ))
        .id();
    let mut rendered = RenderedChunk {
        entity,
        opaque: None,
        grass_overlay: None,
        cutout: None,
        water: None,
        masked: None,
    };
    apply_chunk_meshes(
        commands,
        meshes,
        &mut rendered,
        layers,
        material,
        grass_overlay_material,
        cutout_material,
        water_material,
        mask_material,
    );
    rendered
}

pub(super) fn apply_chunk_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    rendered: &mut RenderedChunk,
    layers: ChunkMeshes,
    material: &Handle<StandardMaterial>,
    grass_overlay_material: &Handle<StandardMaterial>,
    cutout_material: &Handle<LeafCutoutMaterial>,
    water_material: &Handle<StandardMaterial>,
    mask_material: &Handle<StandardMaterial>,
) {
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.opaque,
        layers.opaque,
        material,
        "Opaque",
        0.0,
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.grass_overlay,
        layers.grass_overlay,
        grass_overlay_material,
        "Grass overlay",
        0.0,
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.cutout,
        layers.cutout,
        cutout_material,
        "Cutout",
        LEAF_WIGGLE_AMPLITUDE * 1.5,
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.water,
        layers.water,
        water_material,
        "Water",
        0.0,
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.masked,
        layers.masked,
        mask_material,
        "Alpha-masked geometry",
        0.0,
    );
}

/// Bevy 0.19's mesh allocator logs a use-after-free error if an empty mesh is
/// spawned or uploaded. Skip those layers until they have faces.
fn apply_layer<M: Material>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    layer: &mut Option<MeshLayer>,
    mesh: Mesh,
    material: &Handle<M>,
    name: &'static str,
    bounds_padding: f32,
) {
    let empty = mesh.count_vertices() == 0;
    // Meshes are uploaded once and retain only metadata in the main world.
    // Compute their local bounds while vertex positions are still available.
    let bounds = (!empty).then(|| {
        let mut bounds = mesh.compute_aabb().expect("chunk mesh has positions");
        // Leaf vertices move in the shader; keep them inside the culling box.
        bounds.half_extents += Vec3A::splat(bounds_padding);
        bounds
    });
    match (layer.take(), empty) {
        (Some(existing), false) => {
            if let Some(mut current) = meshes.get_mut(existing.mesh.id()) {
                *current = mesh;
            }
            commands.entity(existing.entity).insert(bounds.unwrap());
            *layer = Some(existing);
        }
        (Some(existing), true) => {
            commands.entity(existing.entity).despawn();
            meshes.remove(existing.mesh.id());
        }
        (None, false) => {
            let handle = meshes.add(mesh);
            let entity = commands
                .spawn((
                    Name::new(name),
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(material.clone()),
                    bounds.unwrap(),
                    NoAutoAabb,
                    Transform::default(),
                    ChildOf(parent),
                ))
                .id();
            *layer = Some(MeshLayer {
                entity,
                mesh: handle,
            });
        }
        (None, true) => {}
    }
}

pub(super) fn despawn_rendered_chunk(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    rendered: RenderedChunk,
) {
    commands.entity(rendered.entity).despawn();
    if let Some(layer) = rendered.opaque {
        meshes.remove(layer.mesh.id());
    }
    if let Some(layer) = rendered.grass_overlay {
        meshes.remove(layer.mesh.id());
    }
    if let Some(layer) = rendered.cutout {
        meshes.remove(layer.mesh.id());
    }
    if let Some(layer) = rendered.water {
        meshes.remove(layer.mesh.id());
    }
    if let Some(layer) = rendered.masked {
        meshes.remove(layer.mesh.id());
    }
}

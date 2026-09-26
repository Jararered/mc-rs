use std::time::Instant;

use bevy::tasks::AsyncComputeTaskPool;

use super::WorldStreaming;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::Skylight;
use crate::world::meshing::ChunkNeighbors;
use crate::world::meshing::mesh_chunk_with_biomes;

/// Snapshot the nine loaded chunks on the main thread, then do all lighting and
/// mesh construction in a background job. A cancelled job never publishes its mesh.
pub(super) fn spawn_mesh_job(
    streaming: &mut WorldStreaming,
    chunks: &WorldChunks,
    position: ChunkPosition,
) -> bool {
    if streaming.meshing.contains_key(&position) || !mesh_neighborhood_ready(chunks, position) {
        return false;
    }
    let Some(generated) = chunks.get(position) else {
        return false;
    };
    let chunk = generated.chunk.clone();
    let biomes = generated.biomes.clone();
    let grass_colors = streaming.grass_colors.clone();
    let foliage_colors = streaming.foliage_colors.clone();
    let old_lighting = streaming.old_lighting;
    let smooth_lighting = streaming.smooth_lighting;
    let fancy_graphics = streaming.fancy_graphics;
    let skylight_subtracted = streaming.skylight_subtracted;
    let neighbor = |dx: i32, dz: i32| {
        position
            .x
            .checked_add(dx)
            .zip(position.z.checked_add(dz))
            .and_then(|(x, z)| chunks.get(ChunkPosition { x, z }))
            .map(|generated| generated.chunk.clone())
    };
    let west = neighbor(-1, 0);
    let east = neighbor(1, 0);
    let north = neighbor(0, -1);
    let south = neighbor(0, 1);
    let northwest = neighbor(-1, -1);
    let northeast = neighbor(1, -1);
    let southwest = neighbor(-1, 1);
    let southeast = neighbor(1, 1);
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let start = Instant::now();
        let skylight = Skylight::from_chunk_with_neighbors_and_corners(
            &chunk,
            west.as_ref(),
            east.as_ref(),
            north.as_ref(),
            south.as_ref(),
            northwest.as_ref(),
            northeast.as_ref(),
            southwest.as_ref(),
            southeast.as_ref(),
        );
        let neighbors = ChunkNeighbors {
            west: west.as_ref(),
            east: east.as_ref(),
            north: north.as_ref(),
            south: south.as_ref(),
            northwest: northwest.as_ref(),
            northeast: northeast.as_ref(),
            southwest: southwest.as_ref(),
            southeast: southeast.as_ref(),
        };
        let layers = mesh_chunk_with_biomes(
            &chunk,
            &neighbors,
            &skylight,
            &biomes,
            &grass_colors,
            &foliage_colors,
            old_lighting,
            smooth_lighting,
            fancy_graphics,
            skylight_subtracted,
            position,
        );
        (layers, start.elapsed())
    });
    streaming.meshing.insert(position, task);
    true
}

/// Lighting and ambient occlusion sample across both faces and corners. Do
/// not bake fallback edge light into a chunk's first mesh while any of its
/// surrounding block data is still being generated or loaded.
pub(super) fn mesh_neighborhood_ready(chunks: &WorldChunks, position: ChunkPosition) -> bool {
    (-1..=1).all(|dx| {
        (-1..=1).all(|dz| {
            position
                .x
                .checked_add(dx)
                .zip(position.z.checked_add(dz))
                .is_some_and(|(x, z)| chunks.contains(ChunkPosition { x, z }))
        })
    })
}

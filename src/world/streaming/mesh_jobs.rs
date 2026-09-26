use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::Task;

use super::ALL_SECTIONS;
use super::SectionMask;
use super::WorldStreaming;
use super::render::SectionMeshes;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTIONS_PER_CHUNK;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::Skylight;
use crate::world::meshing::ChunkNeighbors;
use crate::world::meshing::SectionMesher;

/// A background lighting and meshing job for one chunk. `forced` is kept so a
/// cancelled job's pending section rebuilds can be requeued.
pub(super) struct MeshTask {
    pub(super) task: Task<MeshJob>,
    pub(super) forced: SectionMask,
}

pub(super) struct MeshJob {
    pub(super) sections: Vec<SectionMeshes>,
    pub(super) fingerprints: [u64; SECTIONS_PER_CHUNK],
    /// The chunk's light from this pass, for [`crate::world::lighting::LightCache`].
    pub(super) light: Arc<[u8]>,
    pub(super) elapsed: Duration,
}

/// Snapshot the nine loaded chunks on the main thread, then do all lighting and
/// mesh construction in a background job. A cancelled job never publishes its mesh.
///
/// A chunk that is already rendered rebuilds the `forced` sections plus any
/// section whose light fingerprint changed; a first mesh builds all of them.
pub(super) fn spawn_mesh_job(
    streaming: &mut WorldStreaming,
    chunks: &WorldChunks,
    position: ChunkPosition,
    forced: SectionMask,
) -> bool {
    if streaming.meshing.contains_key(&position)
        || !mesh_neighborhood_ready(chunks, &streaming.held, position)
    {
        return false;
    }
    let Some(generated) = chunks.get(position) else {
        return false;
    };
    let chunk = generated.chunk.clone();
    let biomes = generated.biomes.clone();
    let grass_colors = streaming.grass_colors.clone();
    let foliage_colors = streaming.foliage_colors.clone();
    let fancy_graphics = streaming.fancy_graphics;
    let previous = streaming
        .rendered
        .get(&position)
        .map(|rendered| rendered.fingerprints);
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
        let fingerprints = skylight.section_fingerprints();
        let rebuild = previous.map_or(ALL_SECTIONS, |previous| {
            (0..SECTIONS_PER_CHUNK)
                .filter(|&section| previous[section] != fingerprints[section])
                .fold(forced, |mask, section| mask | (1 << section))
        });
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
        let mesher = SectionMesher::new(
            &chunk,
            &neighbors,
            &skylight,
            &biomes,
            &grass_colors,
            &foliage_colors,
            fancy_graphics,
            position,
        );
        let sections = (0..SECTIONS_PER_CHUNK)
            .filter(|&section| rebuild & (1 << section) != 0)
            .map(|section| SectionMeshes::build(section, mesher.mesh(section)))
            .collect();
        MeshJob {
            sections,
            fingerprints,
            light: skylight.chunk_cells(),
            elapsed: start.elapsed(),
        }
    });
    streaming
        .meshing
        .insert(position, MeshTask { task, forced });
    true
}

/// Lighting and ambient occlusion sample across both faces and corners. Do
/// not bake fallback edge light into a chunk's first mesh while any of its
/// surrounding block data is still being generated, loaded, or populated.
///
/// A finished chunk is never held by a population job, so every chunk this
/// accepts is loaded. `held` supplies the flags of chunks jobs are holding.
pub(super) fn mesh_neighborhood_ready(
    chunks: &WorldChunks,
    held: &HashMap<ChunkPosition, bool>,
    position: ChunkPosition,
) -> bool {
    (-1..=1).all(|dx| {
        (-1..=1).all(|dz| {
            position
                .x
                .checked_add(dx)
                .zip(position.z.checked_add(dz))
                .is_some_and(|(x, z)| chunk_finished(chunks, held, ChunkPosition { x, z }))
        })
    })
}

/// A chunk is finished once every population pass that writes into it has
/// run: its own and those of its `-x`, `-z`, and `-x-z` neighbors. Population
/// never writes into a finished chunk.
fn chunk_finished(
    chunks: &WorldChunks,
    held: &HashMap<ChunkPosition, bool>,
    position: ChunkPosition,
) -> bool {
    chunks.contains(position)
        && [(0, 0), (-1, 0), (0, -1), (-1, -1)]
            .into_iter()
            .all(|(dx, dz)| {
                position
                    .x
                    .checked_add(dx)
                    .zip(position.z.checked_add(dz))
                    .and_then(|(x, z)| {
                        let writer = ChunkPosition { x, z };
                        chunks
                            .get(writer)
                            .map(|generated| generated.populated)
                            .or_else(|| held.get(&writer).copied())
                    })
                    .unwrap_or(false)
            })
}

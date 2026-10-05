use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::Task;

use crate::block::blocks::Block;

use super::chunk::CHUNK_SIZE;
use super::chunk::ChunkPosition;
use super::chunk::GeneratedChunk;
use super::chunk::SECTION_HEIGHT;
use super::chunk::SECTIONS_PER_CHUNK;
use super::generation::ChunkGenerator;
use super::generation::population_footprint;
use crate::rendering::textures::FoliageColors;
use crate::rendering::textures::GrassColors;

mod mesh_jobs;
mod render;
mod systems;

use mesh_jobs::MeshTask;
use render::ChunkMaterials;
use render::RenderedChunk;

pub use render::ChunkCulling;
pub(crate) use systems::setup_streaming;
pub(crate) use systems::stream_chunks;

pub const LOAD_RADIUS: i32 = 4;
/// Chunks are generated two rings beyond the render distance. A chunk is
/// finished once the population passes of it and its `-x`, `-z`, and `-x-z`
/// neighbors have run, and each pass needs its `+x`, `+z`, and `+x+z`
/// neighbors. Meshing a chunk needs its eight neighbors finished too, so the
/// last rendered ring depends on chunks two rings out.
pub const GENERATE_MARGIN: i32 = 2;
/// Chunks stay loaded one ring past where they are generated, so stepping
/// back and forth over a chunk border does not unload and reload a row of
/// chunks each time.
pub const UNLOAD_MARGIN: i32 = 1;
pub const UNLOAD_RADIUS: i32 = LOAD_RADIUS + GENERATE_MARGIN + UNLOAD_MARGIN;

/// One bit per render section of a chunk, bottom section first.
pub(crate) type SectionMask = u8;
const ALL_SECTIONS: SectionMask = u8::MAX;
const _: () = assert!(SECTIONS_PER_CHUNK == SectionMask::BITS as usize);
/// A block change can alter light up to 14 cells away, and meshes read one
/// more cell of light around their chunk.
const LIGHT_REACH: i32 = 15;
/// Face culling, ambient occlusion, and chest pairing read blocks up to two
/// cells from the face they build.
const GEOMETRY_REACH: i32 = 2;

/// The result of a generation job. `loaded` distinguishes a chunk read from disk
/// from one the generator produced, so only new chunks are marked for saving.
pub(crate) struct ChunkJob {
    pub chunk: GeneratedChunk,
    pub loaded: bool,
    pub elapsed: Duration,
}

/// The result of a population job: its source chunk and the source's `+x`,
/// `+z`, and `+x+z` neighbors, in that order.
pub(crate) struct PopulationJob {
    pub chunks: [GeneratedChunk; 4],
    pub elapsed: Duration,
}

/// Timing samples collected since the last performance print.
#[derive(Resource, Default)]
pub struct StreamingDiagnostics {
    pub generate: TimingStats,
    pub populate: TimingStats,
    pub load: TimingStats,
    pub mesh: TimingStats,
    /// Candidate-discovery passes; settled frames should leave this unchanged.
    pub discovery_passes: u64,
}

#[derive(Default)]
pub struct TimingStats {
    count: u32,
    sum_secs: f64,
    max_secs: f64,
}

impl TimingStats {
    pub fn record(&mut self, duration: Duration) {
        let secs = duration.as_secs_f64();
        self.count += 1;
        self.sum_secs += secs;
        if secs > self.max_secs {
            self.max_secs = secs;
        }
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn average_ms(&self) -> Option<f64> {
        (self.count > 0).then_some(self.sum_secs / f64::from(self.count) * 1000.0)
    }

    pub fn max_ms(&self) -> Option<f64> {
        (self.count > 0).then_some(self.max_secs * 1000.0)
    }

    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

#[derive(Resource)]
pub struct WorldStreaming {
    generator: Arc<dyn ChunkGenerator>,
    grass_colors: GrassColors,
    foliage_colors: FoliageColors,
    /// Base terrain and load jobs inside the generation radius.
    generating: HashMap<ChunkPosition, Task<ChunkJob>>,
    /// Population jobs by source chunk. Each takes its four chunks out of
    /// [`WorldChunks`](crate::world::chunk::WorldChunks) until it finishes,
    /// so jobs never share a chunk and nothing else sees one mid-pass.
    populating: HashMap<ChunkPosition, Task<PopulationJob>>,
    /// The chunks population jobs hold, with each one's `populated` flag.
    /// Whether a chunk is finished depends on its neighbors' flags, which
    /// must stay readable while those neighbors are away.
    held: HashMap<ChunkPosition, bool>,
    /// Lighting and mesh jobs for already generated chunks inside the render distance.
    meshing: HashMap<ChunkPosition, MeshTask>,
    rendered: HashMap<ChunkPosition, RenderedChunk>,
    materials: ChunkMaterials,
    culling: ChunkCulling,
    fancy_graphics: bool,
    /// Chunk meshes include only this block while it is set.
    wireframe_block: Option<Block>,
    remesh_queue: VecDeque<ChunkPosition>,
    /// Sections each queued chunk must rebuild even if its light did not
    /// change, because blocks they draw or sample changed.
    remesh_sections: HashMap<ChunkPosition, SectionMask>,
    desired_generation: Vec<ChunkPosition>,
    desired_meshing: Vec<ChunkPosition>,
    desired_center: Option<ChunkPosition>,
    desired_radius: i32,
    discovery_dirty: bool,
    /// Jobs of each kind kept in flight. Enough to keep every compute thread
    /// busy between frames, since finished jobs are only replaced once per
    /// streaming pass.
    max_in_flight: usize,
}

impl WorldStreaming {
    /// Despawn every chunk mesh and free its mesh assets. Call before dropping
    /// the resource when a world is unloaded, since the entities are only
    /// tracked here.
    pub(crate) fn despawn_rendered(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>) {
        for (_, rendered) in self.rendered.drain() {
            render::despawn_rendered_chunk(commands, meshes, rendered);
        }
    }

    pub fn rendered_mesh_count(&self) -> usize {
        self.rendered.len()
    }

    /// Nonempty section layer meshes across all rendered chunks.
    pub fn rendered_layer_count(&self) -> usize {
        self.rendered.values().map(RenderedChunk::layer_count).sum()
    }

    /// GPU vertex and index bytes of every rendered chunk mesh.
    pub fn mesh_bytes(&self) -> usize {
        self.rendered.values().map(RenderedChunk::mesh_bytes).sum()
    }

    pub fn generating_job_count(&self) -> usize {
        self.generating.len()
    }

    pub fn populating_job_count(&self) -> usize {
        self.populating.len()
    }

    pub fn meshing_job_count(&self) -> usize {
        self.meshing.len()
    }

    /// Whether `position` and its eight neighbors are loaded and finished, so
    /// no population pass will write into it again. Block ticks only run in
    /// chunks that pass this, the same test a first mesh waits for.
    pub fn neighborhood_finished(
        &self,
        chunks: &crate::world::chunk::WorldChunks,
        position: ChunkPosition,
    ) -> bool {
        mesh_jobs::mesh_neighborhood_ready(chunks, &self.held, position)
    }

    /// Chunks waiting to be remeshed, in dispatch order.
    pub fn queued_remesh_positions(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        self.remesh_queue.iter().copied()
    }

    /// Rebuild every section of this chunk from current world data.
    ///
    /// Cancels an in-flight first mesh so it cannot apply stale geometry after
    /// an edit. Already-rendered chunks are queued; unrendered ones are meshed
    /// again on the next streaming pass.
    pub fn request_remesh(&mut self, position: ChunkPosition) {
        self.queue_sections(position, ALL_SECTIONS);
    }

    /// The block at world `(x, y, z)` changed. Relight every chunk its light
    /// can reach, and rebuild the sections whose geometry reads that block.
    /// Other sections of those chunks rebuild only if their light changed.
    pub fn request_block_update(&mut self, x: i32, y: i32, z: i32) {
        let mut sections = HashMap::new();
        affected_sections(x, y, z, true, &mut sections);
        for (position, mask) in sections {
            self.queue_sections(position, mask);
        }
    }

    /// Queue remeshes for many changed blocks at once, visiting each affected
    /// chunk once. A change whose `light` flag is `false` kept the block's
    /// light opacity and emission, so it cannot relight a neighbor chunk and
    /// only rebuilds the sections whose geometry reads that block.
    pub fn request_block_changes(
        &mut self,
        changes: impl IntoIterator<Item = (i32, i32, i32, bool)>,
    ) {
        let mut sections = HashMap::new();
        for (x, y, z, light) in changes {
            affected_sections(x, y, z, light, &mut sections);
        }
        for (position, mask) in sections {
            self.queue_sections(position, mask);
        }
    }

    fn queue_sections(&mut self, position: ChunkPosition, mut sections: SectionMask) {
        if let Some(cancelled) = self.meshing.remove(&position) {
            sections |= cancelled.forced;
        }
        if !self.rendered.contains_key(&position) {
            self.discovery_dirty = true;
            // Its first mesh is built from scratch on the next streaming pass.
            self.remesh_sections.remove(&position);
            return;
        }
        *self.remesh_sections.entry(position).or_default() |= sections;
        self.remesh_queue.retain(|queued| *queued != position);
        self.remesh_queue.push_front(position);
    }

    /// Queue a full rebuild of every rendered chunk, nearest first.
    fn remesh_everything(&mut self) {
        self.discovery_dirty = true;
        self.meshing.clear();
        let mut positions: Vec<_> = self.rendered.keys().copied().collect();
        if let Some(center) = self.desired_center {
            systems::sort_by_distance(&mut positions, center);
        }
        self.remesh_sections = positions
            .iter()
            .map(|position| (*position, ALL_SECTIONS))
            .collect();
        self.remesh_queue = positions.into();
    }
}

/// Chunks a change at `(x, y, z)` must relight or remesh, merged into
/// `sections`. A chunk mapped to an empty mask only relights.
fn affected_sections(
    x: i32,
    y: i32,
    z: i32,
    light: bool,
    sections: &mut HashMap<ChunkPosition, SectionMask>,
) {
    let center = ChunkPosition::from_block(x, z);
    let size = CHUNK_SIZE as i32;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let position = ChunkPosition {
                x: center.x + dx,
                z: center.z + dz,
            };
            let (min_x, min_z) = (position.x * size, position.z * size);
            // Distance from the block to the chunk's light ring, which
            // extends one cell past its own columns.
            let light_distance = axis_distance(x, min_x - 1, min_x + size)
                + axis_distance(z, min_z - 1, min_z + size);
            let touches_columns = axis_distance(x, min_x, min_x + size - 1) <= GEOMETRY_REACH
                && axis_distance(z, min_z, min_z + size - 1) <= GEOMETRY_REACH;
            if touches_columns {
                *sections.entry(position).or_default() |= sections_near(y, GEOMETRY_REACH);
            } else if light && light_distance <= LIGHT_REACH {
                sections.entry(position).or_default();
            }
        }
    }
}

/// Distance from `value` to the inclusive range `low..=high`.
fn axis_distance(value: i32, low: i32, high: i32) -> i32 {
    if value < low {
        low - value
    } else if value > high {
        value - high
    } else {
        0
    }
}

/// Sections with a block within `reach` rows of `y`.
fn sections_near(y: i32, reach: i32) -> SectionMask {
    let height = SECTION_HEIGHT as i32;
    let last = SECTIONS_PER_CHUNK as i32 - 1;
    let low = (y - reach).div_euclid(height).clamp(0, last);
    let high = (y + reach).div_euclid(height).clamp(0, last);
    (low..=high).fold(0, |mask, section| mask | (1 << section))
}

pub fn within_radius(position: ChunkPosition, center: ChunkPosition, radius: i32) -> bool {
    (i64::from(position.x) - i64::from(center.x)).abs() <= i64::from(radius)
        && (i64::from(position.z) - i64::from(center.z)).abs() <= i64::from(radius)
}

pub fn positions_in_radius(center: ChunkPosition, radius: i32) -> Vec<ChunkPosition> {
    let mut positions = Vec::with_capacity(((radius * 2 + 1) * (radius * 2 + 1)) as usize);
    for z in -radius..=radius {
        for x in -radius..=radius {
            positions.push(ChunkPosition {
                x: center.x + x,
                z: center.z + z,
            });
        }
    }
    positions
}

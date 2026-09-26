use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::Task;

use super::chunk::CHUNK_SIZE;
use super::chunk::ChunkPosition;
use super::chunk::SECTION_HEIGHT;
use super::chunk::SECTIONS_PER_CHUNK;
use super::generation::GeneratedChunk;
use super::generation::WorldGenerator;
use super::textures::FoliageColors;
use super::textures::GrassColors;

mod mesh_jobs;
mod render;
mod systems;

use mesh_jobs::MeshTask;
use render::ChunkMaterials;
use render::RenderedChunk;

pub(crate) use systems::regenerate_loaded_chunks;
pub(crate) use systems::setup_streaming;
pub(crate) use systems::stream_chunks;

pub const LOAD_RADIUS: i32 = 4;
/// Chunks are generated one ring beyond the render distance. Decoration such as
/// trees can spill into a chunk from its neighbours, so the extra ring is
/// generated (but not meshed) before the chunks inside the render distance are.
pub const GENERATE_MARGIN: i32 = 1;
pub const UNLOAD_RADIUS: i32 = LOAD_RADIUS + GENERATE_MARGIN;

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

/// Timing samples collected since the last performance print.
#[derive(Resource, Default)]
pub struct StreamingDiagnostics {
    pub generate: TimingStats,
    pub load: TimingStats,
    pub mesh: TimingStats,
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
    generator: Arc<WorldGenerator>,
    grass_colors: GrassColors,
    foliage_colors: FoliageColors,
    /// Terrain and decoration jobs inside the generation radius.
    generating: HashMap<ChunkPosition, Task<ChunkJob>>,
    /// Lighting and mesh jobs for already generated chunks inside the render distance.
    meshing: HashMap<ChunkPosition, MeshTask>,
    rendered: HashMap<ChunkPosition, RenderedChunk>,
    materials: ChunkMaterials,
    fancy_graphics: bool,
    remesh_queue: VecDeque<ChunkPosition>,
    /// Sections each queued chunk must rebuild even if its light did not
    /// change, because blocks they draw or sample changed.
    remesh_sections: HashMap<ChunkPosition, SectionMask>,
    desired_generation: Vec<ChunkPosition>,
    desired_meshing: Vec<ChunkPosition>,
    desired_center: Option<ChunkPosition>,
    desired_radius: i32,
    /// Jobs of each kind kept in flight. Enough to keep every compute thread
    /// busy between frames, since finished jobs are only replaced once per
    /// streaming pass.
    max_in_flight: usize,
}

impl WorldStreaming {
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

    pub fn meshing_job_count(&self) -> usize {
        self.meshing.len()
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
                if light_distance > LIGHT_REACH {
                    continue;
                }
                let touches_columns = axis_distance(x, min_x, min_x + size - 1) <= GEOMETRY_REACH
                    && axis_distance(z, min_z, min_z + size - 1) <= GEOMETRY_REACH;
                let sections = if touches_columns {
                    sections_near(y, GEOMETRY_REACH)
                } else {
                    0
                };
                self.queue_sections(position, sections);
            }
        }
    }

    fn queue_sections(&mut self, position: ChunkPosition, mut sections: SectionMask) {
        if let Some(cancelled) = self.meshing.remove(&position) {
            sections |= cancelled.forced;
        }
        if !self.rendered.contains_key(&position) {
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

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;
use bevy::tasks::Task;

use super::chunk::ChunkPos;
use super::generation::GeneratedChunk;
use super::generation::WorldGenerator;
use super::meshing::ChunkMeshes;
use super::textures::FoliageColors;
use super::textures::GrassColors;
use super::textures::LeafCutoutMaterial;

mod mesh_jobs;
mod render;
mod systems;

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
    generating: HashMap<ChunkPos, Task<ChunkJob>>,
    /// Mesh jobs for already generated chunks inside the render distance.
    meshing: HashMap<ChunkPos, Task<(ChunkMeshes, Duration)>>,
    rendered: HashMap<ChunkPos, RenderedChunk>,
    material: Handle<StandardMaterial>,
    grass_overlay_material: Handle<StandardMaterial>,
    cutout_material: Handle<LeafCutoutMaterial>,
    water_material: Handle<StandardMaterial>,
    mask_material: Handle<StandardMaterial>,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
    /// `World.skylightSubtracted`. Meshes bake this into vertex brightness, so
    /// a change rebuilds the loaded chunks the way Beta's `updateAllRenderers` does.
    skylight_subtracted: u8,
    remesh_queue: VecDeque<ChunkPos>,
    desired_generation: Vec<ChunkPos>,
    desired_meshing: Vec<ChunkPos>,
    desired_center: Option<ChunkPos>,
    desired_radius: i32,
}

impl WorldStreaming {
    pub fn rendered_mesh_count(&self) -> usize {
        self.rendered.len()
    }

    pub fn generating_job_count(&self) -> usize {
        self.generating.len()
    }

    pub fn meshing_job_count(&self) -> usize {
        self.meshing.len()
    }

    /// Chunks waiting to be remeshed, in dispatch order.
    pub fn queued_remesh_positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.remesh_queue.iter().copied()
    }

    /// Rebuild this chunk's mesh from current world data.
    ///
    /// Cancels an in-flight first mesh so it cannot apply stale geometry after
    /// an edit. Already-rendered chunks are queued; unrendered ones are meshed
    /// again on the next streaming pass.
    pub fn request_remesh(&mut self, position: ChunkPos) {
        self.meshing.remove(&position);
        if self.rendered.contains_key(&position) {
            self.remesh_queue.retain(|queued| *queued != position);
            self.remesh_queue.push_front(position);
        }
    }
}

pub fn within_radius(position: ChunkPos, center: ChunkPos, radius: i32) -> bool {
    (i64::from(position.x) - i64::from(center.x)).abs() <= i64::from(radius)
        && (i64::from(position.z) - i64::from(center.z)).abs() <= i64::from(radius)
}

pub fn positions_in_radius(center: ChunkPos, radius: i32) -> Vec<ChunkPos> {
    let mut positions = Vec::with_capacity(((radius * 2 + 1) * (radius * 2 + 1)) as usize);
    for z in -radius..=radius {
        for x in -radius..=radius {
            positions.push(ChunkPos {
                x: center.x + x,
                z: center.z + z,
            });
        }
    }
    positions
}

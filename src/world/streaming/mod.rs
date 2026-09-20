use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::player::Player;
use crate::world::persistence::WorldPersistence;

use super::chunk::ChunkPos;
use super::chunk::WorldChunks;
use super::generation::GeneratedChunk;
use super::generation::WorldGenerator;
use super::lighting::Skylight;
use super::meshing::mesh_chunk_with_biomes;
use super::textures::FoliageColors;
use super::textures::GrassColors;
use super::textures::TerrainMaterial;

pub const LOAD_RADIUS: i32 = 4;
/// Chunks are generated one ring beyond the render distance. Decoration such as
/// trees can spill into a chunk from its neighbours, so the extra ring is
/// generated (but not meshed) before the chunks inside the render distance are.
pub const GENERATE_MARGIN: i32 = 1;
pub const UNLOAD_RADIUS: i32 = LOAD_RADIUS + GENERATE_MARGIN;
const MAX_IN_FLIGHT: usize = 2;

/// The result of a generation job. `loaded` distinguishes a chunk read from disk
/// from one the generator produced, so only new chunks are marked for saving.
pub(crate) struct ChunkJob {
    pub chunk: GeneratedChunk,
    pub loaded: bool,
    pub elapsed: Duration,
}

/// Timing samples collected since the last performance print.
#[derive(Resource, Default)]
pub struct StreamingPerf {
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
pub(crate) struct WorldStreaming {
    generator: Arc<WorldGenerator>,
    grass_colors: GrassColors,
    foliage_colors: FoliageColors,
    /// Terrain and decoration jobs inside the generation radius.
    generating: HashMap<ChunkPos, Task<ChunkJob>>,
    /// Mesh jobs for already generated chunks inside the render distance.
    meshing: HashMap<ChunkPos, Task<(Mesh, Duration)>>,
    rendered: HashMap<ChunkPos, (Entity, Handle<Mesh>)>,
    material: Handle<StandardMaterial>,
    old_lighting: bool,
    fancy_graphics: bool,
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
}

pub(crate) fn setup_streaming(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    grass_colors: Res<GrassColors>,
    foliage_colors: Res<FoliageColors>,
    settings: Res<GameSettings>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut chunks: ResMut<WorldChunks>,
    mut perf: ResMut<StreamingPerf>,
) {
    let seed = persistence
        .as_ref()
        .map_or(0, |persistence| persistence.seed());
    let generator = Arc::new(WorldGenerator::new(seed));
    // The player starts in PostStartup and needs this heightmap immediately, so
    // the spawn chunk is loaded or generated synchronously.
    let load_start = Instant::now();
    let stored = persistence
        .as_ref()
        .and_then(|persistence| persistence.storage())
        .and_then(|storage| storage.load_chunk(ChunkPos::ZERO));
    let generated = match stored {
        Some(chunk) => {
            perf.load.record(load_start.elapsed());
            chunk
        }
        None => {
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPos::ZERO);
            }
            let generate_start = Instant::now();
            let chunk = generator.generate(ChunkPos::ZERO);
            perf.generate.record(generate_start.elapsed());
            chunk
        }
    };
    let skylight = Skylight::from_chunk(&generated.chunk);
    let mesh = meshes.add(mesh_chunk_with_biomes(
        &generated.chunk,
        &skylight,
        &generated.biomes,
        &grass_colors,
        &foliage_colors,
        settings.old_lighting,
        settings.fancy_graphics,
    ));
    let material = terrain_material.0.clone();
    let entity = spawn_chunk(&mut commands, ChunkPos::ZERO, &mesh, &material);

    chunks.insert(ChunkPos::ZERO, generated);
    commands.insert_resource(WorldStreaming {
        generator,
        grass_colors: grass_colors.clone(),
        foliage_colors: foliage_colors.clone(),
        generating: HashMap::new(),
        meshing: HashMap::new(),
        rendered: HashMap::from([(ChunkPos::ZERO, (entity, mesh))]),
        material,
        old_lighting: settings.old_lighting,
        fancy_graphics: settings.fancy_graphics,
        remesh_queue: VecDeque::new(),
        desired_generation: Vec::new(),
        desired_meshing: Vec::new(),
        desired_center: None,
        desired_radius: 0,
    });
}

/// Regenerate every loaded chunk from the world generator when F3 is pressed.
///
/// Stored chunk data is dropped so [`stream_chunks`] re-runs terrain and
/// decoration, then rebuilds each mesh. Rendered entities are kept so the world
/// stays visible while it updates.
pub(crate) fn regenerate_loaded_chunks(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    screen: Option<Res<State<AppScreen>>>,
    mut streaming: ResMut<WorldStreaming>,
    mut chunks: ResMut<WorldChunks>,
    mut persistence: Option<ResMut<WorldPersistence>>,
) {
    if screen.is_some_and(|state| *state.get() != AppScreen::Playing) {
        return;
    }
    if keys.is_some_and(|keys| keys.just_pressed(KeyCode::F3)) {
        let count = chunks.positions().count();
        streaming.generating.clear();
        streaming.meshing.clear();
        streaming.remesh_queue.clear();
        chunks.clear();
        // Saved chunks would otherwise be loaded straight back from disk.
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.request_regeneration();
        }
        info!("Regenerating {count} loaded chunks from scratch");
    }
}

pub(crate) fn stream_chunks(
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut streaming: ResMut<WorldStreaming>,
    mut chunks: ResMut<WorldChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    settings: Res<GameSettings>,
    screen: Option<Res<State<AppScreen>>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut perf: ResMut<StreamingPerf>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPos::from_world(player.translation.x, player.translation.z);
    let load_radius = settings.render_distance;
    let generate_radius = load_radius + GENERATE_MARGIN;
    let unload_radius = generate_radius;

    if streaming.old_lighting != settings.old_lighting
        || streaming.fancy_graphics != settings.fancy_graphics
    {
        streaming.old_lighting = settings.old_lighting;
        streaming.fancy_graphics = settings.fancy_graphics;
        // In-flight meshes were built with the previous lighting or leaf style.
        streaming.meshing.clear();
        streaming.remesh_queue = streaming.rendered.keys().copied().collect();
    }

    // Dropping an unfinished task cancels work that is no longer useful.
    streaming
        .generating
        .retain(|position, _| within_radius(*position, center, generate_radius));
    streaming
        .meshing
        .retain(|position, _| within_radius(*position, center, load_radius));

    let expired: Vec<_> = streaming
        .rendered
        .keys()
        .copied()
        .filter(|position| !within_radius(*position, center, unload_radius))
        .collect();
    for position in expired {
        if let Some((entity, mesh)) = streaming.rendered.remove(&position) {
            commands.entity(entity).despawn();
            meshes.remove(mesh.id());
        }
    }

    // Stored chunks are kept for the whole generation radius, including the ring
    // that is generated ahead of the render distance.
    let stale: Vec<_> = chunks
        .positions()
        .filter(|position| !within_radius(*position, center, generate_radius))
        .collect();
    for position in stale {
        if let Some(chunk) = chunks.remove(position)
            && let Some(persistence) = persistence.as_deref_mut()
        {
            persistence.queue_unload(position, chunk);
        }
    }

    // Rebuild one loaded mesh per frame when lighting or leaf graphics change.
    if let Some(position) = streaming.remesh_queue.pop_front()
        && let Some(generated) = chunks.get(position)
        && let Some((_, handle)) = streaming.rendered.get(&position)
    {
        let start = Instant::now();
        let skylight = Skylight::from_chunk(&generated.chunk);
        let mesh = mesh_chunk_with_biomes(
            &generated.chunk,
            &skylight,
            &generated.biomes,
            &streaming.grass_colors,
            &streaming.foliage_colors,
            streaming.old_lighting,
            streaming.fancy_graphics,
        );
        perf.mesh.record(start.elapsed());
        if let Some(mut existing) = meshes.get_mut(handle.id()) {
            *existing = mesh;
        }
    }

    // Apply finished generation jobs. Decoration is part of generation, so a
    // chunk is only eligible for meshing once it lands in `chunks`.
    let generated: Vec<_> = streaming
        .generating
        .iter_mut()
        .filter_map(|(position, task)| check_ready(task).map(|job| (*position, job)))
        .collect();
    for (position, job) in generated {
        streaming.generating.remove(&position);
        if job.loaded {
            perf.load.record(job.elapsed);
        } else {
            perf.generate.record(job.elapsed);
        }
        if within_radius(position, center, generate_radius) {
            if !job.loaded
                && let Some(persistence) = persistence.as_deref_mut()
            {
                persistence.mark_dirty(position);
            }
            chunks.insert(position, job.chunk);
        }
    }

    // Apply finished mesh jobs. A chunk that is already rendered (for example
    // after a regeneration) keeps its entity and handle; only the geometry is
    // replaced.
    let meshed: Vec<_> = streaming
        .meshing
        .iter_mut()
        .filter_map(|(position, task)| check_ready(task).map(|job| (*position, job)))
        .collect();
    for (position, (mesh, elapsed)) in meshed {
        streaming.meshing.remove(&position);
        perf.mesh.record(elapsed);
        if !within_radius(position, center, load_radius) {
            continue;
        }
        let existing = streaming
            .rendered
            .get(&position)
            .map(|(_, handle)| handle.id());
        if let Some(id) = existing {
            if let Some(mut existing) = meshes.get_mut(id) {
                *existing = mesh;
            }
        } else {
            let handle = meshes.add(mesh);
            let entity = spawn_chunk(&mut commands, position, &handle, &streaming.material);
            streaming.rendered.insert(position, (entity, handle));
        }
    }

    if screen.is_some_and(|state| *state.get() != AppScreen::Playing) {
        return;
    }

    if streaming.desired_center != Some(center) || streaming.desired_radius != load_radius {
        streaming.desired_generation = positions_in_radius(center, generate_radius);
        sort_by_distance(&mut streaming.desired_generation, center);
        streaming.desired_meshing = positions_in_radius(center, load_radius);
        sort_by_distance(&mut streaming.desired_meshing, center);
        streaming.desired_center = Some(center);
        streaming.desired_radius = load_radius;
    }

    // Generate terrain and decoration for the render distance plus one ring.
    // Chunks already on disk are loaded instead of regenerated, unless F3 asked
    // for a from-scratch pass.
    let bypass_load = persistence
        .as_deref()
        .is_some_and(WorldPersistence::bypass_load);
    let mut in_flight = streaming.generating.len();
    let mut to_generate = Vec::new();
    for position in &streaming.desired_generation {
        if in_flight >= MAX_IN_FLIGHT {
            break;
        }
        if chunks.contains(*position)
            || streaming.generating.contains_key(position)
            || streaming.meshing.contains_key(position)
        {
            continue;
        }
        to_generate.push(*position);
        in_flight += 1;
    }
    let spawned = !to_generate.is_empty();
    for position in to_generate {
        let generator = Arc::clone(&streaming.generator);
        let storage = persistence
            .as_deref()
            .and_then(WorldPersistence::storage)
            .filter(|_| !bypass_load)
            .cloned();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let start = Instant::now();
            if let Some(storage) = storage
                && let Some(chunk) = storage.load_chunk(position)
            {
                return ChunkJob {
                    chunk,
                    loaded: true,
                    elapsed: start.elapsed(),
                };
            }
            ChunkJob {
                chunk: generator.generate(position),
                loaded: false,
                elapsed: start.elapsed(),
            }
        });
        streaming.generating.insert(position, task);
    }

    // A regeneration pass is over once every desired chunk has been produced.
    if bypass_load
        && !spawned
        && streaming.generating.is_empty()
        && !streaming.desired_generation.is_empty()
        && let Some(persistence) = persistence.as_deref_mut()
    {
        persistence.finish_regeneration();
    }

    // Mesh generated chunks inside the render distance. The chunk is cloned into
    // the job so the stored copy stays available while the mesh is built.
    let mut in_flight = streaming.meshing.len();
    let mut to_mesh = Vec::new();
    for position in &streaming.desired_meshing {
        if in_flight >= MAX_IN_FLIGHT {
            break;
        }
        if streaming.rendered.contains_key(position) || streaming.meshing.contains_key(position) {
            continue;
        }
        if !chunks.contains(*position) {
            continue;
        }
        to_mesh.push(*position);
        in_flight += 1;
    }
    for position in to_mesh {
        let Some(generated) = chunks.get(position) else {
            continue;
        };
        let chunk = generated.chunk.clone();
        let biomes = generated.biomes.clone();
        let grass_colors = streaming.grass_colors.clone();
        let foliage_colors = streaming.foliage_colors.clone();
        let old_lighting = streaming.old_lighting;
        let fancy_graphics = streaming.fancy_graphics;
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let start = Instant::now();
            let skylight = Skylight::from_chunk(&chunk);
            let mesh = mesh_chunk_with_biomes(
                &chunk,
                &skylight,
                &biomes,
                &grass_colors,
                &foliage_colors,
                old_lighting,
                fancy_graphics,
            );
            (mesh, start.elapsed())
        });
        streaming.meshing.insert(position, task);
    }
}

fn sort_by_distance(positions: &mut [ChunkPos], center: ChunkPos) {
    positions.sort_by_key(|position| {
        let dx = i64::from(position.x) - i64::from(center.x);
        let dz = i64::from(position.z) - i64::from(center.z);
        (dx * dx + dz * dz, position.x, position.z)
    });
}

fn spawn_chunk(
    commands: &mut Commands,
    position: ChunkPos,
    mesh: &Handle<Mesh>,
    material: &Handle<StandardMaterial>,
) -> Entity {
    let (x, z) = position.world_origin();
    commands
        .spawn((
            Name::new(format!("Chunk {}, {}", position.x, position.z)),
            position,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, 0.0, z),
        ))
        .id()
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

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
use super::meshing::ChunkMeshes;
use super::meshing::mesh_chunk_with_biomes;
use super::textures::CutoutMaterial;
use super::textures::FoliageColors;
use super::textures::GrassColors;
use super::textures::GrassOverlayMaterial;
use super::textures::LeafCutoutMaterial;
use super::textures::TerrainMaterial;
use super::textures::WaterMaterial;

pub const LOAD_RADIUS: i32 = 4;
/// Chunks are generated one ring beyond the render distance. Decoration such as
/// trees can spill into a chunk from its neighbours, so the extra ring is
/// generated (but not meshed) before the chunks inside the render distance are.
pub const GENERATE_MARGIN: i32 = 1;
pub const UNLOAD_RADIUS: i32 = LOAD_RADIUS + GENERATE_MARGIN;
const MAX_IN_FLIGHT: usize = 2;
/// Player edits touch one to three chunks; settings remesh is the large case.
const MAX_REMESH_PER_FRAME: usize = 8;

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
    meshing: HashMap<ChunkPos, Task<(ChunkMeshes, Duration)>>,
    rendered: HashMap<ChunkPos, RenderedChunk>,
    material: Handle<StandardMaterial>,
    grass_overlay_material: Handle<StandardMaterial>,
    cutout_material: Handle<LeafCutoutMaterial>,
    water_material: Handle<StandardMaterial>,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
    remesh_queue: VecDeque<ChunkPos>,
    desired_generation: Vec<ChunkPos>,
    desired_meshing: Vec<ChunkPos>,
    desired_center: Option<ChunkPos>,
    desired_radius: i32,
}

struct RenderedChunk {
    entity: Entity,
    opaque: Option<MeshLayer>,
    grass_overlay: Option<MeshLayer>,
    cutout: Option<MeshLayer>,
    water: Option<MeshLayer>,
}

struct MeshLayer {
    entity: Entity,
    mesh: Handle<Mesh>,
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

    /// Rebuild this chunk's mesh from current world data.
    ///
    /// Cancels an in-flight first mesh so it cannot apply stale geometry after
    /// an edit. Already-rendered chunks are queued; unrendered ones are meshed
    /// again on the next streaming pass.
    pub fn request_remesh(&mut self, position: ChunkPos) {
        self.meshing.remove(&position);
        if self.rendered.contains_key(&position) && !self.remesh_queue.contains(&position) {
            self.remesh_queue.push_back(position);
        }
    }
}

pub(crate) fn setup_streaming(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    grass_overlay_material: Res<GrassOverlayMaterial>,
    cutout_material: Res<CutoutMaterial>,
    water_material: Res<WaterMaterial>,
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
    let layers = mesh_chunk_with_biomes(
        &generated.chunk,
        &skylight,
        &generated.biomes,
        &grass_colors,
        &foliage_colors,
        settings.old_lighting,
        settings.smooth_lighting,
        settings.graphics.fancy_leaves(),
    );
    let material = terrain_material.0.clone();
    let cutout_material = cutout_material.0.clone();
    let water_material = water_material.0.clone();
    let grass_overlay_material = grass_overlay_material.0.clone();
    let rendered = spawn_chunk(
        &mut commands,
        &mut meshes,
        ChunkPos::ZERO,
        layers,
        &material,
        &grass_overlay_material,
        &cutout_material,
        &water_material,
    );

    chunks.insert(ChunkPos::ZERO, generated);
    commands.insert_resource(WorldStreaming {
        generator,
        grass_colors: grass_colors.clone(),
        foliage_colors: foliage_colors.clone(),
        generating: HashMap::new(),
        meshing: HashMap::new(),
        rendered: HashMap::from([(ChunkPos::ZERO, rendered)]),
        material,
        grass_overlay_material,
        cutout_material,
        water_material,
        old_lighting: settings.old_lighting,
        smooth_lighting: settings.smooth_lighting,
        fancy_graphics: settings.graphics.fancy_leaves(),
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
        || streaming.smooth_lighting != settings.smooth_lighting
        || streaming.fancy_graphics != settings.graphics.fancy_leaves()
    {
        streaming.old_lighting = settings.old_lighting;
        streaming.smooth_lighting = settings.smooth_lighting;
        streaming.fancy_graphics = settings.graphics.fancy_leaves();
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
        if let Some(rendered) = streaming.rendered.remove(&position) {
            despawn_rendered_chunk(&mut commands, &mut meshes, rendered);
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

    // Rebuild a few loaded meshes per frame. Player edits need the changed
    // chunks to appear immediately; a lighting or leaf-graphics change still
    // spreads across several frames.
    for _ in 0..MAX_REMESH_PER_FRAME {
        let Some(position) = streaming.remesh_queue.pop_front() else {
            break;
        };
        let Some(generated) = chunks.get(position) else {
            continue;
        };
        let start = Instant::now();
        let west = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z,
            })
            .map(|chunk| &chunk.chunk);
        let east = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z,
            })
            .map(|chunk| &chunk.chunk);
        let north = chunks
            .get(ChunkPos {
                x: position.x,
                z: position.z - 1,
            })
            .map(|chunk| &chunk.chunk);
        let south = chunks
            .get(ChunkPos {
                x: position.x,
                z: position.z + 1,
            })
            .map(|chunk| &chunk.chunk);
        let northwest = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z - 1,
            })
            .map(|chunk| &chunk.chunk);
        let northeast = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z - 1,
            })
            .map(|chunk| &chunk.chunk);
        let southwest = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z + 1,
            })
            .map(|chunk| &chunk.chunk);
        let southeast = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z + 1,
            })
            .map(|chunk| &chunk.chunk);
        let skylight = Skylight::from_chunk_with_neighbors_and_corners(
            &generated.chunk,
            west,
            east,
            north,
            south,
            northwest,
            northeast,
            southwest,
            southeast,
        );
        let layers = mesh_chunk_with_biomes(
            &generated.chunk,
            &skylight,
            &generated.biomes,
            &streaming.grass_colors,
            &streaming.foliage_colors,
            streaming.old_lighting,
            streaming.smooth_lighting,
            streaming.fancy_graphics,
        );
        perf.mesh.record(start.elapsed());
        let material = streaming.material.clone();
        let grass_overlay_material = streaming.grass_overlay_material.clone();
        let cutout_material = streaming.cutout_material.clone();
        let water_material = streaming.water_material.clone();
        if let Some(rendered) = streaming.rendered.get_mut(&position) {
            apply_chunk_meshes(
                &mut commands,
                &mut meshes,
                rendered,
                layers,
                &material,
                &grass_overlay_material,
                &cutout_material,
                &water_material,
            );
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
    for (position, (layers, elapsed)) in meshed {
        streaming.meshing.remove(&position);
        perf.mesh.record(elapsed);
        if !within_radius(position, center, load_radius) {
            continue;
        }
        let material = streaming.material.clone();
        let grass_overlay_material = streaming.grass_overlay_material.clone();
        let cutout_material = streaming.cutout_material.clone();
        let water_material = streaming.water_material.clone();
        if let Some(rendered) = streaming.rendered.get_mut(&position) {
            apply_chunk_meshes(
                &mut commands,
                &mut meshes,
                rendered,
                layers,
                &material,
                &grass_overlay_material,
                &cutout_material,
                &water_material,
            );
        } else {
            let rendered = spawn_chunk(
                &mut commands,
                &mut meshes,
                position,
                layers,
                &streaming.material,
                &streaming.grass_overlay_material,
                &streaming.cutout_material,
                &streaming.water_material,
            );
            streaming.rendered.insert(position, rendered);
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
        let smooth_lighting = streaming.smooth_lighting;
        let fancy_graphics = streaming.fancy_graphics;
        let west = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z,
            })
            .map(|chunk| chunk.chunk.clone());
        let east = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z,
            })
            .map(|chunk| chunk.chunk.clone());
        let north = chunks
            .get(ChunkPos {
                x: position.x,
                z: position.z - 1,
            })
            .map(|chunk| chunk.chunk.clone());
        let south = chunks
            .get(ChunkPos {
                x: position.x,
                z: position.z + 1,
            })
            .map(|chunk| chunk.chunk.clone());
        let northwest = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z - 1,
            })
            .map(|chunk| chunk.chunk.clone());
        let northeast = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z - 1,
            })
            .map(|chunk| chunk.chunk.clone());
        let southwest = chunks
            .get(ChunkPos {
                x: position.x - 1,
                z: position.z + 1,
            })
            .map(|chunk| chunk.chunk.clone());
        let southeast = chunks
            .get(ChunkPos {
                x: position.x + 1,
                z: position.z + 1,
            })
            .map(|chunk| chunk.chunk.clone());
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
            let layers = mesh_chunk_with_biomes(
                &chunk,
                &skylight,
                &biomes,
                &grass_colors,
                &foliage_colors,
                old_lighting,
                smooth_lighting,
                fancy_graphics,
            );
            (layers, start.elapsed())
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
    meshes: &mut Assets<Mesh>,
    position: ChunkPos,
    layers: ChunkMeshes,
    material: &Handle<StandardMaterial>,
    grass_overlay_material: &Handle<StandardMaterial>,
    cutout_material: &Handle<LeafCutoutMaterial>,
    water_material: &Handle<StandardMaterial>,
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
    );
    rendered
}

fn apply_chunk_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    rendered: &mut RenderedChunk,
    layers: ChunkMeshes,
    material: &Handle<StandardMaterial>,
    grass_overlay_material: &Handle<StandardMaterial>,
    cutout_material: &Handle<LeafCutoutMaterial>,
    water_material: &Handle<StandardMaterial>,
) {
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.opaque,
        layers.opaque,
        material,
        "Opaque",
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.grass_overlay,
        layers.grass_overlay,
        grass_overlay_material,
        "Grass overlay",
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.cutout,
        layers.cutout,
        cutout_material,
        "Cutout",
    );
    apply_layer(
        commands,
        meshes,
        rendered.entity,
        &mut rendered.water,
        layers.water,
        water_material,
        "Water",
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
) {
    let empty = mesh.count_vertices() == 0;
    match (layer.take(), empty) {
        (Some(existing), false) => {
            if let Some(mut current) = meshes.get_mut(existing.mesh.id()) {
                *current = mesh;
            }
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

fn despawn_rendered_chunk(
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

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures::check_ready;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::DroppedItem;
use crate::entity::drops::items::DroppedItemState;
use crate::entity::drops::items::ItemMotion;
use crate::entity::drops::items::PickupAnimation;
use crate::entity::drops::items::chunk_record;
use crate::entity::drops::items::spawn_saved_item;
use crate::player::Player;
use crate::world::persistence::WorldPersistence;

use super::ChunkJob;
use super::GENERATE_MARGIN;
use super::StreamingDiagnostics;
use super::WorldStreaming;
use super::mesh_jobs::mesh_neighborhood_ready;
use super::mesh_jobs::spawn_mesh_job;
use super::positions_in_radius;
use super::render::ChunkMaterials;
use super::render::apply_sections;
use super::render::despawn_rendered_chunk;
use super::render::spawn_chunk;
use super::within_radius;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::generation::WorldGenerator;
use crate::world::textures::AlphaMaskMaterial;
use crate::world::textures::CutoutMaterial;
use crate::world::textures::FoliageColors;
use crate::world::textures::GrassColors;
use crate::world::textures::GrassOverlayMaterial;
use crate::world::textures::TerrainMaterial;
use crate::world::textures::WaterMaterial;

/// Jobs of each kind kept in flight per async compute thread. A job that
/// finishes mid-frame waits for the next streaming pass to be replaced, so a
/// second queued job keeps the thread busy meanwhile.
const JOBS_PER_THREAD: usize = 2;
/// Limit snapshot work on the main thread when many chunks need rebuilding.
const MAX_REMESH_PER_FRAME: usize = 16;

pub(crate) fn setup_streaming(
    mut commands: Commands,
    terrain_material: Res<TerrainMaterial>,
    grass_overlay_material: Res<GrassOverlayMaterial>,
    cutout_material: Res<CutoutMaterial>,
    water_material: Res<WaterMaterial>,
    mask_material: Res<AlphaMaskMaterial>,
    grass_colors: Res<GrassColors>,
    foliage_colors: Res<FoliageColors>,
    settings: Res<GameSettings>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut chunks: ResMut<WorldChunks>,
    mut perf: ResMut<StreamingDiagnostics>,
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
        .and_then(|storage| storage.load_chunk(ChunkPosition::ZERO));
    let mut generated = match stored {
        Some(chunk) => {
            perf.load.record(load_start.elapsed());
            chunk
        }
        None => {
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPosition::ZERO);
            }
            let generate_start = Instant::now();
            let chunk = generator.generate(ChunkPosition::ZERO);
            perf.generate.record(generate_start.elapsed());
            chunk
        }
    };
    let materials = ChunkMaterials([
        terrain_material.0.clone(),
        grass_overlay_material.0.clone(),
        cutout_material.0.clone(),
        water_material.0.clone(),
        mask_material.0.clone(),
    ]);
    let max_in_flight = (AsyncComputeTaskPool::get().thread_num() * JOBS_PER_THREAD).max(2);
    let saved_items = std::mem::take(&mut generated.items);
    chunks.insert(ChunkPosition::ZERO, generated);
    for item in saved_items {
        spawn_saved_item(&mut commands, item);
    }
    commands.insert_resource(WorldStreaming {
        generator,
        grass_colors: grass_colors.clone(),
        foliage_colors: foliage_colors.clone(),
        generating: HashMap::new(),
        meshing: HashMap::new(),
        // The spawn chunk has block data for the player's heightmap, but its
        // first mesh waits for all eight neighboring chunks below.
        rendered: HashMap::new(),
        materials,
        fancy_graphics: settings.graphics.fancy_leaves(),
        remesh_queue: VecDeque::new(),
        remesh_sections: HashMap::new(),
        desired_generation: Vec::new(),
        desired_meshing: Vec::new(),
        desired_center: None,
        desired_radius: 0,
        max_in_flight,
    });
}

/// Regenerate every loaded chunk from the world generator when F4 is pressed.
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
    if keys.is_some_and(|keys| keys.just_pressed(KeyCode::F4)) {
        let count = chunks.positions().count();
        streaming.generating.clear();
        streaming.remesh_everything();
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
    mut perf: ResMut<StreamingDiagnostics>,
    dropped: Query<
        (
            Entity,
            &Transform,
            &DroppedItem,
            &ItemMotion,
            &DroppedItemState,
        ),
        Without<PickupAnimation>,
    >,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPosition::from_world(player.translation.x, player.translation.z);
    let load_radius = settings.render_distance;
    let generate_radius = load_radius + GENERATE_MARGIN;
    let unload_radius = generate_radius;

    // Lighting settings and the time of day are block material uniforms; only
    // the leaf style changes geometry.
    if streaming.fancy_graphics != settings.graphics.fancy_leaves() {
        streaming.fancy_graphics = settings.graphics.fancy_leaves();
        // In-flight meshes were built with the previous leaf style.
        streaming.remesh_everything();
    }

    // Dropping an unfinished task cancels work that is no longer useful.
    streaming
        .generating
        .retain(|position, _| within_radius(*position, center, generate_radius));
    streaming
        .meshing
        .retain(|position, _| within_radius(*position, center, load_radius));
    let mut forgotten = Vec::new();

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
    streaming.remesh_queue.retain(|position| {
        let keep = within_radius(*position, center, load_radius);
        if !keep {
            forgotten.push(*position);
        }
        keep
    });
    for position in forgotten {
        streaming.remesh_sections.remove(&position);
    }

    // Stored chunks are kept for the whole generation radius, including the ring
    // that is generated ahead of the render distance.
    let stale: Vec<_> = chunks
        .positions()
        .filter(|position| !within_radius(*position, center, generate_radius))
        .collect();
    for position in stale {
        if let Some(mut chunk) = chunks.remove(position) {
            let mut leaving = Vec::new();
            for (entity, transform, dropped, motion, state) in &dropped {
                let item_chunk = ChunkPosition::from_block(
                    transform.translation.x.floor() as i32,
                    transform.translation.z.floor() as i32,
                );
                if item_chunk == position {
                    chunk.items.push(chunk_record(
                        dropped.0,
                        transform.translation,
                        motion.0,
                        state,
                    ));
                    leaving.push(entity);
                }
            }
            for entity in leaving {
                commands.entity(entity).despawn();
            }
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.queue_unload(position, chunk);
            }
        }
    }

    // Apply finished generation jobs. Decoration is part of generation, so a
    // chunk is only eligible for meshing once it lands in `chunks`.
    let generated: Vec<_> = streaming
        .generating
        .iter_mut()
        .filter_map(|(position, task)| check_ready(task).map(|job| (*position, job)))
        .collect();
    for (position, mut job) in generated {
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
            let saved_items = std::mem::take(&mut job.chunk.items);
            chunks.insert(position, job.chunk);
            if job.loaded {
                for item in saved_items {
                    spawn_saved_item(&mut commands, item);
                }
            }
        }
    }

    // Apply finished mesh jobs. A chunk that is already rendered (for example
    // after a regeneration) keeps its entity and handle; only the geometry is
    // replaced.
    let meshed: Vec<_> = streaming
        .meshing
        .iter_mut()
        .filter_map(|(position, job)| check_ready(&mut job.task).map(|job| (*position, job)))
        .collect();
    for (position, job) in meshed {
        streaming.meshing.remove(&position);
        perf.mesh.record(job.elapsed);
        if !within_radius(position, center, load_radius) {
            continue;
        }
        let materials = streaming.materials.clone();
        let rendered = streaming
            .rendered
            .entry(position)
            .or_insert_with(|| spawn_chunk(&mut commands, position));
        rendered.fingerprints = job.fingerprints;
        apply_sections(
            &mut commands,
            &mut meshes,
            rendered,
            job.sections,
            &materials,
        );
    }

    // Edits take priority over first meshes. Only snapshot chunk data here;
    // lighting and mesh construction run on the compute pool.
    let max_in_flight = streaming.max_in_flight;
    let remesh_attempts = streaming.remesh_queue.len().min(MAX_REMESH_PER_FRAME);
    for _ in 0..remesh_attempts {
        if streaming.meshing.len() >= max_in_flight {
            break;
        }
        let Some(position) = streaming.remesh_queue.pop_front() else {
            break;
        };
        if !streaming.rendered.contains_key(&position) {
            streaming.remesh_sections.remove(&position);
            continue;
        }
        let forced = streaming
            .remesh_sections
            .get(&position)
            .copied()
            .unwrap_or_default();
        if spawn_mesh_job(&mut streaming, &chunks, position, forced) {
            streaming.remesh_sections.remove(&position);
        } else {
            streaming.remesh_queue.push_back(position);
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
    // Chunks already on disk are loaded instead of regenerated, unless F4 asked
    // for a from-scratch pass.
    let bypass_load = persistence
        .as_deref()
        .is_some_and(WorldPersistence::bypass_load);
    let mut in_flight = streaming.generating.len();
    let mut to_generate = Vec::new();
    for position in &streaming.desired_generation {
        if in_flight >= max_in_flight {
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

    // Fill remaining mesh slots with new chunks after pending edit remeshes.
    let to_mesh: Vec<_> = streaming
        .desired_meshing
        .iter()
        .copied()
        .filter(|position| {
            !streaming.rendered.contains_key(position)
                && !streaming.meshing.contains_key(position)
                && mesh_neighborhood_ready(&chunks, *position)
        })
        .take(max_in_flight.saturating_sub(streaming.meshing.len()))
        .collect();
    for position in to_mesh {
        spawn_mesh_job(&mut streaming, &chunks, position, 0);
    }
}

pub(super) fn sort_by_distance(positions: &mut [ChunkPosition], center: ChunkPosition) {
    positions.sort_by_key(|position| {
        let dx = i64::from(position.x) - i64::from(center.x);
        let dz = i64::from(position.z) - i64::from(center.z);
        (dx * dx + dz * dz, position.x, position.z)
    });
}

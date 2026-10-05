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
use crate::entity::SavedBody;
use crate::entity::SavedBodyData;
use crate::entity::SavedBodyFilter;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::drops::items::DroppedItemState;
use crate::entity::drops::items::ItemMotion;
use crate::entity::drops::items::PickupAnimation;
use crate::entity::drops::items::chunk_record;
use crate::entity::drops::items::spawn_saved_item;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobRecord;
use crate::entity::mobs::spawn_saved;
use crate::player::Player;
use crate::world::block_ticks::BlockTicks;
use crate::world::lighting::LightCache;
use crate::world::persistence::WorldPersistence;

use super::ChunkCulling;
use super::ChunkJob;
use super::GENERATE_MARGIN;
use super::PopulationJob;
use super::StreamingDiagnostics;
use super::UNLOAD_MARGIN;
use super::WorldStreaming;
use super::mesh_jobs::mesh_neighborhood_ready;
use super::mesh_jobs::spawn_mesh_job;
use super::population_footprint;
use super::positions_in_radius;
use super::render::ChunkMaterials;
use super::render::apply_sections;
use super::render::despawn_rendered_chunk;
use super::render::spawn_chunk;
use super::within_radius;
use crate::rendering::textures::AlphaMaskMaterial;
use crate::rendering::textures::CutoutMaterial;
use crate::rendering::textures::FoliageColors;
use crate::rendering::textures::GrassColors;
use crate::rendering::textures::GrassOverlayMaterial;
use crate::rendering::textures::MeshWireframe;
use crate::rendering::textures::TerrainMaterial;
use crate::rendering::textures::WaterMaterial;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::WorldChunks;
use crate::world::generation::overworld::OverworldGenerator;

/// Jobs of each kind kept in flight per async compute thread. A job that
/// finishes mid-frame waits for the next streaming pass to be replaced, so a
/// second queued job keeps the thread busy meanwhile.
const JOBS_PER_THREAD: usize = 2;
/// Limit snapshot work on the main thread when many chunks need rebuilding.
const MAX_REMESH_PER_FRAME: usize = 16;

pub(crate) fn setup_streaming(
    mut commands: Commands,
    generation: Option<Res<crate::world::generation::WorldGeneration>>,
    // Grouped to stay within Bevy's sixteen system parameters.
    (terrain_material, grass_overlay_material, cutout_material, water_material, mask_material): (
        Res<TerrainMaterial>,
        Res<GrassOverlayMaterial>,
        Res<CutoutMaterial>,
        Res<WaterMaterial>,
        Res<AlphaMaskMaterial>,
    ),
    culling: Option<Res<ChunkCulling>>,
    grass_colors: Res<GrassColors>,
    foliage_colors: Res<FoliageColors>,
    settings: Res<GameSettings>,
    wireframe: Res<MeshWireframe>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut chunks: ResMut<WorldChunks>,
    mut perf: ResMut<StreamingDiagnostics>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut light: Option<ResMut<LightCache>>,
) {
    // Mesh jobs light chunks from here on, so block ticks must not.
    if let Some(light) = light.as_deref_mut() {
        light.set_streamed(true);
    }
    let seed = persistence
        .as_ref()
        .map_or(0, |persistence| persistence.seed());
    let generator = generation.map_or_else(
        || {
            Arc::new(OverworldGenerator::new(seed))
                as Arc<dyn crate::world::generation::ChunkGenerator>
        },
        |generation| Arc::clone(&generation.0),
    );
    // The player starts in PostStartup and needs this heightmap immediately, so
    // the chunk it will stand in is loaded or generated synchronously: where a
    // saved player left off, or the origin for a new one.
    let storage = persistence
        .as_ref()
        .and_then(|persistence| persistence.storage())
        .cloned();
    let spawn_chunk = storage
        .as_ref()
        .and_then(|storage| storage.load_player())
        .map_or(ChunkPosition::ZERO, |player| {
            ChunkPosition::from_world(player.x, player.z)
        });
    let load_start = Instant::now();
    let stored = storage
        .as_ref()
        .and_then(|storage| storage.load_chunk(spawn_chunk));
    let spawn_area = match stored {
        Some(chunk) => {
            perf.load.record(load_start.elapsed());
            vec![(spawn_chunk, chunk, true)]
        }
        None => {
            // A new spawn chunk is only finished once its neighbors' population
            // passes have run, so build its whole neighborhood now. Saved
            // neighbors are kept rather than overwritten.
            let generate_start = Instant::now();
            let area = crate::world::generation::generate_area(generator.as_ref(), spawn_chunk, 0);
            perf.generate.record(generate_start.elapsed());
            area.into_iter()
                .map(|(position, generated)| {
                    let stored = (position != spawn_chunk)
                        .then(|| storage.as_ref()?.load_chunk(position))
                        .flatten();
                    match stored {
                        Some(stored) => (position, stored, true),
                        None => (position, generated, false),
                    }
                })
                .collect()
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
    for (position, generated, loaded) in spawn_area {
        if !loaded && let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(position);
        }
        admit_chunk(
            &mut commands,
            &mut chunks,
            ticks.as_deref_mut(),
            position,
            generated,
        );
    }
    commands.insert_resource(WorldStreaming {
        generator,
        grass_colors: grass_colors.clone(),
        foliage_colors: foliage_colors.clone(),
        generating: HashMap::new(),
        populating: HashMap::new(),
        held: HashMap::new(),
        meshing: HashMap::new(),
        // The spawn chunk has block data for the player's heightmap, but its
        // first mesh waits for all eight neighboring chunks below.
        rendered: HashMap::new(),
        materials,
        // The renderer picks this from the GPU's features. A headless app
        // has no renderer and keeps the default.
        culling: culling.map_or_else(ChunkCulling::default, |culling| *culling),
        fancy_graphics: settings.graphics.fancy_leaves(),
        wireframe_block: wireframe.block,
        remesh_queue: VecDeque::new(),
        remesh_sections: HashMap::new(),
        desired_generation: Vec::new(),
        desired_meshing: Vec::new(),
        desired_center: None,
        desired_radius: 0,
        discovery_dirty: true,
        max_in_flight,
        halted: false,
    });
}

pub(crate) fn stream_chunks(
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut streaming: ResMut<WorldStreaming>,
    mut chunks: ResMut<WorldChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    settings: Res<GameSettings>,
    wireframe: Res<MeshWireframe>,
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
    // Paired to stay within Bevy's sixteen system parameters.
    (mobs, bodies): (
        Query<(Entity, &Transform, &Velocity, &Mob, Option<&Living>)>,
        Query<SavedBodyData, SavedBodyFilter>,
    ),
    mut last_unload_sweep: Local<Option<(ChunkPosition, i32)>>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut light: Option<ResMut<LightCache>>,
    mut discovered_revision: Local<Option<u64>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPosition::from_world(player.translation.x, player.translation.z);
    let load_radius = settings.render_distance;
    let generate_radius = load_radius + GENERATE_MARGIN;
    // Chunk data outlives the generation radius by a ring, so pacing over a
    // chunk border does not unload and reload a row of chunks each way.
    let unload_radius = generate_radius + UNLOAD_MARGIN;

    // Lighting settings and the time of day are block material uniforms.
    // Leaf style and a wireframe block filter change geometry.
    if streaming.fancy_graphics != settings.graphics.fancy_leaves() {
        streaming.fancy_graphics = settings.graphics.fancy_leaves();
        // In-flight meshes were built with the previous leaf style.
        streaming.remesh_everything();
    }
    if streaming.wireframe_block != wireframe.block {
        streaming.wireframe_block = wireframe.block;
        streaming.remesh_everything();
    }

    // Dropping an unfinished task cancels work that is no longer useful.
    streaming
        .generating
        .retain(|position, _| within_radius(*position, center, generate_radius));
    // A chunk that is still shown keeps its remeshes after it leaves the
    // render distance. Its mesh would otherwise stay stale when the player
    // turns back before it expires.
    {
        let streaming = &mut *streaming;
        let rendered = &streaming.rendered;
        streaming.meshing.retain(|position, _| {
            within_radius(*position, center, load_radius) || rendered.contains_key(position)
        });
    }
    // Chunks only leave the load/unload/generate radii when the player
    // crosses a chunk boundary or the render distance setting changes, so
    // skip these O(loaded-chunks) sweeps on the many frames in between.
    if *last_unload_sweep != Some((center, load_radius)) {
        *last_unload_sweep = Some((center, load_radius));

        let mut forgotten = Vec::new();

        let expired: Vec<_> = streaming
            .rendered
            .keys()
            .copied()
            .filter(|position| !within_radius(*position, center, generate_radius))
            .collect();
        for position in expired {
            // Its cached light stays until the chunk unloads: block ticks at
            // the render edge read it, and a Beta world saves it.
            if let Some(rendered) = streaming.rendered.remove(&position) {
                despawn_rendered_chunk(&mut commands, &mut meshes, rendered);
            }
        }
        {
            let streaming = &mut *streaming;
            let rendered = &streaming.rendered;
            streaming.remesh_queue.retain(|position| {
                let keep = within_radius(*position, center, load_radius)
                    || rendered.contains_key(position);
                if !keep {
                    forgotten.push(*position);
                }
                keep
            });
        }
        for position in forgotten {
            streaming.remesh_sections.remove(&position);
        }

        // Stored chunks are kept for the whole generation radius, including the
        // ring that is generated ahead of the render distance, and one ring
        // more before they unload.
        let stale: Vec<_> = chunks
            .positions()
            .filter(|position| !within_radius(*position, center, unload_radius))
            .collect();
        for position in stale {
            if let Some(mut chunk) = chunks.remove(position) {
                if let Some(ticks) = ticks.as_deref_mut() {
                    ticks.unload_chunk(position, &mut chunk.chunk);
                    // Pending ticks only survive in the saved chunk.
                    if !chunk.chunk.pending_ticks().is_empty()
                        && let Some(persistence) = persistence.as_deref_mut()
                    {
                        persistence.mark_dirty(position);
                    }
                }
                let cells = light
                    .as_deref_mut()
                    .and_then(|light| light.remove(position));
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
                let mut saved_mobs = Vec::new();
                for (entity, transform, velocity, mob, living) in &mobs {
                    if ChunkPosition::from_world(transform.translation.x, transform.translation.z)
                        == position
                    {
                        saved_mobs.push(MobRecord::capture(
                            mob,
                            transform.translation,
                            velocity.0,
                            living,
                        ));
                        commands.entity(entity).despawn();
                    }
                }
                // Records left by the last autosave are stale: mobs that died
                // or walked off since would come back on the next load.
                if !saved_mobs.is_empty() || !chunk.chunk.mob_records().is_empty() {
                    chunk.chunk.set_mob_records(saved_mobs);
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(position);
                    }
                }
                let mut saved_bodies = Vec::new();
                for (entity, transform, falling, tnt, velocity) in &bodies {
                    if ChunkPosition::from_world(transform.translation.x, transform.translation.z)
                        == position
                        && let Some(body) = SavedBody::capture(transform, falling, tnt, velocity)
                    {
                        saved_bodies.push(body);
                        commands.entity(entity).despawn();
                    }
                }
                if !saved_bodies.is_empty() || !chunk.chunk.saved_bodies().is_empty() {
                    chunk.chunk.set_saved_bodies(saved_bodies);
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(position);
                    }
                }
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.queue_unload_lit(position, chunk, cells);
                }
            }
        }
    }

    // Apply finished population jobs. Their chunks rejoin the world even if
    // the player has moved away, so the next unload pass saves what the
    // population pass wrote into them.
    let populated: Vec<_> = streaming
        .populating
        .iter_mut()
        .filter_map(|(source, task)| check_ready(task).map(|job| (*source, job)))
        .collect();
    for (source, job) in populated {
        streaming.discovery_dirty = true;
        streaming.populating.remove(&source);
        perf.populate.record(job.elapsed);
        for (position, generated) in population_footprint(source).into_iter().zip(job.chunks) {
            streaming.held.remove(&position);
            if !within_radius(position, center, unload_radius) {
                // A job may return outside the radii after the last sweep.
                *last_unload_sweep = None;
            }
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(position);
            }
            chunks.insert(position, generated);
        }
    }

    // Apply finished generation jobs. A chunk is only eligible for meshing once
    // it and its neighbors are populated.
    let generated: Vec<_> = streaming
        .generating
        .iter_mut()
        .filter_map(|(position, task)| check_ready(task).map(|job| (*position, job)))
        .collect();
    for (position, job) in generated {
        streaming.discovery_dirty = true;
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
            admit_chunk(
                &mut commands,
                &mut chunks,
                ticks.as_deref_mut(),
                position,
                job.chunk,
            );
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
        streaming.discovery_dirty = true;
        streaming.meshing.remove(&position);
        perf.mesh.record(job.elapsed);
        if !within_radius(position, center, load_radius)
            && !streaming.rendered.contains_key(&position)
        {
            continue;
        }
        let materials = streaming.materials.clone();
        let culling = streaming.culling;
        let rendered = streaming
            .rendered
            .entry(position)
            .or_insert_with(|| spawn_chunk(&mut commands, position));
        rendered.fingerprints = job.fingerprints;
        if let Some(light) = light.as_deref_mut() {
            light.insert(position, job.light);
        }
        apply_sections(
            &mut commands,
            &mut meshes,
            rendered,
            job.sections,
            &materials,
            culling,
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

    if streaming.halted || screen.is_some_and(|state| *state.get() != AppScreen::Playing) {
        return;
    }

    let desired_changed =
        streaming.desired_center != Some(center) || streaming.desired_radius != load_radius;
    if !desired_changed
        && !streaming.discovery_dirty
        && *discovered_revision == Some(chunks.membership_revision())
    {
        return;
    }
    perf.discovery_passes += 1;
    if desired_changed {
        streaming.desired_generation = positions_in_radius(center, generate_radius);
        sort_by_distance(&mut streaming.desired_generation, center);
        streaming.desired_meshing = positions_in_radius(center, load_radius);
        sort_by_distance(&mut streaming.desired_meshing, center);
        streaming.desired_center = Some(center);
        streaming.desired_radius = load_radius;
    }

    // Generate base terrain out to the generation margin. Chunks already on
    // disk are loaded instead of regenerated, unless a regeneration pass asked
    // to build them from scratch.
    let bypass_load = persistence
        .as_deref()
        .is_some_and(WorldPersistence::bypass_load);
    let mut in_flight = streaming.generating.len();
    let mut to_generate = Vec::new();
    let mut waiting_on_save = false;
    for position in &streaming.desired_generation {
        if in_flight >= max_in_flight {
            break;
        }
        if chunks.contains(*position)
            || streaming.generating.contains_key(position)
            || streaming.meshing.contains_key(position)
            || streaming.held.contains_key(position)
        {
            continue;
        }
        // The disk holds this chunk's older state until the write lands.
        if persistence
            .as_deref()
            .is_some_and(|persistence| persistence.is_saving(*position))
        {
            waiting_on_save = true;
            continue;
        }
        to_generate.push(*position);
        in_flight += 1;
    }
    let spawned = !to_generate.is_empty();
    for position in to_generate {
        // A chunk that unloaded with unsaved changes is newer in memory than
        // on disk, where it may not exist at all.
        if !bypass_load
            && let Some(chunk) = persistence
                .as_deref_mut()
                .and_then(|persistence| persistence.take_pending(position))
        {
            admit_chunk(
                &mut commands,
                &mut chunks,
                ticks.as_deref_mut(),
                position,
                chunk,
            );
            continue;
        }
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
                chunk: generator.generate_base(position),
                loaded: false,
                elapsed: start.elapsed(),
            }
        });
        streaming.generating.insert(position, task);
    }

    // Populate each unpopulated chunk whose `+x`, `+z`, and `+x+z` neighbors
    // are loaded, nearest first, as Beta does when those chunks arrive. A job
    // takes its chunks with it, so overlapping passes wait their turn.
    let candidates: Vec<_> = streaming
        .desired_generation
        .iter()
        .copied()
        .filter(|source| {
            chunks
                .get(*source)
                .is_some_and(|generated| !generated.populated)
        })
        .collect();
    for source in candidates {
        if streaming.populating.len() >= max_in_flight {
            break;
        }
        let footprint = population_footprint(source);
        if !footprint.iter().all(|position| {
            within_radius(*position, center, generate_radius) && chunks.contains(*position)
        }) {
            continue;
        }
        let taken = footprint.map(|position| chunks.remove(position).unwrap());
        for (position, generated) in footprint.iter().zip(&taken) {
            streaming.held.insert(*position, generated.populated);
        }
        let generator = Arc::clone(&streaming.generator);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let start = Instant::now();
            PopulationJob {
                chunks: generator.populate(source, taken),
                elapsed: start.elapsed(),
            }
        });
        streaming.populating.insert(source, task);
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
                && mesh_neighborhood_ready(&chunks, &streaming.held, *position)
        })
        .take(max_in_flight.saturating_sub(streaming.meshing.len()))
        .collect();
    for position in to_mesh {
        spawn_mesh_job(&mut streaming, &chunks, position, 0);
    }
    *discovered_revision = Some(chunks.membership_revision());
    // Look again next frame for a chunk that waited on its write.
    streaming.discovery_dirty = waiting_on_save;
}

/// Put a chunk into the live world: its pending block ticks go to the
/// scheduler, and the items, mobs and bodies saved with it become entities.
fn admit_chunk(
    commands: &mut Commands,
    chunks: &mut WorldChunks,
    ticks: Option<&mut BlockTicks>,
    position: ChunkPosition,
    mut generated: GeneratedChunk,
) {
    let saved_items = std::mem::take(&mut generated.items);
    let saved_mobs = generated.chunk.take_mob_records();
    let saved_bodies = generated.chunk.take_saved_bodies();
    if let Some(ticks) = ticks {
        ticks.load_chunk(position, &mut generated.chunk);
    }
    chunks.insert(position, generated);
    for item in saved_items {
        spawn_saved_item(commands, item);
    }
    for mob in saved_mobs {
        spawn_saved(commands, mob);
    }
    for body in saved_bodies {
        body.spawn(commands);
    }
}

pub(super) fn sort_by_distance(positions: &mut [ChunkPosition], center: ChunkPosition) {
    positions.sort_by_key(|position| {
        let dx = i64::from(position.x) - i64::from(center.x);
        let dz = i64::from(position.z) - i64::from(center.z);
        (dx * dx + dz * dz, position.x, position.z)
    });
}

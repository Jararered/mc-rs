use std::thread;
use std::time::Duration;
use std::time::Instant;

use bevy::asset::AssetPlugin;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoCpuCulling;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::block::blocks::Block;
use game::player::Player;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::plugin::WorldPlugin;
use game::world::streaming::ChunkCulling;
use game::world::streaming::GENERATE_MARGIN;
use game::world::streaming::LOAD_RADIUS;
use game::world::streaming::WorldStreaming;
use game::world::streaming::positions_in_radius;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app
}

fn run_until(
    app: &mut App,
    timeout: Duration,
    mut condition: impl FnMut(&mut App) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition(app) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(5));
        app.update();
    }
}

fn rendered_positions(app: &mut App) -> Vec<ChunkPosition> {
    let mut query = app.world_mut().query::<&ChunkPosition>();
    query.iter(app.world()).copied().collect()
}

/// Section layer mesh entities of every rendered chunk.
fn layer_entities(app: &mut App) -> Vec<Entity> {
    let mut layers = app.world_mut().query::<(Entity, &Mesh3d, &Name)>();
    layers
        .iter(app.world())
        .filter(|(_, _, name)| {
            matches!(
                name.as_str(),
                "Opaque" | "Grass overlay" | "Cutout" | "Water" | "Alpha-masked geometry"
            )
        })
        .map(|(entity, _, _)| entity)
        .collect()
}

fn block_at(app: &App, position: ChunkPosition, x: usize, y: usize, z: usize) -> Option<Block> {
    app.world()
        .resource::<WorldChunks>()
        .get(position)
        .and_then(|chunk| chunk.chunk.get(x, y, z))
}

fn top_solid(chunk: &Chunk) -> (usize, usize, usize) {
    for y in (0..CHUNK_HEIGHT).rev() {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if chunk.get(x, y, z) != Some(Block::Air) {
                    return (x, y, z);
                }
            }
        }
    }
    (0, 0, 0)
}

#[test]
fn distant_chunks_release_world_data_entities_and_meshes() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(128.0, 80.0, 0.0)));

    app.update();

    assert!(
        app.world()
            .resource::<WorldChunks>()
            .get(ChunkPosition::ZERO)
            .is_none()
    );
    assert_eq!(
        app.world()
            .resource::<WorldStreaming>()
            .rendered_mesh_count(),
        0
    );
    assert_eq!(rendered_positions(&mut app).len(), 0);

    let destination = ChunkPosition { x: 8, z: 0 };
    // Generation and meshing are separate stages, so wait for the rendered
    // entity rather than just the stored chunk data.
    assert!(run_until(&mut app, Duration::from_secs(3), |app| {
        rendered_positions(app).contains(&destination)
    }));
    assert!(
        app.world()
            .resource::<WorldChunks>()
            .get(destination)
            .is_some()
    );
}

#[test]
fn generation_runs_one_ring_ahead_of_meshing() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    // A chunk on the generation ring is stored but never rendered, because it
    // sits outside the render distance.
    let ring = ChunkPosition {
        x: LOAD_RADIUS + GENERATE_MARGIN,
        z: 0,
    };
    assert!(run_until(&mut app, Duration::from_secs(20), |app| {
        app.world().resource::<WorldChunks>().get(ring).is_some()
    }));
    assert!(!rendered_positions(&mut app).contains(&ring));

    let edge = ChunkPosition {
        x: LOAD_RADIUS,
        z: 0,
    };
    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        rendered_positions(app).contains(&edge)
    }));
    let chunks = app.world().resource::<WorldChunks>();
    for dx in -1..=1 {
        for dz in -1..=1 {
            assert!(
                chunks.contains(ChunkPosition {
                    x: edge.x + dx,
                    z: edge.z + dz,
                }),
                "render-distance edge chunk meshed without neighbor ({dx}, {dz})"
            );
        }
    }
}

#[test]
fn spawn_chunk_waits_for_all_neighbor_block_data_before_its_first_mesh() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    app.update();
    assert!(
        !rendered_positions(&mut app).contains(&ChunkPosition::ZERO),
        "the spawn chunk must not mesh before its neighbors are loaded"
    );
    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        rendered_positions(app).contains(&ChunkPosition::ZERO)
    }));
    let layer_entities = layer_entities(&mut app);
    assert!(
        !layer_entities.is_empty(),
        "rendered chunks need mesh bounds"
    );
    for entity in layer_entities {
        let layer = app.world().entity(entity);
        assert!(layer.contains::<Aabb>());
        assert!(layer.contains::<NoAutoAabb>());
        assert!(!layer.contains::<NoFrustumCulling>());
        assert!(
            !layer.contains::<NoCpuCulling>(),
            "without a renderer, layers keep the main-world frustum test"
        );
    }
    let chunks = app.world().resource::<WorldChunks>();
    for dx in -1..=1 {
        for dz in -1..=1 {
            assert!(
                chunks.contains(ChunkPosition { x: dx, z: dz }),
                "missing neighbor ({dx}, {dz}) when the spawn chunk was meshed"
            );
        }
    }
}

#[test]
fn chunk_culling_follows_the_driver_unless_forced() {
    assert_eq!(ChunkCulling::choose(false, None), ChunkCulling::Cpu);
    assert_eq!(ChunkCulling::choose(true, None), ChunkCulling::Gpu);
    assert_eq!(ChunkCulling::choose(false, Some("gpu")), ChunkCulling::Gpu);
    assert_eq!(ChunkCulling::choose(true, Some("cpu")), ChunkCulling::Cpu);
    assert_eq!(ChunkCulling::choose(true, Some("other")), ChunkCulling::Gpu);
}

#[test]
fn gpu_culled_layers_skip_the_main_world_frustum_test() {
    let mut app = test_app();
    app.insert_resource(ChunkCulling::Gpu);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        rendered_positions(app).contains(&ChunkPosition::ZERO)
    }));
    let layer_entities = layer_entities(&mut app);
    assert!(!layer_entities.is_empty());
    for entity in layer_entities {
        let layer = app.world().entity(entity);
        assert!(layer.contains::<NoCpuCulling>());
        // The GPU pass culls against these bounds.
        assert!(layer.contains::<Aabb>());
    }
}

#[test]
fn repeated_remesh_request_promotes_chunk_without_duplicates() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    let first = ChunkPosition::ZERO;
    let second = ChunkPosition { x: 1, z: 0 };
    let third = ChunkPosition { x: 0, z: 1 };
    assert!(run_until(&mut app, Duration::from_secs(10), |app| {
        let rendered = rendered_positions(app);
        [first, second, third]
            .into_iter()
            .all(|position| rendered.contains(&position))
    }));

    {
        let mut streaming = app.world_mut().resource_mut::<WorldStreaming>();
        streaming.request_remesh(first);
        streaming.request_remesh(second);
        streaming.request_remesh(third);
        streaming.request_remesh(second);

        assert_eq!(
            streaming.queued_remesh_positions().collect::<Vec<_>>(),
            vec![second, third, first]
        );

        streaming.request_remesh(third);
        streaming.request_remesh(third);

        assert_eq!(
            streaming.queued_remesh_positions().collect::<Vec<_>>(),
            vec![third, second, first]
        );
    }

    // The streaming pass pops from the front, so these promoted requests are
    // the next remeshes considered for dispatch.
    app.update();
    assert!(app.world().resource::<WorldStreaming>().meshing_job_count() > 0);
}

#[test]
fn edited_chunk_remesh_is_dispatched_without_main_thread_meshing() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));
    let origin = ChunkPosition::ZERO;
    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        rendered_positions(app).contains(&origin)
    }));
    assert!(run_until(&mut app, Duration::from_secs(20), |app| {
        app.world().resource::<WorldStreaming>().meshing_job_count() == 0
    }));

    let (x, y, z) = {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let chunk = chunks.get_mut(origin).unwrap();
        let (x, y, z) = top_solid(&chunk.chunk);
        chunk.chunk.set(x, y, z, Block::Air);
        (x, y, z)
    };
    app.world_mut()
        .resource_mut::<WorldStreaming>()
        .request_remesh(origin);
    app.update();
    assert_eq!(block_at(&app, origin, x, y, z), Some(Block::Air));
    assert!(app.world().resource::<WorldStreaming>().meshing_job_count() > 0);

    // A second edit before the first job is applied must replace that job.
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .get_mut(origin)
        .unwrap()
        .chunk
        .set(x, y, z, Block::Stone);
    app.world_mut()
        .resource_mut::<WorldStreaming>()
        .request_remesh(origin);
    app.update();
    assert_eq!(block_at(&app, origin, x, y, z), Some(Block::Stone));
    assert!(app.world().resource::<WorldStreaming>().meshing_job_count() > 0);
}

#[test]
fn generation_radius_is_two_rings_beyond_the_render_distance() {
    let center = ChunkPosition { x: 3, z: -2 };
    let generated = positions_in_radius(center, LOAD_RADIUS + GENERATE_MARGIN);
    let rendered = positions_in_radius(center, LOAD_RADIUS);
    assert_eq!(GENERATE_MARGIN, 2);
    assert_eq!(rendered.len(), 81);
    assert_eq!(generated.len(), 169);
    for position in &rendered {
        assert!(generated.contains(position));
    }
}

#[test]
fn settled_streaming_skips_discovery_and_wakes_for_chunk_changes_and_movement() {
    use game::world::streaming::StreamingDiagnostics;
    let mut app = test_app();
    let player = app
        .world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)))
        .id();
    app.update();
    assert!(run_until(&mut app, Duration::from_secs(20), |app| {
        let streaming = app.world().resource::<WorldStreaming>();
        streaming.generating_job_count() == 0
            && streaming.populating_job_count() == 0
            && streaming.meshing_job_count() == 0
            && streaming.rendered_mesh_count() == ((LOAD_RADIUS * 2 + 1).pow(2) as usize)
    }));
    let passes = app
        .world()
        .resource::<StreamingDiagnostics>()
        .discovery_passes;
    for _ in 0..40 {
        app.update();
    }
    assert_eq!(
        app.world()
            .resource::<StreamingDiagnostics>()
            .discovery_passes,
        passes
    );

    let ring = ChunkPosition {
        x: LOAD_RADIUS + GENERATE_MARGIN,
        z: 0,
    };
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .remove(ring)
        .unwrap();
    app.update();
    assert!(
        app.world()
            .resource::<StreamingDiagnostics>()
            .discovery_passes
            > passes
    );
    assert!(run_until(&mut app, Duration::from_secs(10), |app| {
        app.world().resource::<WorldChunks>().contains(ring)
    }));

    let passes = app
        .world()
        .resource::<StreamingDiagnostics>()
        .discovery_passes;
    app.world_mut()
        .get_mut::<Transform>(player)
        .unwrap()
        .translation
        .x += 16.0;
    app.update();
    assert!(
        app.world()
            .resource::<StreamingDiagnostics>()
            .discovery_passes
            > passes
    );
}

#[derive(Clone)]
struct TestGenerator {
    bases: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    populations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl game::world::generation::ChunkGenerator for TestGenerator {
    fn generate_base(&self, _: ChunkPosition) -> game::world::chunk::GeneratedChunk {
        use game::world::biome::Biome;
        use game::world::biome::BiomeMap;
        use game::world::biome::Climate;
        use game::world::chunk::GeneratedChunk;
        use game::world::chunk::Heightmap;
        self.bases
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut chunk = Chunk::new();
        chunk.set(0, 40, 0, Block::Obsidian);
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            chunk,
            biomes: BiomeMap::from_cells(
                [Climate {
                    biome: Biome::Plains,
                    temperature: 0.5,
                    humidity: 0.5,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            items: Vec::new(),
            populated: false,
        }
    }

    fn populate(
        &self,
        _: ChunkPosition,
        mut chunks: [game::world::chunk::GeneratedChunk; 4],
    ) -> [game::world::chunk::GeneratedChunk; 4] {
        self.populations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Leave a marker in every member of the footprint, to exercise
        // neighboring writes and the shared finished-neighborhood rules.
        for generated in &mut chunks {
            generated.chunk.set(1, 40, 1, Block::GoldBlock);
            generated.heightmap = game::world::chunk::Heightmap::from_chunk(&generated.chunk);
        }
        chunks[0].populated = true;
        chunks
    }
}

fn test_generator() -> TestGenerator {
    TestGenerator {
        bases: Default::default(),
        populations: Default::default(),
    }
}

#[test]
fn reusable_area_generation_populates_a_consistent_neighborhood() {
    use game::world::generation::generate_area;
    use game::world::generation::population_footprint;
    use std::sync::atomic::Ordering;

    let generator = test_generator();
    let center = ChunkPosition { x: -7, z: 12 };
    let area = generate_area(&generator, center, 0);
    assert_eq!(area.len(), 9);
    assert_eq!(generator.bases.load(Ordering::Relaxed), 9);
    assert_eq!(generator.populations.load(Ordering::Relaxed), 4);
    assert!(area[&center].populated);
    assert!(
        area.values()
            .all(|generated| generated.chunk.get(1, 40, 1) == Some(Block::GoldBlock))
    );
    assert!(
        !area[&ChunkPosition {
            x: center.x + 1,
            z: center.z + 1
        }]
            .populated
    );
    assert_eq!(
        population_footprint(center),
        [
            center,
            ChunkPosition { x: -6, z: 12 },
            ChunkPosition { x: -7, z: 13 },
            ChunkPosition { x: -6, z: 13 }
        ]
    );
}

#[test]
fn streaming_uses_selected_backend_for_spawn_and_background_jobs() {
    use game::world::generation::WorldGeneration;
    use std::sync::atomic::Ordering;

    let generator = test_generator();
    let mut app = test_app();
    app.insert_resource(WorldGeneration::new(generator.clone()));
    app.update();
    assert_eq!(
        block_at(&app, ChunkPosition::ZERO, 0, 40, 0),
        Some(Block::Obsidian)
    );
    assert_eq!(
        block_at(&app, ChunkPosition::ZERO, 1, 40, 1),
        Some(Block::GoldBlock)
    );
    assert_eq!(generator.bases.load(Ordering::Relaxed), 9);
    assert_eq!(generator.populations.load(Ordering::Relaxed), 4);

    let center = ChunkPosition { x: 6, z: -4 };
    app.world_mut()
        .spawn((Player, Transform::from_xyz(96.0, 65.0, -64.0)));
    assert!(run_until(&mut app, Duration::from_secs(10), |app| {
        app.world()
            .resource::<WorldStreaming>()
            .neighborhood_finished(app.world().resource::<WorldChunks>(), center)
    }));
    assert_eq!(block_at(&app, center, 0, 40, 0), Some(Block::Obsidian));
    assert_eq!(block_at(&app, center, 1, 40, 1), Some(Block::GoldBlock));
    assert!(generator.bases.load(Ordering::Relaxed) > 9);
    assert!(generator.populations.load(Ordering::Relaxed) > 4);
}

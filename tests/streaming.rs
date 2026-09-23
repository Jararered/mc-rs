use std::thread;
use std::time::Duration;
use std::time::Instant;

use bevy::asset::AssetPlugin;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use game::player::Player;
use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::plugin::WorldPlugin;
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
        .add_plugins(WorldPlugin);
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

fn rendered_positions(app: &mut App) -> Vec<ChunkPos> {
    let mut query = app.world_mut().query::<&ChunkPos>();
    query.iter(app.world()).copied().collect()
}

fn block_at(app: &App, position: ChunkPos, x: usize, y: usize, z: usize) -> Option<BlockId> {
    app.world()
        .resource::<WorldChunks>()
        .get(position)
        .and_then(|chunk| chunk.chunk.get(x, y, z))
}

fn top_solid(chunk: &Chunk) -> (usize, usize, usize) {
    for y in (0..CHUNK_HEIGHT).rev() {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if chunk.get(x, y, z) != Some(BlockId::Air) {
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
            .get(ChunkPos::ZERO)
            .is_none()
    );
    assert_eq!(
        app.world()
            .resource::<WorldStreaming>()
            .rendered_mesh_count(),
        0
    );
    assert_eq!(rendered_positions(&mut app).len(), 0);

    let destination = ChunkPos { x: 8, z: 0 };
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
    let ring = ChunkPos {
        x: LOAD_RADIUS + GENERATE_MARGIN,
        z: 0,
    };
    assert!(run_until(&mut app, Duration::from_secs(20), |app| {
        app.world().resource::<WorldChunks>().get(ring).is_some()
    }));
    assert!(!rendered_positions(&mut app).contains(&ring));

    let edge = ChunkPos {
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
                chunks.contains(ChunkPos {
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
        !rendered_positions(&mut app).contains(&ChunkPos::ZERO),
        "the spawn chunk must not mesh before its neighbors are loaded"
    );
    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        rendered_positions(app).contains(&ChunkPos::ZERO)
    }));
    let mut layers = app.world_mut().query::<(Entity, &Mesh3d, &Name)>();
    let layer_entities: Vec<_> = layers
        .iter(app.world())
        .filter(|(_, _, name)| {
            matches!(
                name.as_str(),
                "Opaque" | "Grass overlay" | "Cutout" | "Water" | "Plants"
            )
        })
        .map(|(entity, _, _)| entity)
        .collect();
    assert!(
        !layer_entities.is_empty(),
        "rendered chunks need mesh bounds"
    );
    for entity in layer_entities {
        let layer = app.world().entity(entity);
        assert!(layer.contains::<Aabb>());
        assert!(layer.contains::<NoAutoAabb>());
        assert!(!layer.contains::<NoFrustumCulling>());
    }
    let chunks = app.world().resource::<WorldChunks>();
    for dx in -1..=1 {
        for dz in -1..=1 {
            assert!(
                chunks.contains(ChunkPos { x: dx, z: dz }),
                "missing neighbor ({dx}, {dz}) when the spawn chunk was meshed"
            );
        }
    }
}

#[test]
fn repeated_remesh_request_promotes_chunk_without_duplicates() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    let first = ChunkPos::ZERO;
    let second = ChunkPos { x: 1, z: 0 };
    let third = ChunkPos { x: 0, z: 1 };
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
    let origin = ChunkPos::ZERO;
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
        chunk.chunk.set(x, y, z, BlockId::Air);
        (x, y, z)
    };
    app.world_mut()
        .resource_mut::<WorldStreaming>()
        .request_remesh(origin);
    app.update();
    assert_eq!(block_at(&app, origin, x, y, z), Some(BlockId::Air));
    assert!(app.world().resource::<WorldStreaming>().meshing_job_count() > 0);

    // A second edit before the first job is applied must replace that job.
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .get_mut(origin)
        .unwrap()
        .chunk
        .set(x, y, z, BlockId::Stone);
    app.world_mut()
        .resource_mut::<WorldStreaming>()
        .request_remesh(origin);
    app.update();
    assert_eq!(block_at(&app, origin, x, y, z), Some(BlockId::Stone));
    assert!(app.world().resource::<WorldStreaming>().meshing_job_count() > 0);
}

#[test]
fn pressing_f4_regenerates_loaded_chunks_from_scratch() {
    let mut app = test_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(8.0, 80.0, 8.0)));

    let origin = ChunkPos::ZERO;
    assert!(run_until(&mut app, Duration::from_secs(3), |app| {
        rendered_positions(app).contains(&origin)
    }));

    // Corrupt a block in the stored chunk. Regeneration must discard the edit,
    // which a plain re-mesh of the cached chunk would keep.
    let (x, y, z) = {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let chunk = chunks.get_mut(origin).unwrap();
        let (x, y, z) = top_solid(&chunk.chunk);
        chunk.chunk.set(x, y, z, BlockId::Air);
        (x, y, z)
    };
    assert_eq!(block_at(&app, origin, x, y, z), Some(BlockId::Air));

    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::F4);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();

    // The chunk is dropped and regenerated, so the edit disappears while the
    // chunk stays rendered.
    assert!(run_until(&mut app, Duration::from_secs(5), |app| {
        block_at(app, origin, x, y, z).is_some_and(|block| block != BlockId::Air)
    }));
    assert!(rendered_positions(&mut app).contains(&origin));
}

#[test]
fn generation_radius_is_one_ring_beyond_the_render_distance() {
    let center = ChunkPos { x: 3, z: -2 };
    let generated = positions_in_radius(center, LOAD_RADIUS + GENERATE_MARGIN);
    let rendered = positions_in_radius(center, LOAD_RADIUS);
    assert_eq!(GENERATE_MARGIN, 1);
    assert_eq!(rendered.len(), 81);
    assert_eq!(generated.len(), 121);
    for position in &rendered {
        assert!(generated.contains(position));
    }
}

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};

use crate::{
    app::{settings::GameSettings, state::AppScreen},
    player::Player,
};

use super::{
    chunk::{ChunkPos, WorldChunks},
    generation::{GeneratedChunk, WorldGenerator},
    lighting::Skylight,
    meshing::mesh_chunk_with_biomes,
    textures::{GrassColors, TerrainMaterial},
};

pub const LOAD_RADIUS: i32 = 4;
pub const UNLOAD_RADIUS: i32 = LOAD_RADIUS + 1;
const MAX_IN_FLIGHT: usize = 2;

#[derive(Resource)]
pub(crate) struct WorldStreaming {
    generator: Arc<WorldGenerator>,
    grass_colors: GrassColors,
    pending: HashMap<ChunkPos, Task<(GeneratedChunk, Mesh)>>,
    rendered: HashMap<ChunkPos, (Entity, Handle<Mesh>)>,
    material: Handle<StandardMaterial>,
    old_lighting: bool,
    remesh_queue: VecDeque<ChunkPos>,
    desired: Vec<ChunkPos>,
    next_desired: usize,
    desired_center: Option<ChunkPos>,
    desired_radius: i32,
}

pub(crate) fn setup_streaming(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    grass_colors: Res<GrassColors>,
    settings: Res<GameSettings>,
    mut chunks: ResMut<WorldChunks>,
) {
    let generator = Arc::new(WorldGenerator::new(0));
    // The player starts in PostStartup and needs this heightmap immediately.
    let generated = generator.generate(ChunkPos::ZERO);
    let skylight = Skylight::from_chunk(&generated.chunk);
    let mesh = meshes.add(mesh_chunk_with_biomes(
        &generated.chunk,
        &skylight,
        &generated.biomes,
        &grass_colors,
        settings.old_lighting,
    ));
    let material = terrain_material.0.clone();
    let entity = spawn_chunk(&mut commands, ChunkPos::ZERO, &mesh, &material);

    chunks.insert(ChunkPos::ZERO, generated);
    commands.insert_resource(WorldStreaming {
        generator,
        grass_colors: grass_colors.clone(),
        pending: HashMap::new(),
        rendered: HashMap::from([(ChunkPos::ZERO, (entity, mesh))]),
        material,
        old_lighting: settings.old_lighting,
        remesh_queue: VecDeque::new(),
        desired: Vec::new(),
        next_desired: 0,
        desired_center: None,
        desired_radius: 0,
    });
}

pub(crate) fn stream_chunks(
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut streaming: ResMut<WorldStreaming>,
    mut chunks: ResMut<WorldChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    settings: Res<GameSettings>,
    screen: Option<Res<State<AppScreen>>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPos::from_world(player.translation.x, player.translation.z);
    let load_radius = settings.render_distance;
    let unload_radius = load_radius + 1;

    if streaming.old_lighting != settings.old_lighting {
        streaming.old_lighting = settings.old_lighting;
        streaming.pending.clear();
        streaming.desired_center = None;
        streaming.remesh_queue = streaming.rendered.keys().copied().collect();
    }

    // Dropping an unfinished task cancels work that is no longer useful.
    streaming
        .pending
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
            chunks.remove(position);
        }
    }

    // Rebuild one loaded mesh per frame when the old skylight/face shading toggle changes.
    if let Some(position) = streaming.remesh_queue.pop_front()
        && let Some(generated) = chunks.get(position)
        && let Some((_, handle)) = streaming.rendered.get(&position)
    {
        let skylight = Skylight::from_chunk(&generated.chunk);
        let mesh = mesh_chunk_with_biomes(
            &generated.chunk,
            &skylight,
            &generated.biomes,
            &streaming.grass_colors,
            streaming.old_lighting,
        );
        if let Some(mut existing) = meshes.get_mut(handle.id()) {
            *existing = mesh;
        }
    }

    // Limit mesh asset creation and entity spawning to one completed chunk per frame.
    let completed = streaming
        .pending
        .iter_mut()
        .find_map(|(position, task)| check_ready(task).map(|result| (*position, result)));
    if let Some((position, (generated, mesh))) = completed {
        streaming.pending.remove(&position);
        if within_radius(position, center, load_radius) {
            let mesh = meshes.add(mesh);
            let entity = spawn_chunk(&mut commands, position, &mesh, &streaming.material);
            chunks.insert(position, generated);
            streaming.rendered.insert(position, (entity, mesh));
        }
    }

    if screen.is_some_and(|state| *state.get() != AppScreen::Playing) {
        return;
    }

    if streaming.pending.len() >= MAX_IN_FLIGHT {
        return;
    }

    if streaming.desired_center != Some(center) || streaming.desired_radius != load_radius {
        let mut desired = positions_in_radius(center, load_radius);
        desired.sort_by_key(|position| {
            let dx = i64::from(position.x) - i64::from(center.x);
            let dz = i64::from(position.z) - i64::from(center.z);
            (dx * dx + dz * dz, position.x, position.z)
        });
        streaming.desired = desired;
        streaming.next_desired = 0;
        streaming.desired_center = Some(center);
        streaming.desired_radius = load_radius;
    }
    while streaming.next_desired < streaming.desired.len() {
        if streaming.pending.len() >= MAX_IN_FLIGHT {
            break;
        }
        let position = streaming.desired[streaming.next_desired];
        streaming.next_desired += 1;
        if chunks.contains(position) || streaming.pending.contains_key(&position) {
            continue;
        }

        let generator = Arc::clone(&streaming.generator);
        let grass_colors = streaming.grass_colors.clone();
        let old_lighting = streaming.old_lighting;
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let generated = generator.generate(position);
            let skylight = Skylight::from_chunk(&generated.chunk);
            let mesh = mesh_chunk_with_biomes(
                &generated.chunk,
                &skylight,
                &generated.biomes,
                &grass_colors,
                old_lighting,
            );
            (generated, mesh)
        });
        streaming.pending.insert(position, task);
    }
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

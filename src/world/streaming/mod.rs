use std::{collections::HashMap, sync::Arc};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};

use crate::player::Player;

use super::{
    chunk::{ChunkPos, WorldChunks},
    generation::{GeneratedChunk, WorldGenerator},
    lighting::Skylight,
    meshing::mesh_chunk,
    textures::TerrainMaterial,
};

const LOAD_RADIUS: i32 = 2;
const UNLOAD_RADIUS: i32 = 3;
const MAX_IN_FLIGHT: usize = 2;

#[derive(Resource)]
pub(crate) struct WorldStreaming {
    generator: Arc<WorldGenerator>,
    pending: HashMap<ChunkPos, Task<(GeneratedChunk, Mesh)>>,
    rendered: HashMap<ChunkPos, (Entity, Handle<Mesh>)>,
    material: Handle<StandardMaterial>,
}

pub(crate) fn setup_streaming(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_material: Res<TerrainMaterial>,
    mut chunks: ResMut<WorldChunks>,
) {
    let generator = Arc::new(WorldGenerator::new(0));
    // The player starts in PostStartup and needs this heightmap immediately.
    let generated = generator.generate(ChunkPos::ZERO);
    let skylight = Skylight::from_chunk(&generated.chunk);
    let mesh = meshes.add(mesh_chunk(&generated.chunk, &skylight));
    let material = terrain_material.0.clone();
    let entity = spawn_chunk(&mut commands, ChunkPos::ZERO, &mesh, &material);

    chunks.insert(ChunkPos::ZERO, generated);
    commands.insert_resource(WorldStreaming {
        generator,
        pending: HashMap::new(),
        rendered: HashMap::from([(ChunkPos::ZERO, (entity, mesh))]),
        material,
    });
}

pub(crate) fn stream_chunks(
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut streaming: ResMut<WorldStreaming>,
    mut chunks: ResMut<WorldChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPos::from_world(player.translation.x, player.translation.z);

    // Dropping an unfinished task cancels work that is no longer useful.
    streaming
        .pending
        .retain(|position, _| within_radius(*position, center, LOAD_RADIUS));

    let expired: Vec<_> = streaming
        .rendered
        .keys()
        .copied()
        .filter(|position| !within_radius(*position, center, UNLOAD_RADIUS))
        .collect();
    for position in expired {
        if let Some((entity, mesh)) = streaming.rendered.remove(&position) {
            commands.entity(entity).despawn();
            meshes.remove(mesh.id());
            chunks.remove(position);
        }
    }

    // Limit mesh asset creation and entity spawning to one completed chunk per frame.
    let completed = streaming
        .pending
        .iter_mut()
        .find_map(|(position, task)| check_ready(task).map(|result| (*position, result)));
    if let Some((position, (generated, mesh))) = completed {
        streaming.pending.remove(&position);
        if within_radius(position, center, LOAD_RADIUS) {
            let mesh = meshes.add(mesh);
            let entity = spawn_chunk(&mut commands, position, &mesh, &streaming.material);
            chunks.insert(position, generated);
            streaming.rendered.insert(position, (entity, mesh));
        }
    }

    if streaming.pending.len() >= MAX_IN_FLIGHT {
        return;
    }

    let mut desired = positions_in_radius(center, LOAD_RADIUS);
    desired.sort_by_key(|position| {
        let dx = i64::from(position.x) - i64::from(center.x);
        let dz = i64::from(position.z) - i64::from(center.z);
        (dx * dx + dz * dz, position.x, position.z)
    });
    for position in desired {
        if streaming.pending.len() >= MAX_IN_FLIGHT {
            break;
        }
        if chunks.contains(position) || streaming.pending.contains_key(&position) {
            continue;
        }

        let generator = Arc::clone(&streaming.generator);
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let generated = generator.generate(position);
            let skylight = Skylight::from_chunk(&generated.chunk);
            let mesh = mesh_chunk(&generated.chunk, &skylight);
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

fn within_radius(position: ChunkPos, center: ChunkPos, radius: i32) -> bool {
    (i64::from(position.x) - i64::from(center.x)).abs() <= i64::from(radius)
        && (i64::from(position.z) - i64::from(center.z)).abs() <= i64::from(radius)
}

fn positions_in_radius(center: ChunkPos, radius: i32) -> Vec<ChunkPos> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_radius_is_centered_on_the_player_chunk() {
        let center = ChunkPos { x: -2, z: 3 };
        let positions = positions_in_radius(center, LOAD_RADIUS);
        assert_eq!(positions.len(), 25);
        assert!(positions.contains(&center));
        assert!(positions.contains(&ChunkPos { x: -4, z: 1 }));
        assert!(positions.contains(&ChunkPos { x: 0, z: 5 }));
        assert!(!within_radius(
            ChunkPos { x: 2, z: 3 },
            center,
            UNLOAD_RADIUS
        ));
    }
}

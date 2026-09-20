use bevy::prelude::*;

use super::{
    chunk::{ChunkPos, WorldChunks},
    generation::generate_chunk,
    lighting::Skylight,
    meshing::mesh_chunk,
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.53, 0.73, 0.95)))
            .init_resource::<WorldChunks>()
            .add_systems(Startup, spawn_world);
    }
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut chunks: ResMut<WorldChunks>,
) {
    let position = ChunkPos::ZERO;
    let chunk = generate_chunk(position);
    let skylight = Skylight::from_chunk(&chunk);
    let mesh = mesh_chunk(&chunk, &skylight);
    chunks.insert(position, chunk);
    let (origin_x, origin_z) = position.world_origin();

    commands.spawn((
        Name::new("Chunk 0, 0"),
        position,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(origin_x, 0.0, origin_z),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

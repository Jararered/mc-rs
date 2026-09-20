use bevy::prelude::*;

use super::{
    chunk::WorldChunks,
    streaming::{setup_streaming, stream_chunks},
    textures::TerrainTexturePlugin,
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TerrainTexturePlugin)
            .insert_resource(ClearColor(Color::srgb(0.53, 0.73, 0.95)))
            .init_resource::<WorldChunks>()
            .add_systems(Startup, (setup_streaming, spawn_sun))
            .add_systems(Update, stream_chunks);
    }
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

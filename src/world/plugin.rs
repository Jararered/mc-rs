use bevy::prelude::*;

use crate::app::settings::GameSettings;

use super::chunk::WorldChunks;
use super::streaming::regenerate_loaded_chunks;
use super::streaming::setup_streaming;
use super::streaming::stream_chunks;
use super::textures::TerrainTexturePlugin;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TerrainTexturePlugin)
            .insert_resource(ClearColor(Color::srgb(0.53, 0.73, 0.95)))
            .init_resource::<GameSettings>()
            .init_resource::<GlobalAmbientLight>()
            .init_resource::<WorldChunks>()
            .add_systems(Startup, (setup_streaming, spawn_sun))
            .add_systems(
                Update,
                (
                    apply_lighting_settings,
                    (regenerate_loaded_chunks, stream_chunks).chain(),
                ),
            );
    }
}

fn spawn_sun(mut commands: Commands, settings: Res<GameSettings>) {
    commands.spawn((
        DirectionalLight {
            illuminance: if settings.directional_lighting {
                10_000.0
            } else {
                0.0
            },
            shadow_maps_enabled: settings.directional_lighting,
            ..default()
        },
        Transform::from_xyz(3.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn apply_lighting_settings(
    settings: Res<GameSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun: Query<&mut DirectionalLight>,
) {
    if !settings.is_changed() {
        return;
    }
    ambient.brightness = settings.brightness;
    for mut light in &mut sun {
        light.illuminance = if settings.directional_lighting {
            10_000.0
        } else {
            0.0
        };
        light.shadow_maps_enabled = settings.directional_lighting;
    }
}

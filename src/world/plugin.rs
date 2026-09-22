use bevy::camera::Hdr;
use bevy::core_pipeline::prepass::DeferredPrepass;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::pbr::DefaultOpaqueRendererMethod;
use bevy::pbr::ScreenSpaceReflections;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::physics::PhysicsSet;

use super::chunk::WorldChunks;
use super::sky::CelestialCamera;
use super::sky::SkyCamera;
use super::streaming::StreamingPerf;
use super::streaming::regenerate_loaded_chunks;
use super::streaming::setup_streaming;
use super::streaming::stream_chunks;
use super::textures::TerrainTexturePlugin;
use super::tick::WorldTick;
use super::tick::advance_world_tick;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        super::sky::plugin(app);
        super::clouds::plugin(app);
        app.add_plugins(TerrainTexturePlugin)
            .insert_resource(ClearColor(Color::srgb(0.53, 0.73, 0.95)))
            .init_resource::<GameSettings>()
            .init_resource::<GlobalAmbientLight>()
            .init_resource::<WorldChunks>()
            .init_resource::<WorldTick>()
            .init_resource::<StreamingPerf>()
            .add_systems(Startup, (setup_streaming, spawn_sun))
            .add_systems(First, advance_world_tick)
            .add_systems(
                Update,
                (
                    apply_lighting_settings,
                    apply_graphics_pipeline,
                    (regenerate_loaded_chunks, stream_chunks)
                        .chain()
                        .after(PhysicsSet::ApplyInput),
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

pub(super) fn apply_lighting_settings(
    settings: Res<GameSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun: Query<&mut DirectionalLight>,
) {
    if !settings.is_changed() {
        return;
    }
    ambient.brightness = if settings.old_lighting {
        0.0
    } else {
        settings.ambient_light_brightness()
    };
    for mut light in &mut sun {
        light.illuminance = if settings.directional_lighting {
            10_000.0
        } else {
            0.0
        };
        light.shadow_maps_enabled = settings.directional_lighting;
    }
}

/// Ultra water uses Bevy screen-space reflections, which need deferred rendering
/// and MSAA off. Fast/Fancy stay on the forward path.
fn apply_graphics_pipeline(
    mut commands: Commands,
    settings: Res<GameSettings>,
    cameras: Query<
        (Entity, Option<&ScreenSpaceReflections>),
        (With<Camera3d>, Without<SkyCamera>, Without<CelestialCamera>),
    >,
    sky_cameras: Query<Entity, Or<(With<SkyCamera>, With<CelestialCamera>)>>,
    renderer_method: Option<ResMut<DefaultOpaqueRendererMethod>>,
) {
    if !settings.is_changed() {
        return;
    }
    let ultra = settings.graphics.realistic_water();
    for (entity, ssr) in &cameras {
        if ultra && ssr.is_none() {
            commands
                .entity(entity)
                .insert((ScreenSpaceReflections::default(), Msaa::Off, Hdr));
        } else if !ultra && ssr.is_some() {
            commands
                .entity(entity)
                .remove::<ScreenSpaceReflections>()
                .remove::<DepthPrepass>()
                .remove::<DeferredPrepass>()
                .remove::<Hdr>()
                .insert(Msaa::Sample4);
        }
    }
    for entity in &sky_cameras {
        if ultra {
            commands.entity(entity).insert((Msaa::Off, Hdr));
        } else {
            commands
                .entity(entity)
                .remove::<Hdr>()
                .insert(Msaa::Sample4);
        }
    }
    if let Some(mut method) = renderer_method {
        if ultra {
            method.set_to_deferred();
        } else {
            method.set_to_forward();
        }
    }
}

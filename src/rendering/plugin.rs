use bevy::camera::Hdr;
use bevy::core_pipeline::prepass::DeferredPrepass;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::light::DirectionalLightShadowMap;
use bevy::light::PointLightShadowMap;
use bevy::pbr::DefaultOpaqueRendererMethod;
use bevy::pbr::ScreenSpaceReflections;
use bevy::prelude::*;
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::renderer::RenderDevice;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::physics::PhysicsSet;
use crate::ui::screens::panorama::MenuPanoramaCamera;

use super::sky::CelestialCamera;
use super::sky::SkyCamera;
use super::textures::TerrainTexturePlugin;
use crate::world::block_ticks::BlockTickSet;
use crate::world::streaming::ChunkCulling;
use crate::world::streaming::StreamingDiagnostics;
use crate::world::streaming::setup_streaming;
use crate::world::streaming::stream_chunks;

/// Set to `gpu` or `cpu` to force how chunk layers are frustum culled. See
/// [`ChunkCulling`].
const CHUNK_CULLING_ENV: &str = "MC_CHUNK_CULLING";

/// Bevy's `LightPlugin` allocates a point-light cube array and a directional
/// cascade array at these resources' sizes even when no such light exists
/// (24 MiB and 16 MiB per cascade at Bevy's defaults). Nothing in the game is
/// a directional or point light, so both stay at this unused size.
const UNUSED_SHADOW_MAP_SIZE: usize = 16;

/// Client world presentation and background chunk streaming.
pub struct WorldRenderingPlugin;

impl Plugin for WorldRenderingPlugin {
    fn build(&self, app: &mut App) {
        super::sky::plugin(app);
        super::mobs::plugin(app);
        super::creatures::plugin(app);
        super::weather::plugin(app);
        super::clouds::plugin(app);
        app.add_plugins(TerrainTexturePlugin)
            .insert_resource(ClearColor(Color::srgb(0.53, 0.73, 0.95)))
            .init_resource::<GameSettings>()
            .init_resource::<GlobalAmbientLight>()
            .insert_resource(PointLightShadowMap {
                size: UNUSED_SHADOW_MAP_SIZE,
            })
            .insert_resource(DirectionalLightShadowMap {
                size: UNUSED_SHADOW_MAP_SIZE,
            })
            .init_resource::<StreamingDiagnostics>()
            .add_systems(
                Startup,
                setup_streaming.run_if(crate::world::persistence::starts_with_world),
            )
            .add_systems(
                PostUpdate,
                apply_world_camera_activity.before(bevy::camera::CameraUpdateSystems),
            )
            .add_systems(
                Update,
                (
                    choose_chunk_culling.run_if(resource_added::<RenderDevice>),
                    apply_lighting_settings,
                    apply_graphics_pipeline,
                    crate::entity::falling_block::sync_falling_block_rendering.after(BlockTickSet),
                    stream_chunks
                        .chain()
                        .after(PhysicsSet::ApplyInput)
                        .after(BlockTickSet)
                        .run_if(resource_exists::<crate::world::streaming::WorldStreaming>),
                ),
            );
    }
}

pub(super) fn apply_lighting_settings(
    settings: Res<GameSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    if !settings.is_changed() {
        return;
    }
    ambient.brightness = if settings.old_lighting {
        0.0
    } else {
        settings.ambient_light_brightness()
    };
}

/// Decide how chunk layers are culled once the GPU's features are known, and
/// log what Bevy's indirect drawing has to work with on this machine.
fn choose_chunk_culling(
    mut commands: Commands,
    device: Res<RenderDevice>,
    adapter: Res<RenderAdapterInfo>,
) {
    let multi_draw_count = device
        .features()
        .contains(WgpuFeatures::MULTI_DRAW_INDIRECT_COUNT);
    let forced = std::env::var(CHUNK_CULLING_ENV).ok();
    let culling = ChunkCulling::choose(multi_draw_count, forced.as_deref());
    info!(
        "render backend {:?}: multi-draw indirect count {}, chunk culling {culling:?}{}",
        adapter.backend,
        if multi_draw_count {
            "supported"
        } else {
            "unsupported"
        },
        if forced.is_some() {
            format!(" ({CHUNK_CULLING_ENV} set)")
        } else {
            String::new()
        },
    );
    commands.insert_resource(culling);
}

/// Ultra water uses Bevy screen-space reflections, which need deferred rendering
/// and MSAA off. Fast/Fancy stay on the forward path.
///
/// Every camera on the window gets the same [`Msaa`], the UI and title
/// cameras included: Bevy keeps one set of window-sized targets per sample
/// count in use, so a single 4x camera keeps the multisampled ones alive.
fn apply_graphics_pipeline(
    mut commands: Commands,
    settings: Res<GameSettings>,
    cameras: Query<
        (Entity, Option<&ScreenSpaceReflections>),
        (
            With<Camera3d>,
            Without<SkyCamera>,
            Without<CelestialCamera>,
            Without<MenuPanoramaCamera>,
        ),
    >,
    sky_cameras: Query<Entity, Or<(With<SkyCamera>, With<CelestialCamera>)>>,
    all_cameras: Query<(Entity, Option<&Msaa>), With<Camera>>,
    new_cameras: Query<(), Added<Camera>>,
    renderer_method: Option<ResMut<DefaultOpaqueRendererMethod>>,
) {
    // A world's cameras can spawn after the settings change that loading it
    // caused was already seen here.
    if !settings.is_changed() && new_cameras.is_empty() {
        return;
    }
    let ultra = settings.graphics.realistic_water();
    for (entity, ssr) in &cameras {
        if ultra && ssr.is_none() {
            commands
                .entity(entity)
                .insert((ScreenSpaceReflections::default(), Hdr));
        } else if !ultra && ssr.is_some() {
            commands
                .entity(entity)
                .remove::<ScreenSpaceReflections>()
                .remove::<DepthPrepass>()
                .remove::<DeferredPrepass>()
                .remove::<Hdr>();
        }
    }
    for entity in &sky_cameras {
        if ultra {
            commands.entity(entity).insert(Hdr);
        } else {
            commands.entity(entity).remove::<Hdr>();
        }
    }
    let msaa = settings.msaa();
    for (entity, current) in &all_cameras {
        if current != Some(&msaa) {
            commands.entity(entity).insert(msaa);
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

/// Menu cameras render separately; rendering the world behind the menu is
/// wasted GPU work. PostUpdate includes cameras created during Update.
fn apply_world_camera_activity(
    screen: Option<Res<State<AppScreen>>>,
    mut cameras: Query<&mut Camera, (With<Camera3d>, Without<MenuPanoramaCamera>)>,
) {
    let playing = screen.is_none_or(|screen| *screen.get() == AppScreen::Playing);
    for mut camera in &mut cameras {
        if camera.is_active != playing {
            camera.is_active = playing;
        }
    }
}

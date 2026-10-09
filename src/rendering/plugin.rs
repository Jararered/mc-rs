use bevy::light::DirectionalLightShadowMap;
use bevy::light::PointLightShadowMap;
use bevy::prelude::*;
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::renderer::RenderDevice;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::physics::PhysicsSet;
use crate::ui::screens::panorama::MenuPanoramaCamera;

use super::textures::TerrainTexturePlugin;
use crate::world::block_ticks::BlockTickSet;
use crate::world::chunk::ChunkPosition;
use crate::world::streaming::ChunkCulling;
use crate::world::streaming::StreamingDiagnostics;
use crate::world::streaming::setup_streaming;
use crate::world::streaming::stream_chunks;

/// Set to `gpu` or `cpu` to force how chunk layers are frustum culled. See
/// [`ChunkCulling`].
const CHUNK_CULLING_ENV: &str = "MC_CHUNK_CULLING";

/// Bevy's PBR renderer allocates a point-light cube array and a directional
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
            .insert_resource(GlobalAmbientLight {
                brightness: 0.0,
                ..default()
            })
            .insert_resource(PointLightShadowMap {
                size: UNUSED_SHADOW_MAP_SIZE,
            })
            .insert_resource(DirectionalLightShadowMap {
                size: UNUSED_SHADOW_MAP_SIZE,
            })
            .init_resource::<StreamingDiagnostics>()
            .add_systems(PreStartup, disable_gpu_light_clustering)
            .add_systems(
                Startup,
                setup_streaming.run_if(crate::world::persistence::starts_with_world),
            )
            .add_systems(
                PostUpdate,
                apply_world_camera_activity
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            )
            .add_systems(
                Update,
                (
                    choose_chunk_culling.run_if(resource_added::<RenderDevice>),
                    apply_graphics_pipeline,
                    super::falling_block::sync_falling_block_rendering.after(BlockTickSet),
                    stream_chunks
                        .chain()
                        .after(PhysicsSet::ApplyInput)
                        .after(BlockTickSet)
                        .run_if(resource_exists::<crate::world::streaming::WorldStreaming>),
                ),
            );
    }
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

/// `ClusterConfig::None` needs the CPU light path: Bevy 0.19's GPU light
/// path otherwise creates a zero-sized dummy texture. Mesh GPU preprocessing
/// and culling are independent and remain enabled.
fn disable_gpu_light_clustering(
    settings: Option<ResMut<bevy::light::cluster::GlobalClusterSettings>>,
) {
    if let Some(mut settings) = settings {
        settings.gpu_clustering = None;
    }
}

/// Keep all window cameras on one sample count and disable unused light clusters.
fn apply_graphics_pipeline(
    mut commands: Commands,
    settings: Res<GameSettings>,
    all_cameras: Query<(Entity, Option<&Msaa>), With<Camera>>,
    new_cameras: Query<(), Added<Camera>>,
    new_3d_cameras: Query<Entity, Added<Camera3d>>,
) {
    for entity in &new_3d_cameras {
        commands.entity(entity).insert((
            bevy::light::cluster::ClusterConfig::None,
            bevy::core_pipeline::tonemapping::Tonemapping::None,
        ));
    }
    if !settings.is_changed() && new_cameras.is_empty() {
        return;
    }
    let msaa = settings.msaa();
    for (entity, current) in &all_cameras {
        if current != Some(&msaa) {
            commands.entity(entity).insert(msaa);
        }
    }
}

/// Menu cameras render separately; rendering the world behind the menu is
/// wasted GPU work. PostUpdate includes cameras created during Update.
///
/// Chunks are hidden along with the camera. Bevy removes an inactive camera's
/// `RenderVisibleEntities`, and a `NoCpuCulling` mesh is only announced to
/// views when it becomes visible, so a layer that stayed visible would be
/// missing from the camera's list once it is active again.
fn apply_world_camera_activity(
    screen: Option<Res<State<AppScreen>>>,
    mut cameras: Query<&mut Camera, (With<Camera3d>, Without<MenuPanoramaCamera>)>,
    mut chunks: Query<&mut Visibility, With<ChunkPosition>>,
    mut shown: Local<Option<bool>>,
) {
    let playing = screen.is_none_or(|screen| *screen.get() == AppScreen::Playing);
    for mut camera in &mut cameras {
        if camera.is_active != playing {
            camera.is_active = playing;
        }
    }
    // Chunks keep arriving behind a menu, so every frame there hides the new
    // ones; in game only the switch needs a pass.
    if playing && *shown == Some(true) {
        return;
    }
    *shown = Some(playing);
    let visibility = if playing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut chunk in &mut chunks {
        chunk.set_if_neq(visibility);
    }
}

//! F4 draws block meshes as edges instead of filled triangles.
//!
//! The chunk, drop, and falling-block meshes share [`BlockMaterial`]s and a
//! custom vertex layout, so this flips `BlockShading::wireframe` on those
//! assets. The material specialization then sets `PolygonMode::Line`. Sky,
//! clouds, the first-person arm, and UI keep their own materials.

use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::renderer::RenderDevice;

use super::BlockMaterial;
use crate::app::state::AppScreen;
use crate::ui::ChatState;

/// Whether block meshes are drawn as wireframes. F4 toggles this while playing.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshWireframe {
    pub enabled: bool,
}

impl Default for MeshWireframe {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// `PolygonMode::Line` is exposed by the current device.
///
/// Defaults to enabled so a headless toggle test can run. [`detect_line_raster`]
/// replaces it once the render device exists.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct LineRasterSupported(pub bool);

impl Default for LineRasterSupported {
    fn default() -> Self {
        Self(true)
    }
}

pub struct MeshWireframePlugin;

impl Plugin for MeshWireframePlugin {
    fn build(&self, app: &mut App) {
        if app
            .world()
            .get_resource::<Assets<BlockMaterial>>()
            .is_none()
        {
            app.init_asset::<BlockMaterial>();
        }
        app.init_resource::<MeshWireframe>()
            .init_resource::<LineRasterSupported>()
            .add_systems(Update, toggle_mesh_wireframe);
    }
}

pub(super) fn detect_line_raster(app: &mut App) {
    let supported = app
        .get_sub_app(RenderApp)
        .and_then(|render_app| render_app.world().get_resource::<RenderDevice>())
        .is_some_and(|device| device.features().contains(WgpuFeatures::POLYGON_MODE_LINE));
    app.insert_resource(LineRasterSupported(supported));
}

fn toggle_mesh_wireframe(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    chat: Option<Res<ChatState>>,
    screen: Option<Res<State<AppScreen>>>,
    supported: Res<LineRasterSupported>,
    mut mode: ResMut<MeshWireframe>,
    mut materials: ResMut<Assets<BlockMaterial>>,
) {
    let Some(keys) = keys else {
        return;
    };
    if screen.is_some_and(|screen| *screen.get() != AppScreen::Playing)
        || chat.is_some_and(|chat| chat.suppress_controls)
        || !keys.just_pressed(KeyCode::F4)
    {
        return;
    }
    if !supported.0 {
        warn!("Mesh wireframe needs line rasterization, which this GPU does not expose");
        return;
    }

    mode.enabled = !mode.enabled;
    let enabled = mode.enabled;
    for (_, material) in materials.iter_mut() {
        material.extension.wireframe = enabled;
    }
    info!("Mesh wireframe {}", if enabled { "on" } else { "off" });
}

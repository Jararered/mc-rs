use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::ShaderType;
use bevy::shader::Shader;
use bevy::shader::ShaderRef;

const LEAF_WIGGLE_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("7e3c1a90-4b2d-4f86-9c51-a8d0e4b17c22");
const LEAF_WIGGLE_VERTEX_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("c41f8e2b-6a70-4d13-b9e5-2f7c90d4a6b1");
const LEAF_WIGGLE_PREPASS_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("5d9a2c14-8e6f-4b07-a3c8-1e5d7f90b4a6");

/// Fancy/Ultra cutout leaves: standard atlas material plus a vertex wiggle.
pub type LeafCutoutMaterial = ExtendedMaterial<StandardMaterial, LeafWiggle>;

/// GPU uniform for [`LeafWiggle`]. `amplitude` is world units (~blocks).
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct LeafWiggleSettings {
    pub amplitude: f32,
    pub time: f32,
    pub previous_time: f32,
    pub _padding: f32,
}

impl Default for LeafWiggleSettings {
    fn default() -> Self {
        Self {
            amplitude: 0.06,
            time: 0.0,
            previous_time: 0.0,
            _padding: 0.0,
        }
    }
}

/// Vertex displacement for Fancy leaf cubes. Fragment shading stays on
/// [`StandardMaterial`] so atlas UVs, vertex tint, and the cutout mask remain.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct LeafWiggle {
    #[uniform(100)]
    pub settings: LeafWiggleSettings,
}

impl MaterialExtension for LeafWiggle {
    fn vertex_shader() -> ShaderRef {
        LEAF_WIGGLE_VERTEX_SHADER_HANDLE.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        LEAF_WIGGLE_PREPASS_SHADER_HANDLE.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        LEAF_WIGGLE_PREPASS_SHADER_HANDLE.into()
    }
}

pub(super) fn plugin(app: &mut App) {
    // RenderPlugin already `init_asset::<Shader>()` and fills the collection.
    // Calling it again replaces that resource and panics later when old shader
    // indices are used (`index out of bounds: the len is 2 but the index is 2`).
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<LeafCutoutMaterial>::default());
    app.add_systems(Update, update_leaf_wiggle_time);
    load_internal_asset!(
        app,
        LEAF_WIGGLE_SHADER_HANDLE,
        "leaf_wiggle.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        LEAF_WIGGLE_VERTEX_SHADER_HANDLE,
        "leaf_wiggle_vertex.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        LEAF_WIGGLE_PREPASS_SHADER_HANDLE,
        "leaf_wiggle_prepass.wgsl",
        Shader::from_wgsl
    );
}

fn update_leaf_wiggle_time(time: Res<Time>, mut materials: ResMut<Assets<LeafCutoutMaterial>>) {
    let current_time = time.elapsed_secs();
    let previous_time = current_time - time.delta_secs();

    for (_, material) in materials.iter_mut() {
        material.extension.settings.time = current_time;
        material.extension.settings.previous_time = previous_time;
    }
}

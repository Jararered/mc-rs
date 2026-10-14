//! The material of the sun, moon, stars, and sunrise fan, which the world
//! camera draws behind everything else.
//!
//! Its vertex shader puts every vertex on the far plane, so terrain, clouds,
//! and entities cover the sky wherever they are drawn, however far away, and
//! the sky never covers them. Its fragment shader is the instance tint's, so
//! [`tint_tag`](super::tint_tag) colors these bodies as it colors clouds.
//!
//! Bodies are blended, and Bevy sorts blended meshes back to front by their
//! distance plus [`StandardMaterial::depth_bias`]. Every backdrop material
//! carries [`BACKDROP_SORT`] there, so the sky is drawn before water, rain,
//! and anything else blended in front of it. `StandardMaterial` also turns
//! that bias into a depth offset, which [`SkyBackdrop::specialize`] removes.

use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialExtensionKey;
use bevy::pbr::MaterialExtensionPipeline;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::DepthBiasState;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::render::render_resource::SpecializedMeshPipelineError;
use bevy::shader::Shader;
use bevy::shader::ShaderRef;

use super::instance_tint::INSTANCE_TINT_SHADER_HANDLE;

const SKY_BACKDROP_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("5d1e8b47-92c3-4f0a-b6e8-3a7c41d2f905");

/// The `depth_bias` of every backdrop material. Bevy's blended sort key is
/// the view-space depth plus this, most negative first, and nothing in a
/// loaded world is a million blocks away.
pub const BACKDROP_SORT: f32 = -1.0e6;

/// `StandardMaterial` on the far plane, tinted by the entity's `MeshTag`.
pub type SkyMaterial = ExtendedMaterial<StandardMaterial, SkyBackdrop>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct SkyBackdrop {}

impl MaterialExtension for SkyBackdrop {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }

    fn vertex_shader() -> ShaderRef {
        SKY_BACKDROP_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        INSTANCE_TINT_SHADER_HANDLE.into()
    }

    /// The base material has already turned [`BACKDROP_SORT`] into a depth
    /// bias, which would push the far plane around. It only orders the sort.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias = DepthBiasState::default();
        }
        Ok(())
    }
}

pub(super) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<SkyMaterial>::default());
    load_internal_asset!(
        app,
        SKY_BACKDROP_SHADER_HANDLE,
        "sky_backdrop.wesl",
        crate::rendering::wesl_shader
    );
}

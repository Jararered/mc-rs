//! Per-entity color for sky bodies and clouds without touching their materials.
//!
//! The sunrise fan, stars, and clouds change color continuously at dusk and
//! dawn. Writing that into a material asset every frame makes Bevy re-prepare
//! the material. Instead the color rides in [`MeshTag`], a per-instance value
//! Bevy already uploads with each mesh's transform, and a small fragment
//! extension multiplies it into `StandardMaterial`'s base color.

use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::mesh::MeshTag;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::Shader;
use bevy::shader::ShaderRef;

const INSTANCE_TINT_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("2f6b9d31-0c4e-4a57-8e1d-93b5c7a2f408");

/// `StandardMaterial` tinted by the entity's [`MeshTag`].
pub type TintedMaterial = ExtendedMaterial<StandardMaterial, InstanceTint>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct InstanceTint {}

impl MaterialExtension for InstanceTint {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }

    fn fragment_shader() -> ShaderRef {
        INSTANCE_TINT_SHADER_HANDLE.into()
    }
}

/// Encode an sRGB color and linear alpha as the tag the tint shader reads.
///
/// The tag stores the complement, so the default tag of zero is opaque white.
pub fn tint_tag(color: Color) -> MeshTag {
    let srgba = color.to_srgba();
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let packed =
        byte(srgba.red) | byte(srgba.green) << 8 | byte(srgba.blue) << 16 | byte(srgba.alpha) << 24;
    MeshTag(!packed)
}

pub(super) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<TintedMaterial>::default());
    load_internal_asset!(
        app,
        INSTANCE_TINT_SHADER_HANDLE,
        "instance_tint.wgsl",
        Shader::from_wgsl
    );
}

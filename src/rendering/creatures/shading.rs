//! Beta's entity lighting for creature models.
//!
//! `RenderManager` colors each entity by the world brightness at its body,
//! and `RenderHelper.enableStandardItemLighting` shades its faces with two
//! fixed lights. Under old lighting the fragment extension reproduces both;
//! otherwise Bevy's lights do the shading and only the tint applies. The
//! brightness and the sheep's fleece tint ride in [`MeshTag`], so neither
//! rewrites a material.

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

const CREATURE_SHADER_HANDLE: Handle<Shader> = uuid_handle!("8d1e4c7a-52b3-4f69-a0d8-3e6b91c2f574");

/// `StandardMaterial` lit like a Beta entity.
pub type CreatureMaterial = ExtendedMaterial<StandardMaterial, CreatureShading>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct CreatureShading {}

impl MaterialExtension for CreatureShading {
    fn fragment_shader() -> ShaderRef {
        CREATURE_SHADER_HANDLE.into()
    }
}

/// Pack an sRGB tint and a brightness for the creature shader. The tag holds
/// the complement, so an untagged mesh draws white at full brightness.
pub fn creature_tag(tint: Color, brightness: f32) -> MeshTag {
    let srgb = tint.to_srgba();
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let packed =
        byte(srgb.red) | byte(srgb.green) << 8 | byte(srgb.blue) << 16 | byte(brightness) << 24;
    MeshTag(!packed)
}

pub(super) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<CreatureMaterial>::default());
    load_internal_asset!(
        app,
        CREATURE_SHADER_HANDLE,
        "creature.wgsl",
        Shader::from_wgsl
    );
}

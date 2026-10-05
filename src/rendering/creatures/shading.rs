//! Beta's entity lighting for creature models.
//!
//! `RenderManager` colors each entity by the world brightness at its body,
//! and `RenderHelper.enableStandardItemLighting` shades its faces with two
//! fixed lights. Under old lighting the fragment extension reproduces both;
//! otherwise the skin is lit by Bevy's ambient light only. `RenderLiving`'s
//! red hurt pass and a creeper's white flash are mixed over the result. All
//! of this per-entity state rides in [`MeshTag`], so none of it rewrites a
//! material.

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

/// How a creature material draws, as `creature.wgsl` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// A skin, lit like an entity.
    Skin,
    /// Spider eyes: lit but never darkened, faded by the tag's alpha.
    Glow,
    /// A charged creeper's aura: half bright and added on top.
    Charge,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct CreatureShading {
    /// `x`: the [`Pass`]. `y`: texture scroll in UV units per second.
    #[uniform(100)]
    pub params: Vec4,
}

impl CreatureShading {
    pub fn new(pass: Pass, scroll: f32) -> Self {
        let mode = match pass {
            Pass::Skin => 0.0,
            Pass::Glow => 1.0,
            Pass::Charge => 2.0,
        };
        Self {
            params: Vec4::new(mode, scroll, 0.0, 0.0),
        }
    }
}

impl MaterialExtension for CreatureShading {
    fn fragment_shader() -> ShaderRef {
        CREATURE_SHADER_HANDLE.into()
    }
}

/// Pack the world brightness, the red hurt pass, the white flash's opacity,
/// and a layer opacity. The brightness and opacity are stored inverted so an
/// untagged mesh draws at full brightness, unhurt, and opaque.
pub fn creature_tag(brightness: f32, hurt: bool, flash: f32, alpha: f32) -> MeshTag {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let packed = (255 - byte(brightness))
        | u32::from(hurt) * 255 << 8
        | byte(flash) << 16
        | (255 - byte(alpha)) << 24;
    MeshTag(packed)
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

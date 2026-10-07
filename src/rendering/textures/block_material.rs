//! The material every block mesh uses: `StandardMaterial` shading over the
//! packed vertex format from `meshing::vertex`.
//!
//! The vertex shaders decode positions, atlas UVs, normals, and tints, and
//! evaluate Beta's light curve from raw sky and block samples with the
//! uniform below. Changing the time of day or smooth lighting
//! updates this uniform instead of rebuilding meshes. Leaf wiggle reads
//! Bevy's time uniform, so nothing here changes every frame.
//!
//! A greedy rectangle stores an unwrapped block position as its UV and the
//! atlas tile in vertex alpha. The fragment shaders wrap that back into one
//! tile before sampling in the unlit forward pass.

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
use bevy::render::render_resource::PolygonMode;
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::render::render_resource::ShaderType;
use bevy::render::render_resource::SpecializedMeshPipelineError;
use bevy::render::storage::ShaderBuffer;
use bevy::shader::Shader;
use bevy::shader::ShaderDefVal;
use bevy::shader::ShaderRef;

use super::ATLAS_GRID;
use super::ATLAS_PAD_TEXELS;
use super::ATLAS_TILE_PX;
use crate::rendering::meshing::ATTRIBUTE_BLOCK_VERTEX;
use crate::rendering::meshing::ATTRIBUTE_QUAD_CORNER;
use crate::rendering::meshing::BlockLighting;

pub const LEAF_WIGGLE_AMPLITUDE: f32 = 0.06;

const BLOCK_VERTEX_IMPORT_HANDLE: Handle<Shader> =
    uuid_handle!("7e3c1a90-4b2d-4f86-9c51-a8d0e4b17c22");
const BLOCK_VERTEX_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("c41f8e2b-6a70-4d13-b9e5-2f7c90d4a6b1");
const BLOCK_FRAGMENT_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("9c4e2a71-3b58-4d0e-8f16-6a2d91c0e4b7");

/// Atlas-textured block shading shared by terrain layers and dropped blocks.
pub type BlockMaterial = ExtendedMaterial<StandardMaterial, BlockShading>;

/// GPU uniform for [`BlockShading`]. Mirrored in `block_vertex.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, ShaderType, Reflect)]
pub struct BlockShadingSettings {
    pub skylight_subtracted: f32,
    pub flags: u32,
    /// Leaf wiggle in world units (~blocks). Zero for every layer but leaves.
    pub wiggle_amplitude: f32,
    /// Brightness of light level 0: `Dimension::ambient_light`.
    pub ambient: f32,
}

impl BlockShadingSettings {
    pub const SMOOTH_LIGHTING: u32 = 2;

    pub fn new(lighting: BlockLighting, wiggle_amplitude: f32) -> Self {
        let mut flags = 0;
        if lighting.smooth_lighting {
            flags |= Self::SMOOTH_LIGHTING;
        }
        Self {
            skylight_subtracted: f32::from(lighting.skylight_subtracted),
            flags,
            wiggle_amplitude,
            ambient: crate::world::dimension::Dimension::Overworld.ambient_light(),
        }
    }

    pub fn lighting(&self) -> BlockLighting {
        BlockLighting {
            smooth_lighting: self.flags & Self::SMOOTH_LIGHTING != 0,
            skylight_subtracted: self.skylight_subtracted as u8,
        }
    }
}

impl Default for BlockShadingSettings {
    fn default() -> Self {
        Self::new(BlockLighting::default(), 0.0)
    }
}

/// Pipeline permutation for [`BlockShading`]. Line mode draws the mesh edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPipelineKey {
    pub wireframe: bool,
}

impl From<&BlockShading> for BlockPipelineKey {
    fn from(shading: &BlockShading) -> Self {
        Self {
            wireframe: shading.wireframe,
        }
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
#[bind_group_data(BlockPipelineKey)]
pub struct BlockShading {
    #[uniform(100)]
    pub settings: BlockShadingSettings,
    /// Every chunk layer's quad records (`rendering::chunk_quads`). Meshes in
    /// the packed vertex format never read it, but the binding must hold a
    /// buffer for the material to be drawn at all.
    #[storage(101, read_only, visibility(vertex))]
    pub quads: Handle<ShaderBuffer>,
    /// Draw triangle edges instead of filled faces. Not uploaded; it selects
    /// the pipeline via [`BlockPipelineKey`].
    pub wireframe: bool,
}

impl MaterialExtension for BlockShading {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }

    fn vertex_shader() -> ShaderRef {
        BLOCK_VERTEX_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        BLOCK_FRAGMENT_SHADER_HANDLE.into()
    }

    /// Block meshes carry one packed attribute, so Bevy's mesh pipeline finds
    /// none of the standard ones. Bind the packed buffer and declare the UV
    /// and color outputs the vertex shaders fill for `StandardMaterial`. A
    /// chunk layer's proxy mesh carries a quad and corner number instead, and
    /// its shaders read the rest from the quad buffer.
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Filled and line pipelines are different specializations. The key
        // changes with `BlockShading::wireframe`, so both stay cached.
        if key.bind_group_data.wireframe {
            descriptor.primitive.polygon_mode = PolygonMode::Line;
            // A face between dirt and water points at the water. Draw it from
            // the shore as well as from inside the liquid.
            descriptor.primitive.cull_mode = None;
            // Keep overlapping wireframe edges from fighting at equal depth.
            if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
                depth_stencil.bias.slope_scale = 1.0;
            }
        } else {
            descriptor.primitive.polygon_mode = PolygonMode::Fill;
        }
        let pulled = layout.0.contains(ATTRIBUTE_QUAD_CORNER);
        let attribute = if pulled {
            ATTRIBUTE_QUAD_CORNER
        } else {
            ATTRIBUTE_BLOCK_VERTEX
        };
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[attribute.at_shader_location(0)])?];
        if pulled {
            descriptor.vertex.shader_defs.push("PULLED_QUADS".into());
        }
        let defs: [ShaderDefVal; 6] = [
            "VERTEX_UVS".into(),
            "VERTEX_UVS_A".into(),
            "VERTEX_COLORS".into(),
            ShaderDefVal::UInt("ATLAS_GRID".into(), ATLAS_GRID),
            ShaderDefVal::UInt("ATLAS_TILE_PX".into(), ATLAS_TILE_PX),
            ShaderDefVal::UInt("ATLAS_PAD_TEXELS".into(), ATLAS_PAD_TEXELS),
        ];
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend(defs);
        }
        Ok(())
    }
}

pub(super) fn plugin(app: &mut App) {
    // RenderPlugin already `init_asset::<Shader>()` and fills the collection.
    // Calling it again replaces that resource and panics later when old shader
    // indices are used (`index out of bounds: the len is 2 but the index is 2`).
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<BlockMaterial>::default());
    load_internal_asset!(
        app,
        BLOCK_VERTEX_IMPORT_HANDLE,
        "block_vertex.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        BLOCK_VERTEX_SHADER_HANDLE,
        "block_vertex_main.wgsl",
        Shader::from_wgsl
    );
    load_internal_asset!(
        app,
        BLOCK_FRAGMENT_SHADER_HANDLE,
        "block_fragment.wgsl",
        Shader::from_wgsl
    );
}

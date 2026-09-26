//! Block vertices and their 16-byte GPU encoding.
//!
//! The mesher emits [`BlockVertex`] values: positions, atlas texels, biome
//! tint, ambient occlusion, and raw sky and block light samples. Nothing
//! time-of-day dependent is baked in. `block_vertex.wgsl` decodes the packed
//! form and evaluates Beta's brightness curve with the current
//! `skylight_subtracted`, so dusk never rebuilds a mesh.
//!
//! Packed layout, four `u32` words:
//!
//! | word | bits                                                        |
//! |------|-------------------------------------------------------------|
//! | 0    | x: 13, z: 13, octahedral normal x: 6                        |
//! | 1    | y: 12, u (tile 4, texel 5): 9, v (tile 4, texel 5): 9, AO: 2 |
//! | 2    | sRGB tint: 24, octahedral normal y: 6, shade: 1, bright: 1  |
//! | 3    | four light samples, each sky << 4 \| block                  |
//!
//! Positions are relative to the mesh origin in `-8..24`, x and z in 1/256
//! block steps and y in 1/128. Section meshes stay inside that range, and so
//! do dropped block meshes centered on their entity.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::math::Vec3;
use bevy::mesh::Indices;
use bevy::mesh::Mesh;
use bevy::mesh::MeshVertexAttribute;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Color;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::render_resource::VertexFormat;

use crate::world::lighting::beta_brightness;
use crate::world::lighting::combined_light;
use crate::world::lighting::unpack;
use crate::world::textures::ATLAS_GRID;
use crate::world::textures::ATLAS_PAD_TEXELS;
use crate::world::textures::ATLAS_TILE_PX;

/// The only vertex attribute of block meshes: four packed words per vertex.
pub const ATTRIBUTE_BLOCK_VERTEX: MeshVertexAttribute =
    MeshVertexAttribute::new("Block_Vertex", 1_985_470_021, VertexFormat::Uint32x4);

/// Lowest representable coordinate, relative to the mesh origin.
pub const POSITION_MIN: f32 = -8.0;
/// Span of representable coordinates on every axis.
pub const POSITION_SPAN: f32 = 32.0;
const HORIZONTAL_STEPS: f32 = 256.0;
const VERTICAL_STEPS: f32 = 128.0;
/// Beta's lit furnace body is brighter than white. The shader multiplies tints
/// flagged `bright` by this, so the stored tint stays within 0..=1.
pub const BRIGHT_TINT: f32 = 1.12;
/// Face shade multipliers from `RenderBlocks`, keyed by the dominant normal axis.
pub const SHADE_TOP: f32 = 1.0;
pub const SHADE_BOTTOM: f32 = 0.55;
pub const SHADE_X: f32 = 0.6;
pub const SHADE_Z: f32 = 0.8;
/// A light sample from a block that emits full light, regardless of the sky.
pub const FULL_BRIGHT: [u8; 4] = [15; 4];

/// A point in an atlas tile, in Beta texels. `texel` runs `0..=16` so a quad
/// can reach the far edge of its tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasTexel {
    pub tile: [u8; 2],
    pub texel: [u8; 2],
}

impl AtlasTexel {
    pub const fn new(tile_x: u8, tile_y: u8, texel_u: u8, texel_v: u8) -> Self {
        Self {
            tile: [tile_x, tile_y],
            texel: [texel_u, texel_v],
        }
    }

    /// Normalized UV in the padded atlas, as `atlas_tile_uvs` lays tiles out.
    /// Texels are Beta texels, so HD packs scale with the atlas.
    pub fn uv(self) -> [f32; 2] {
        let stride = (ATLAS_TILE_PX + 2 * ATLAS_PAD_TEXELS) as f32;
        let atlas = ATLAS_GRID as f32 * stride;
        let pad = ATLAS_PAD_TEXELS as f32;
        std::array::from_fn(|axis| {
            (f32::from(self.tile[axis]) * stride + pad + f32::from(self.texel[axis])) / atlas
        })
    }
}

/// One corner of a block quad before packing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub texel: AtlasTexel,
    /// Linear RGB. Components above 1 are only produced by `BRIGHT_TINT`.
    pub tint: [f32; 3],
    /// Four light samples, each packed as `sky << 4 | block`. Flat lighting
    /// uses the first; smooth lighting averages all four.
    pub light: [u8; 4],
    /// Ambient occlusion level, 0 (open) to 3 (enclosed).
    pub ao: u8,
    /// Apply Beta's per-face shade under old lighting.
    pub shade: bool,
}

/// How the block shader turns vertex data into a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockLighting {
    pub old_lighting: bool,
    pub smooth_lighting: bool,
    pub skylight_subtracted: u8,
}

impl Default for BlockLighting {
    fn default() -> Self {
        Self {
            old_lighting: true,
            smooth_lighting: true,
            skylight_subtracted: 0,
        }
    }
}

impl BlockVertex {
    /// The color `block_vertex.wgsl` computes for this vertex, before 8-bit
    /// tint quantization. Alpha comes from the material.
    pub fn color(&self, lighting: BlockLighting) -> [f32; 4] {
        let light = if lighting.old_lighting {
            let brightness = |sample: u8| {
                let (sky, block) = unpack(sample);
                beta_brightness(combined_light(sky, block, lighting.skylight_subtracted))
            };
            let level = if lighting.smooth_lighting {
                self.light.iter().copied().map(brightness).sum::<f32>() * 0.25
            } else {
                brightness(self.light[0])
            };
            if self.shade {
                level * face_shade(self.normal)
            } else {
                level
            }
        } else {
            1.0
        };
        let occlusion = if lighting.smooth_lighting {
            1.0 - f32::from(self.ao) * 0.2
        } else {
            1.0
        };
        let factor = light * occlusion;
        [
            self.tint[0] * factor,
            self.tint[1] * factor,
            self.tint[2] * factor,
            1.0,
        ]
    }

    pub fn pack(&self) -> [u32; 4] {
        let [x, y, z] = self.position;
        let horizontal = |value: f32| {
            debug_assert!(
                (POSITION_MIN..POSITION_MIN + POSITION_SPAN).contains(&value),
                "block vertex coordinate {value} is outside the packed range"
            );
            ((value - POSITION_MIN) * HORIZONTAL_STEPS)
                .round()
                .clamp(0.0, 8191.0) as u32
        };
        let vertical = ((y - POSITION_MIN) * VERTICAL_STEPS)
            .round()
            .clamp(0.0, 4095.0) as u32;
        debug_assert!((POSITION_MIN..POSITION_MIN + POSITION_SPAN).contains(&y));
        let [normal_x, normal_y] = oct_encode(self.normal);
        let texel = |axis: usize| {
            (u32::from(self.texel.tile[axis]) & 0xf) << 5 | u32::from(self.texel.texel[axis]) & 0x1f
        };
        let bright = self.tint.iter().any(|channel| *channel > 1.0);
        let tint = if bright {
            self.tint.map(|channel| channel / BRIGHT_TINT)
        } else {
            self.tint
        };
        let srgb = Color::linear_rgb(tint[0], tint[1], tint[2]).to_srgba();
        let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
        [
            horizontal(x) | horizontal(z) << 13 | normal_x << 26,
            vertical | texel(0) << 12 | texel(1) << 21 | u32::from(self.ao.min(3)) << 30,
            byte(srgb.red)
                | byte(srgb.green) << 8
                | byte(srgb.blue) << 16
                | normal_y << 24
                | u32::from(self.shade) << 30
                | u32::from(bright) << 31,
            u32::from_le_bytes(self.light),
        ]
    }
}

/// Decoded fields of a packed vertex, for tests and debugging.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedFields {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub texel: AtlasTexel,
    pub tint_srgb: [u8; 3],
    pub light: [u8; 4],
    pub ao: u8,
    pub shade: bool,
    pub bright: bool,
}

pub fn unpack_vertex(words: [u32; 4]) -> PackedFields {
    let horizontal = |bits: u32| (bits & 0x1fff) as f32 / HORIZONTAL_STEPS + POSITION_MIN;
    let texel = |bits: u32| ((bits >> 5 & 0xf) as u8, (bits & 0x1f) as u8);
    let (tile_u, texel_u) = texel(words[1] >> 12);
    let (tile_v, texel_v) = texel(words[1] >> 21);
    PackedFields {
        position: [
            horizontal(words[0]),
            (words[1] & 0xfff) as f32 / VERTICAL_STEPS + POSITION_MIN,
            horizontal(words[0] >> 13),
        ],
        normal: oct_decode([words[0] >> 26 & 0x3f, words[2] >> 24 & 0x3f]),
        texel: AtlasTexel::new(tile_u, tile_v, texel_u, texel_v),
        tint_srgb: [
            words[2] as u8,
            (words[2] >> 8) as u8,
            (words[2] >> 16) as u8,
        ],
        light: words[3].to_le_bytes(),
        ao: (words[1] >> 30) as u8,
        shade: words[2] >> 30 & 1 == 1,
        bright: words[2] >> 31 == 1,
    }
}

/// Beta shade for a face with this normal, by its dominant axis.
pub fn face_shade(normal: [f32; 3]) -> f32 {
    let [x, y, z] = normal.map(f32::abs);
    if y >= x && y >= z {
        if normal[1] > 0.0 {
            SHADE_TOP
        } else {
            SHADE_BOTTOM
        }
    } else if x >= z {
        SHADE_X
    } else {
        SHADE_Z
    }
}

/// Octahedral normal with six bits per axis, stored as `round(v * 31) + 31` so
/// the axis directions and zero decode exactly.
fn oct_encode(normal: [f32; 3]) -> [u32; 2] {
    let [x, y, z] = normal;
    let length = x.abs() + y.abs() + z.abs();
    if length == 0.0 {
        return [31, 31];
    }
    let (mut u, mut v) = (x / length, y / length);
    if z < 0.0 {
        (u, v) = (
            (1.0 - v.abs()) * sign_not_zero(u),
            (1.0 - u.abs()) * sign_not_zero(v),
        );
    }
    [u, v].map(|value| ((value.clamp(-1.0, 1.0) * 31.0).round() as i32 + 31) as u32)
}

fn oct_decode(bits: [u32; 2]) -> [f32; 3] {
    let [u, v] = bits.map(|value| (value as f32 - 31.0) / 31.0);
    let mut normal = Vec3::new(u, v, 1.0 - u.abs() - v.abs());
    if normal.z < 0.0 {
        let folded_x = (1.0 - normal.y.abs()) * sign_not_zero(normal.x);
        let folded_y = (1.0 - normal.x.abs()) * sign_not_zero(normal.y);
        normal.x = folded_x;
        normal.y = folded_y;
    }
    normal.normalize_or_zero().to_array()
}

fn sign_not_zero(value: f32) -> f32 {
    if value >= 0.0 { 1.0 } else { -1.0 }
}

/// Quads of block vertices, four per quad in winding order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockGeometry {
    vertices: Vec<BlockVertex>,
}

impl BlockGeometry {
    pub fn push_quad(&mut self, quad: [BlockVertex; 4]) {
        self.vertices.extend_from_slice(&quad);
    }

    pub fn vertices(&self) -> &[BlockVertex] {
        &self.vertices
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    pub fn index_count(&self) -> usize {
        self.vertices.len() / 4 * 6
    }

    pub fn positions(&self) -> Vec<[f32; 3]> {
        self.vertices.iter().map(|vertex| vertex.position).collect()
    }

    pub fn normals(&self) -> Vec<[f32; 3]> {
        self.vertices.iter().map(|vertex| vertex.normal).collect()
    }

    pub fn uvs(&self) -> Vec<[f32; 2]> {
        self.vertices
            .iter()
            .map(|vertex| vertex.texel.uv())
            .collect()
    }

    pub fn colors(&self, lighting: BlockLighting) -> Vec<[f32; 4]> {
        self.vertices
            .iter()
            .map(|vertex| vertex.color(lighting))
            .collect()
    }

    /// Bounds of the vertex positions, grown by `padding` on every side.
    pub fn aabb(&self, padding: f32) -> Option<Aabb> {
        let first = self.vertices.first()?;
        let (min, max) = self.vertices.iter().fold(
            (Vec3::from(first.position), Vec3::from(first.position)),
            |(min, max), vertex| {
                let position = Vec3::from(vertex.position);
                (min.min(position), max.max(position))
            },
        );
        Some(Aabb::from_min_max(
            min - Vec3::splat(padding),
            max + Vec3::splat(padding),
        ))
    }

    /// Pack into a render-world-only mesh. Indices are 16-bit whenever the
    /// vertex count allows, which every section short of a solid block of
    /// fancy leaves does.
    pub fn into_mesh(self) -> Mesh {
        let quads = self.vertices.len() / 4;
        let packed: Vec<[u32; 4]> = self.vertices.iter().map(BlockVertex::pack).collect();
        let indices = if self.vertices.len() <= usize::from(u16::MAX) + 1 {
            Indices::U16(
                (0..quads as u16)
                    .flat_map(|quad| {
                        let start = quad * 4;
                        [start, start + 1, start + 2, start, start + 2, start + 3]
                    })
                    .collect(),
            )
        } else {
            Indices::U32(
                (0..quads as u32)
                    .flat_map(|quad| {
                        let start = quad * 4;
                        [start, start + 1, start + 2, start, start + 2, start + 3]
                    })
                    .collect(),
            )
        };
        Mesh::new(
            PrimitiveTopology::TriangleList,
            // Chunk block data remains in WorldChunks. After upload, Bevy can
            // release this mesh's CPU vertex and index buffers; a remesh
            // replaces the asset at the same handle with fresh geometry.
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            ATTRIBUTE_BLOCK_VERTEX,
            VertexAttributeValues::Uint32x4(packed),
        )
        .with_inserted_indices(indices)
    }
}

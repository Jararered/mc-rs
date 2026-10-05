//! One 32-byte record per block quad, for chunk layers.
//!
//! A [`BlockVertex`](super::BlockVertex) mesh spends 76 bytes on a quad: four
//! vertices that repeat its tint, tile, normal, and flags, plus six indices.
//! A [`QuadRecord`] stores the quad once. Chunk layers keep their records in
//! one storage buffer and draw a shared proxy mesh whose vertices only name a
//! quad and a corner; `block_vertex.wgsl` (`decode_block_quad`) rebuilds the
//! vertex from the record. Dropped and falling blocks still use the vertex
//! format.
//!
//! Record layout, eight `u32` words:
//!
//! | word | bits                                                              |
//! |------|-------------------------------------------------------------------|
//! | 0    | origin x: 11, origin y: 11, origin z (low): 10                    |
//! | 1    | origin z (high): 1, edge 1 x: 12, y: 12, shade, repeat, snow side, triangle, bright |
//! | 2    | edge 1 z: 12, edge 2 x: 12, tile x: 4, tile y: 4                  |
//! | 3    | edge 2 y: 12, edge 2 z: 12, normal mode: 3, texel u: 5            |
//! | 4    | texel v: 5, texel edge 1 u: 6, v: 6, texel edge 2 u: 6, v: 6      |
//! | 5    | sRGB tint: 24, light diagonal 3: 8                                |
//! | 6    | light centre, mids 0..=2                                          |
//! | 7    | light mid 3, diagonals 0..=2                                      |
//!
//! Corners are `o`, `o + e1`, `o + e1 + e2`, `o + e2`, in 1/64 block steps
//! from [`POSITION_MIN`](super::vertex::POSITION_MIN). Texels follow the same
//! affine rule. A quad that is not a parallelogram in both (a fluid surface
//! with four different heights) becomes two records with the triangle flag,
//! which draw only their first three corners.
//!
//! Light is the 3×3 grid `face_corner_light` samples in the face plane: the
//! centre, the mid between each corner and the next, and one diagonal per
//! corner. Corner `k` averages the centre, mids `k - 1` and `k`, and diagonal
//! `k`, which are the same four samples its vertex carried.
//!
//! Normal mode 0 takes the normal from `e1 × e2`; modes 1..=6 are the axis
//! faces in `geometry::FACE_*` order, and also the face a repeated tile is
//! laid out on.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::Mesh;
use bevy::mesh::MeshVertexAttribute;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Color;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::render::render_resource::VertexFormat;

use super::vertex::BRIGHT_TINT;
use super::vertex::BlockGeometry;
use super::vertex::BlockVertex;
use super::vertex::POSITION_MIN;
use super::vertex::POSITION_SPAN;
use super::vertex::face_index;
use super::vertex::face_normal;

/// The only vertex attribute of a proxy mesh: `quad << 2 | corner`.
pub const ATTRIBUTE_QUAD_CORNER: MeshVertexAttribute =
    MeshVertexAttribute::new("Quad_Corner", 1_985_470_022, VertexFormat::Uint32);

/// `u32` words in one record.
pub const QUAD_WORDS: usize = 8;
/// Position steps per block on every axis.
pub const QUAD_STEPS: f32 = 64.0;

const SHADE_BIT: u32 = 1 << 25;
const REPEAT_BIT: u32 = 1 << 26;
const SNOW_SIDE_BIT: u32 = 1 << 27;
const TRIANGLE_BIT: u32 = 1 << 28;
const BRIGHT_BIT: u32 = 1 << 29;

/// One block quad, or one triangle of a quad that is not a parallelogram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct QuadRecord(pub [u32; QUAD_WORDS]);

/// Light samples of one quad. `mid[k]` lies between corners `k` and `k + 1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LightGrid {
    centre: u8,
    mid: [u8; 4],
    diagonal: [u8; 4],
}

/// One decoded corner of a record, as `decode_block_quad` computes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuadCorner {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub tile: [u8; 2],
    pub texel: [i32; 2],
    pub tint_srgb: [u8; 3],
    pub bright: bool,
    /// Centre first, as flat lighting reads it; the rest in no fixed order.
    pub light: [u8; 4],
    pub shade: bool,
    pub repeat_uv: bool,
    pub snow_side: bool,
}

impl BlockGeometry {
    /// Pack every quad as records for a chunk layer.
    pub fn into_quads(self) -> Vec<QuadRecord> {
        let mut records = Vec::with_capacity(self.vertex_count() / 4);
        for quad in self.vertices().chunks_exact(4) {
            pack_quad(&mut records, quad.try_into().expect("four vertices"));
        }
        records
    }
}

/// Append the record, or the two triangle records, for one quad.
pub fn pack_quad(records: &mut Vec<QuadRecord>, quad: &[BlockVertex; 4]) {
    let positions = quad.map(|vertex| vertex.position.map(quantize));
    let repeat = quad[0].repeat_uv;
    let texels = quad.map(|vertex| {
        if repeat {
            [0, 0]
        } else {
            vertex.texel.texel.map(i32::from)
        }
    });
    debug_assert!(
        quad.iter()
            .all(|vertex| vertex.texel.tile == quad[0].texel.tile),
        "a quad samples one atlas tile"
    );
    let parallelogram = closes(&positions) && closes(&texels);
    if parallelogram && let Some(light) = solve_light(quad, &[0, 1, 2, 3]) {
        records.push(record(quad, &positions, &texels, [0, 1, 2], light, false));
        return;
    }
    // Each triangle starts where its neighbors around the quad still share a
    // light sample, so the grid holds all three corners exactly.
    for order in [[0, 1, 2], [2, 3, 0]] {
        let light = solve_light(quad, &order).unwrap_or_else(|| {
            debug_assert!(false, "quad light is not a face-plane grid");
            LightGrid {
                centre: quad[order[0]].light[0],
                mid: [quad[order[0]].light[1]; 4],
                diagonal: [quad[order[0]].light[3]; 4],
            }
        });
        records.push(record(quad, &positions, &texels, order, light, true));
    }
}

fn quantize(value: f32) -> i32 {
    debug_assert!(
        (POSITION_MIN..POSITION_MIN + POSITION_SPAN).contains(&value),
        "block quad coordinate {value} is outside the packed range"
    );
    (((value - POSITION_MIN) * QUAD_STEPS).round() as i32).clamp(0, 2047)
}

/// Whether corner 2 is corner 0 plus both edges.
fn closes<const N: usize>(corners: &[[i32; N]; 4]) -> bool {
    (0..N).all(|axis| corners[1][axis] + corners[3][axis] - corners[0][axis] == corners[2][axis])
}

/// Fit the corners' samples onto the shared grid. `order` lists three or four
/// corners around the quad. A corner's vertex holds `[centre, side, side,
/// diagonal]`; neighbors around the quad share one side sample.
fn solve_light(quad: &[BlockVertex; 4], order: &[usize]) -> Option<LightGrid> {
    let light = |slot: usize| quad[order[slot]].light;
    let centre = light(0)[0];
    if (0..order.len()).any(|slot| light(slot)[0] != centre) {
        return None;
    }
    let other = |slot: usize, shared: u8| {
        let [_, a, b, _] = light(slot);
        if a == shared {
            Some(b)
        } else if b == shared {
            Some(a)
        } else {
            None
        }
    };
    let [_, first, second, _] = light(0);
    [(first, second), (second, first)]
        .into_iter()
        .find_map(|(mid0, mid3)| {
            let mid1 = other(1, mid0)?;
            let mid2 = other(2, mid1)?;
            if order.len() == 4 && other(3, mid2)? != mid3 {
                return None;
            }
            let mut diagonal = [0; 4];
            for slot in 0..order.len() {
                diagonal[slot] = light(slot)[3];
            }
            Some(LightGrid {
                centre,
                mid: [mid0, mid1, mid2, mid3],
                diagonal,
            })
        })
}

fn record(
    quad: &[BlockVertex; 4],
    positions: &[[i32; 3]; 4],
    texels: &[[i32; 2]; 4],
    order: [usize; 3],
    light: LightGrid,
    triangle: bool,
) -> QuadRecord {
    let [a, b, c] = order;
    let first = &quad[a];
    let origin = positions[a].map(|value| value as u32);
    let edge = |from: usize, to: usize, axis: usize| {
        let delta = positions[to][axis] - positions[from][axis];
        debug_assert!((-2048..2048).contains(&delta));
        (delta as u32) & 0xfff
    };
    let texel_edge = |from: usize, to: usize, axis: usize| {
        let delta = texels[to][axis] - texels[from][axis];
        debug_assert!((-32..32).contains(&delta));
        (delta as u32) & 0x3f
    };
    let normal_mode = if first.normal.iter().any(|value| value.abs() > 0.999) {
        face_index(first.normal) + 1
    } else {
        0
    };
    let snow_side = first.repeat_uv && first.texel.texel[0] == 1;
    let bright = first.tint.iter().any(|channel| *channel > 1.0);
    let tint = if bright {
        first.tint.map(|channel| channel / BRIGHT_TINT)
    } else {
        first.tint
    };
    let srgb = Color::linear_rgb(tint[0], tint[1], tint[2]).to_srgba();
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u32;
    let tile = u32::from(first.texel.tile[0] & 0xf) | u32::from(first.texel.tile[1] & 0xf) << 4;
    let mut flags = 0;
    for (set, bit) in [
        (first.shade, SHADE_BIT),
        (first.repeat_uv, REPEAT_BIT),
        (snow_side, SNOW_SIDE_BIT),
        (triangle, TRIANGLE_BIT),
        (bright, BRIGHT_BIT),
    ] {
        if set {
            flags |= bit;
        }
    }
    QuadRecord([
        origin[0] | origin[1] << 11 | (origin[2] & 0x3ff) << 22,
        origin[2] >> 10 | edge(a, b, 0) << 1 | edge(a, b, 1) << 13 | flags,
        edge(a, b, 2) | edge(b, c, 0) << 12 | tile << 24,
        edge(b, c, 1)
            | edge(b, c, 2) << 12
            | normal_mode << 24
            | (texels[a][0] as u32 & 0x1f) << 27,
        (texels[a][1] as u32 & 0x1f)
            | texel_edge(a, b, 0) << 5
            | texel_edge(a, b, 1) << 11
            | texel_edge(b, c, 0) << 17
            | texel_edge(b, c, 1) << 23,
        byte(srgb.red)
            | byte(srgb.green) << 8
            | byte(srgb.blue) << 16
            | u32::from(light.diagonal[3]) << 24,
        u32::from_le_bytes([light.centre, light.mid[0], light.mid[1], light.mid[2]]),
        u32::from_le_bytes([
            light.mid[3],
            light.diagonal[0],
            light.diagonal[1],
            light.diagonal[2],
        ]),
    ])
}

/// Sign-extend the low `bits` of `value`.
fn signed(value: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

impl QuadRecord {
    /// A record that draws only corners 0, 1, and 2.
    pub fn is_triangle(&self) -> bool {
        self.0[1] & TRIANGLE_BIT != 0
    }

    /// The four corners `decode_block_quad` produces. A triangle's corner 3
    /// repeats corner 0.
    pub fn corners(&self) -> [QuadCorner; 4] {
        let words = self.0;
        let origin = [
            words[0] & 0x7ff,
            words[0] >> 11 & 0x7ff,
            words[0] >> 22 | (words[1] & 1) << 10,
        ]
        .map(|value| value as i32);
        let edge1 = [words[1] >> 1, words[1] >> 13, words[2]].map(|value| signed(value, 12));
        let edge2 = [words[2] >> 12, words[3], words[3] >> 12].map(|value| signed(value, 12));
        let texel = [words[3] >> 27, words[4] & 0x1f].map(|value| value as i32);
        let texel1 = [words[4] >> 5, words[4] >> 11].map(|value| signed(value, 6));
        let texel2 = [words[4] >> 17, words[4] >> 23].map(|value| signed(value, 6));
        let normal_mode = words[3] >> 24 & 7;
        let normal = if normal_mode == 0 {
            let [a, b] = [edge1, edge2].map(|edge| edge.map(|value| value as f32));
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let length = cross.iter().map(|value| value * value).sum::<f32>().sqrt();
            cross.map(|value| value / length)
        } else {
            face_normal(normal_mode - 1)
        };
        let [centre, mid0, mid1, mid2] = words[6].to_le_bytes();
        let [mid3, diagonal0, diagonal1, diagonal2] = words[7].to_le_bytes();
        let mid = [mid0, mid1, mid2, mid3];
        let diagonal = [diagonal0, diagonal1, diagonal2, (words[5] >> 24) as u8];
        let triangle = self.is_triangle();
        std::array::from_fn(|corner| {
            let corner = if triangle && corner == 3 { 0 } else { corner };
            let along1 = i32::from(corner == 1 || corner == 2);
            let along2 = i32::from(corner >= 2);
            QuadCorner {
                position: std::array::from_fn(|axis| {
                    (origin[axis] + edge1[axis] * along1 + edge2[axis] * along2) as f32 / QUAD_STEPS
                        + POSITION_MIN
                }),
                normal,
                tile: [(words[2] >> 24 & 0xf) as u8, (words[2] >> 28) as u8],
                texel: std::array::from_fn(|axis| {
                    texel[axis] + texel1[axis] * along1 + texel2[axis] * along2
                }),
                tint_srgb: [
                    words[5] as u8,
                    (words[5] >> 8) as u8,
                    (words[5] >> 16) as u8,
                ],
                bright: words[1] & BRIGHT_BIT != 0,
                light: [centre, mid[(corner + 3) % 4], mid[corner], diagonal[corner]],
                shade: words[1] & SHADE_BIT != 0,
                repeat_uv: words[1] & REPEAT_BIT != 0,
                snow_side: words[1] & SNOW_SIDE_BIT != 0,
            }
        })
    }
}

/// Quads a proxy mesh for `quads` quads holds: the next power of two or one
/// and a half times a power of two, so few distinct meshes cover every layer
/// and a layer draws at most a third more vertices than it has.
pub fn proxy_capacity(quads: u32) -> u32 {
    let quads = quads.max(8);
    let power = quads.next_power_of_two();
    let between = power / 4 * 3;
    if quads <= between { between } else { power }
}

/// A mesh of `capacity` quads whose vertices carry no geometry, only which
/// corner of which quad they are. Every layer that fits shares it, and the
/// vertex shader reads the layer's records for the rest.
pub fn proxy_mesh(capacity: u32) -> Mesh {
    let corners: Vec<u32> = (0..capacity * 4).collect();
    let triangles = |quad: u32| {
        let start = quad * 4;
        [start, start + 1, start + 2, start, start + 2, start + 3]
    };
    let indices = if capacity * 4 <= u32::from(u16::MAX) + 1 {
        Indices::U16(
            (0..capacity)
                .flat_map(triangles)
                .map(|index| index as u16)
                .collect(),
        )
    } else {
        Indices::U32((0..capacity).flat_map(triangles).collect())
    };
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        ATTRIBUTE_QUAD_CORNER,
        VertexAttributeValues::Uint32(corners),
    )
    .with_inserted_indices(indices)
}

/// A layer's bytes for the quad buffer: a header record holding the quad
/// count, then the records.
pub fn layer_bytes(records: &[QuadRecord]) -> Vec<u8> {
    let count = u32::try_from(records.len()).expect("layer quad count");
    let mut bytes = Vec::with_capacity((records.len() + 1) * QUAD_WORDS * 4);
    bytes.extend_from_slice(&count.to_le_bytes());
    bytes.resize(QUAD_WORDS * 4, 0);
    bytes.extend(
        records
            .iter()
            .flat_map(|record| record.0)
            .flat_map(u32::to_le_bytes),
    );
    bytes
}

//! Greedy merging of coplanar faces that share a tile, tint, and shading.

use super::ChunkMeshes;
use super::CornerShading;
use super::FACES;
use super::LAYER_CUTOUT;
use super::LAYER_MASKED;
use super::LAYER_OVERLAY;
use super::LAYER_WATER;
use super::faces::face_texels;
use super::geometry::BlockFaceGeometry;
use super::geometry::FACE_BOTTOM;
use super::geometry::FACE_EAST;
use super::geometry::FACE_SOUTH;
use super::geometry::FACE_TOP;
use super::geometry::FACE_WEST;
use super::greedy::PLANE;
use super::greedy::mesh_binary_plane;
use super::vertex::AtlasTexel;
use super::vertex::BRIGHT_TINT;
use super::vertex::BlockGeometry;
use super::vertex::BlockVertex;
use bevy::prelude::Color;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct MergeKey {
    pub(super) layer: u8,
    pub(super) tile: [u8; 2],
    pub(super) tint: [u8; 3],
    pub(super) bright: bool,
    pub(super) ao: u8,
    pub(super) light: [u8; 4],
    pub(super) shade: bool,
    /// Keep an untinted grass side's rectangles in step with its overlay.
    pub(super) overlay_tint: Option<([u8; 3], bool)>,
    pub(super) snow_layer: bool,
    pub(super) snow_side_y: Option<u8>,
}

pub(super) struct Plane {
    pub(super) key: MergeKey,
    pub(super) tint: [f32; 3],
    pub(super) shading: CornerShading,
    pub(super) fixed: u8,
    pub(super) rows: [u16; PLANE],
}

pub(super) fn plane_key(
    layer: u8,
    tile: [u8; 2],
    tint: [f32; 3],
    shading: &CornerShading,
) -> MergeKey {
    let (tint, bright) = quantized_tint(tint);
    MergeKey {
        layer,
        tile,
        tint,
        bright,
        ao: shading.ao[0],
        light: shading.light[0],
        shade: shading.shade,
        overlay_tint: None,
        snow_layer: false,
        snow_side_y: None,
    }
}

/// Same sRGB bytes [`BlockVertex::pack`] stores, so merged faces agree on screen.
pub(super) fn quantized_tint(tint: [f32; 3]) -> ([u8; 3], bool) {
    let bright = tint.iter().any(|channel| *channel > 1.0);
    let tint = if bright {
        tint.map(|channel| channel / BRIGHT_TINT)
    } else {
        tint
    };
    let srgb = Color::linear_rgb(tint[0], tint[1], tint[2]).to_srgba();
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    ([byte(srgb.red), byte(srgb.green), byte(srgb.blue)], bright)
}

/// `(fixed, row, bit)` for a cell. Horizontal faces use x as the row and z as
/// the bit. Vertical faces use the horizontal axis as the row and the band-local
/// y as the bit, which keeps every plane inside 16×16.
pub(super) fn plane_coords(
    face: usize,
    x: usize,
    y: usize,
    z: usize,
    band_start: usize,
) -> (u8, usize, usize) {
    match face {
        FACE_TOP | FACE_BOTTOM => (y as u8, x, z),
        FACE_EAST | FACE_WEST => (x as u8, z, y - band_start),
        _ => (z as u8, x, y - band_start),
    }
}

pub(super) fn set_plane_bit(rows: &mut [u16; PLANE], row: usize, bit: usize) {
    let shift = u32::try_from(bit).expect("greedy plane bit");
    rows[row] |= 1u16.checked_shl(shift).expect("greedy plane bit");
}

pub(super) fn insert_plane(
    planes: &mut [Vec<Plane>; 6],
    face: usize,
    key: MergeKey,
    tint: [f32; 3],
    shading: CornerShading,
    fixed: u8,
    row: usize,
    bit: usize,
) {
    let rows = &mut planes[face];
    if let Some(plane) = rows
        .iter_mut()
        .find(|plane| plane.key == key && plane.fixed == fixed)
    {
        set_plane_bit(&mut plane.rows, row, bit);
        return;
    }
    let mut stored = [0u16; PLANE];
    set_plane_bit(&mut stored, row, bit);
    rows.push(Plane {
        key,
        tint,
        shading,
        fixed,
        rows: stored,
    });
}

pub(super) fn flush_planes(
    meshes: &mut ChunkMeshes,
    planes: [Vec<Plane>; 6],
    band_start: usize,
    y_origin: usize,
) {
    for (face_index, face_planes) in planes.into_iter().enumerate() {
        for plane in face_planes {
            let quads = mesh_binary_plane(plane.rows);
            let layer = layer_mut(meshes, plane.key.layer);
            for quad in quads {
                let (x, y, z) =
                    cell_origin(face_index, plane.fixed, band_start, quad.row, quad.bit);
                let origin = [x as f32, (y - y_origin) as f32, z as f32];
                push_greedy_quad(
                    layer,
                    face_index,
                    origin,
                    quad.w,
                    quad.h,
                    plane.key.tile,
                    plane.tint,
                    plane.shading,
                    plane.key.snow_layer,
                );
            }
        }
    }
}

pub(super) fn cell_origin(
    face: usize,
    fixed: u8,
    band_start: usize,
    row: u32,
    bit: u32,
) -> (usize, usize, usize) {
    let row = row as usize;
    let bit = bit as usize;
    match face {
        FACE_TOP | FACE_BOTTOM => (row, usize::from(fixed), bit),
        FACE_EAST | FACE_WEST => (usize::from(fixed), band_start + bit, row),
        _ => (row, band_start + bit, usize::from(fixed)),
    }
}

pub(super) fn layer_mut(meshes: &mut ChunkMeshes, layer: u8) -> &mut BlockGeometry {
    match layer {
        LAYER_OVERLAY => &mut meshes.grass_overlay,
        LAYER_CUTOUT => &mut meshes.cutout,
        LAYER_WATER => &mut meshes.water,
        LAYER_MASKED => &mut meshes.masked,
        _ => &mut meshes.opaque,
    }
}

pub(super) fn push_greedy_quad(
    layer: &mut BlockGeometry,
    face_index: usize,
    origin: [f32; 3],
    w: u32,
    h: u32,
    tile: [u8; 2],
    tint: [f32; 3],
    shading: CornerShading,
    snow_layer: bool,
) {
    let normal = FACES[face_index].normal;
    if w == 1 && h == 1 {
        let mut corners = BlockFaceGeometry::unit_cube().face(face_index).corners;
        if snow_layer {
            for corner in &mut corners {
                corner[1] *= 0.125;
            }
        }
        let texels = face_texels(tile[0], tile[1], face_index);
        layer.push_block_quad(origin, normal, corners, texels, tint, shading);
        return;
    }
    let mut corners = merged_corners(face_index, w as f32, h as f32);
    if snow_layer {
        for corner in &mut corners {
            corner[1] *= 0.125;
        }
    }
    // The nonzero texel flags the snow side UV scale in block_vertex.wgsl.
    let texel = AtlasTexel::new(
        tile[0],
        tile[1],
        u8::from(snow_layer && face_index != FACE_TOP && face_index != FACE_BOTTOM),
        0,
    );
    let ao = shading.ao[0];
    let light = shading.light[0];
    layer.push_quad(std::array::from_fn(|corner| BlockVertex {
        position: std::array::from_fn(|axis| origin[axis] + corners[corner][axis]),
        normal,
        texel,
        tint,
        light,
        ao,
        shade: shading.shade,
        repeat_uv: true,
    }));
}

pub(super) fn merged_corners(face: usize, w: f32, h: f32) -> [[f32; 3]; 4] {
    match face {
        FACE_TOP => [[0.0, 1.0, 0.0], [0.0, 1.0, h], [w, 1.0, h], [w, 1.0, 0.0]],
        FACE_BOTTOM => [[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, 0.0, h], [0.0, 0.0, h]],
        FACE_EAST => [[1.0, 0.0, 0.0], [1.0, h, 0.0], [1.0, h, w], [1.0, 0.0, w]],
        FACE_WEST => [[0.0, 0.0, 0.0], [0.0, 0.0, w], [0.0, h, w], [0.0, h, 0.0]],
        FACE_SOUTH => [[0.0, 0.0, 1.0], [w, 0.0, 1.0], [w, h, 1.0], [0.0, h, 1.0]],
        _ => [[0.0, 0.0, 0.0], [0.0, h, 0.0], [w, h, 0.0], [w, 0.0, 0.0]],
    }
}

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::Color;
use bevy::prelude::Mesh;
use bevy::render::render_resource::PrimitiveTopology;

use crate::world::block::block::BlockId;
use crate::world::block::properties::is_opaque_cube;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::generation::BiomeMap;
use crate::world::lighting::Skylight;
use crate::world::lighting::beta_brightness;
use crate::world::textures::FoliageColors;
use crate::world::textures::GrassColors;
use crate::world::textures::atlas_tile_uvs;
use crate::world::textures::block_tile;

struct Face {
    neighbor: [i32; 3],
    normal: [f32; 3],
    corners: [[f32; 3]; 4],
    shade: f32,
}

const FACES: [Face; 6] = [
    Face {
        neighbor: [0, 1, 0],
        normal: [0.0, 1.0, 0.0],
        corners: [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        shade: 1.0,
    },
    Face {
        neighbor: [0, -1, 0],
        normal: [0.0, -1.0, 0.0],
        corners: [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        shade: 0.55,
    },
    Face {
        neighbor: [1, 0, 0],
        normal: [1.0, 0.0, 0.0],
        corners: [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        shade: 0.6,
    },
    Face {
        neighbor: [-1, 0, 0],
        normal: [-1.0, 0.0, 0.0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        shade: 0.6,
    },
    Face {
        neighbor: [0, 0, 1],
        normal: [0.0, 0.0, 1.0],
        corners: [
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        shade: 0.8,
    },
    Face {
        neighbor: [0, 0, -1],
        normal: [0.0, 0.0, -1.0],
        corners: [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
        shade: 0.8,
    },
];

/// Opaque terrain, cutout leaves, and a separate translucent water surface.
pub struct ChunkMeshes {
    pub opaque: Mesh,
    pub grass_overlay: Mesh,
    pub cutout: Mesh,
    pub water: Mesh,
}

/// Emit only faces touching air. A missing neighbor is treated as air for this isolated chunk.
pub fn mesh_chunk(chunk: &Chunk, skylight: &Skylight) -> Mesh {
    mesh_chunk_inner(chunk, skylight, None, true, true, false).opaque
}

/// Like [`mesh_chunk`], with lighting and leaf graphics matching the settings menu.
pub fn mesh_chunk_with_settings(
    chunk: &Chunk,
    skylight: &Skylight,
    old_lighting: bool,
    fancy_graphics: bool,
) -> ChunkMeshes {
    mesh_chunk_inner(chunk, skylight, None, old_lighting, true, fancy_graphics)
}

pub fn mesh_chunk_with_settings_and_smooth_lighting(
    chunk: &Chunk,
    skylight: &Skylight,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
) -> ChunkMeshes {
    mesh_chunk_inner(
        chunk,
        skylight,
        None,
        old_lighting,
        smooth_lighting,
        fancy_graphics,
    )
}

/// Per-column biome tints applied to grass tops and leaves.
struct ColumnTints {
    grass: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
    foliage: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
}

pub(crate) fn mesh_chunk_with_biomes(
    chunk: &Chunk,
    skylight: &Skylight,
    biomes: &BiomeMap,
    grass_colors: &GrassColors,
    foliage_colors: &FoliageColors,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
) -> ChunkMeshes {
    let tints = ColumnTints {
        grass: std::array::from_fn(|index| {
            grass_colors.sample(biomes.get(index % CHUNK_SIZE, index / CHUNK_SIZE))
        }),
        foliage: std::array::from_fn(|index| {
            foliage_colors.sample(biomes.get(index % CHUNK_SIZE, index / CHUNK_SIZE))
        }),
    };
    mesh_chunk_inner(
        chunk,
        skylight,
        Some(&tints),
        old_lighting,
        smooth_lighting,
        fancy_graphics,
    )
}

const FACE_TOP: usize = 0;
const FACE_BOTTOM: usize = 1;
/// Keep the animated Beta-blue texture visible instead of washing it out
/// against the sky and lake bed through two stacked alpha layers.
const WATER_ALPHA: f32 = 0.8;
/// Two texels of a 16-pixel block, matching Beta's still-water surface drop.
const WATER_SURFACE_DROP: f32 = 2.0 / 16.0;

#[derive(Default)]
struct MeshBuffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuffers {
    fn push_face(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        face: &Face,
        face_index: usize,
        color: [f32; 4],
        corner_ao: [f32; 4],
        corner_light: [f32; 4],
        block: BlockId,
        fancy_graphics: bool,
        y_drop: f32,
    ) {
        let uvs = face_uvs(block, face_index, fancy_graphics);
        let corners = face
            .corners
            .map(|corner| [corner[0], corner[1] - y_drop, corner[2]]);
        self.push_quad(x, y, z, face, corners, uvs, color, corner_ao, corner_light);
    }

    /// Beta's fancy grass pass: the transparent overlay tile contains only
    /// the hanging grass pixels, leaving the normal dirt side unmodified.
    fn push_grass_overlay(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        face: &Face,
        face_index: usize,
        color: [f32; 4],
        corner_ao: [f32; 4],
        corner_light: [f32; 4],
    ) {
        let uvs = face_uvs_for_tile(6, 2, face_index);
        let corners = face.corners.map(|corner| {
            [
                corner[0] + face.normal[0] * 0.001,
                corner[1] + face.normal[1] * 0.001,
                corner[2] + face.normal[2] * 0.001,
            ]
        });
        self.push_quad(x, y, z, face, corners, uvs, color, corner_ao, corner_light);
    }

    fn push_quad(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        face: &Face,
        corners: [[f32; 3]; 4],
        uvs: [[f32; 2]; 4],
        color: [f32; 4],
        corner_ao: [f32; 4],
        corner_light: [f32; 4],
    ) {
        let start = self.positions.len() as u32;
        for (corner_index, corner) in corners.into_iter().enumerate() {
            self.positions.push([
                x as f32 + corner[0],
                y as f32 + corner[1],
                z as f32 + corner[2],
            ]);
            self.normals.push(face.normal);
            self.colors.push([
                color[0] * corner_ao[corner_index] * corner_light[corner_index],
                color[1] * corner_ao[corner_index] * corner_light[corner_index],
                color[2] * corner_ao[corner_index] * corner_light[corner_index],
                color[3],
            ]);
        }
        self.uvs.extend_from_slice(&uvs);
        self.indices
            .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    }

    fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn mesh_chunk_inner(
    chunk: &Chunk,
    skylight: &Skylight,
    tints: Option<&ColumnTints>,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
) -> ChunkMeshes {
    let mut opaque = MeshBuffers::default();
    let mut grass_overlay = MeshBuffers::default();
    let mut cutout = MeshBuffers::default();
    let mut water = MeshBuffers::default();

    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z).unwrap();
                if block == BlockId::Air {
                    continue;
                }

                for (face_index, face) in FACES.iter().enumerate() {
                    if block == BlockId::Water && face_index != FACE_TOP {
                        continue;
                    }

                    let nx = x as i32 + face.neighbor[0];
                    let ny = y as i32 + face.neighbor[1];
                    let nz = z as i32 + face.neighbor[2];
                    let neighbor = (nx >= 0 && ny >= 0 && nz >= 0)
                        .then(|| chunk.get(nx as usize, ny as usize, nz as usize))
                        .flatten();
                    if block == BlockId::Water && neighbor == Some(BlockId::Water) {
                        continue;
                    }
                    if neighbor_hides_face(block, neighbor, fancy_graphics) {
                        continue;
                    }

                    let level = (nx >= 0 && ny >= 0 && nz >= 0)
                        .then(|| skylight.get(nx as usize, ny as usize, nz as usize))
                        .flatten()
                        .unwrap_or(15);
                    let brightness = if old_lighting {
                        beta_brightness(level) * face.shade
                    } else {
                        1.0
                    };
                    let grass_side = block == BlockId::Grass
                        && face_index != FACE_TOP
                        && face_index != FACE_BOTTOM;
                    let grass_tint = tints
                        .map(|tints| tints.grass[z * CHUNK_SIZE + x])
                        .unwrap_or([0.55, 0.8, 0.4]);
                    let base = if block == BlockId::Grass && face_index == FACE_TOP {
                        grass_tint
                    } else {
                        block_tint(block, tints.map(|tints| tints.foliage[z * CHUNK_SIZE + x]))
                    };
                    let alpha = if block == BlockId::Water {
                        WATER_ALPHA
                    } else {
                        1.0
                    };
                    let color = [base[0], base[1], base[2], alpha];
                    let buffers = if block == BlockId::Water {
                        &mut water
                    } else if fancy_graphics && is_leaf(block) {
                        &mut cutout
                    } else {
                        &mut opaque
                    };
                    let y_drop = if block == BlockId::Water {
                        WATER_SURFACE_DROP
                    } else {
                        0.0
                    };
                    let corner_ao = if smooth_lighting {
                        face_corner_ao(chunk, x, y, z, face)
                    } else {
                        [1.0; 4]
                    };
                    let corner_light = if old_lighting && smooth_lighting {
                        face_corner_light(chunk, skylight, x, y, z, face)
                    } else if old_lighting {
                        [brightness; 4]
                    } else {
                        [1.0; 4]
                    };
                    let side_color = if grass_side {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        color
                    };
                    buffers.push_face(
                        x,
                        y,
                        z,
                        face,
                        face_index,
                        side_color,
                        corner_ao,
                        corner_light,
                        block,
                        fancy_graphics,
                        y_drop,
                    );
                    if grass_side && fancy_graphics {
                        let overlay_color = [grass_tint[0], grass_tint[1], grass_tint[2], 1.0];
                        grass_overlay.push_grass_overlay(
                            x,
                            y,
                            z,
                            face,
                            face_index,
                            overlay_color,
                            corner_ao,
                            corner_light,
                        );
                    }
                }
            }
        }
    }

    ChunkMeshes {
        opaque: opaque.into_mesh(),
        grass_overlay: grass_overlay.into_mesh(),
        cutout: cutout.into_mesh(),
        water: water.into_mesh(),
    }
}

/// Per-corner ambient occlusion matching the original voxel renderer. A
/// corner is darkened by its two face-adjacent blocks and its diagonal block;
/// when both side blocks are present the diagonal is treated as occluded too.
fn face_corner_light(
    _chunk: &Chunk,
    skylight: &Skylight,
    x: usize,
    y: usize,
    z: usize,
    face: &Face,
) -> [f32; 4] {
    let tangent_axes = match face.normal {
        [0.0, 1.0, 0.0] | [0.0, -1.0, 0.0] => [0, 2],
        [1.0, 0.0, 0.0] | [-1.0, 0.0, 0.0] => [1, 2],
        _ => [0, 1],
    };

    std::array::from_fn(|corner_index| {
        let corner = face.corners[corner_index];
        let directions = [
            if corner[tangent_axes[0]] < 0.5 { -1 } else { 1 },
            if corner[tangent_axes[1]] < 0.5 { -1 } else { 1 },
        ];
        let samples = [[0, 0], [directions[0], 0], [0, directions[1]], directions];
        let sum: f32 = samples
            .into_iter()
            .map(|sample| {
                let mut offset = face.neighbor;
                offset[tangent_axes[0]] += sample[0];
                offset[tangent_axes[1]] += sample[1];
                let position = [
                    x as i32 + offset[0],
                    y as i32 + offset[1],
                    z as i32 + offset[2],
                ];
                let level = if position.iter().any(|&coordinate| coordinate < 0) {
                    15
                } else {
                    skylight
                        .get(
                            position[0] as usize,
                            position[1] as usize,
                            position[2] as usize,
                        )
                        .unwrap_or(15)
                };
                beta_brightness(level)
            })
            .sum();
        sum * face.shade * 0.25
    })
}

fn face_corner_ao(chunk: &Chunk, x: usize, y: usize, z: usize, face: &Face) -> [f32; 4] {
    let tangent_axes = match face.normal {
        [0.0, 1.0, 0.0] | [0.0, -1.0, 0.0] => [0, 2],
        [1.0, 0.0, 0.0] | [-1.0, 0.0, 0.0] => [1, 2],
        _ => [0, 1],
    };

    std::array::from_fn(|corner_index| {
        let corner = face.corners[corner_index];
        let tangent_direction = [
            if corner[tangent_axes[0]] < 0.5 { -1 } else { 1 },
            if corner[tangent_axes[1]] < 0.5 { -1 } else { 1 },
        ];
        let mut side_a = face.neighbor;
        let mut side_b = face.neighbor;
        let mut diagonal = face.neighbor;
        side_a[tangent_axes[0]] += tangent_direction[0];
        side_b[tangent_axes[1]] += tangent_direction[1];
        diagonal[tangent_axes[0]] += tangent_direction[0];
        diagonal[tangent_axes[1]] += tangent_direction[1];

        let side_a = opaque_at(chunk, x, y, z, side_a);
        let side_b = opaque_at(chunk, x, y, z, side_b);
        let diagonal = opaque_at(chunk, x, y, z, diagonal);
        let level = if side_a && side_b {
            3
        } else {
            side_a as u8 + side_b as u8 + diagonal as u8
        };
        1.0 - level as f32 * 0.2
    })
}

fn opaque_at(chunk: &Chunk, x: usize, y: usize, z: usize, offset: [i32; 3]) -> bool {
    let position = [
        x as i32 + offset[0],
        y as i32 + offset[1],
        z as i32 + offset[2],
    ];
    if position.iter().any(|&coordinate| coordinate < 0) {
        return false;
    }
    chunk
        .get(
            position[0] as usize,
            position[1] as usize,
            position[2] as usize,
        )
        .is_some_and(is_opaque_cube)
}

fn is_leaf(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves
    )
}

/// Fast leaves hide every non-air neighbour, like any solid cube. Fancy leaves
/// are cutout, so leaf-to-leaf faces stay visible and solid faces towards a
/// canopy are not covered. Water never hides a neighbour, so lake beds and
/// walls stay visible under the surface plane.
fn neighbor_hides_face(block: BlockId, neighbor: Option<BlockId>, fancy_graphics: bool) -> bool {
    let Some(neighbor) = neighbor else {
        return false;
    };
    if neighbor == BlockId::Air || neighbor == BlockId::Water {
        return false;
    }
    if fancy_graphics && is_leaf(block) && is_leaf(neighbor) {
        return false;
    }
    if fancy_graphics && is_leaf(neighbor) && !is_leaf(block) {
        return false;
    }
    true
}

fn face_uvs(block: BlockId, face: usize, fancy_graphics: bool) -> [[f32; 2]; 4] {
    let (tile_x, tile_y) = block_tile(block, face, fancy_graphics);
    face_uvs_for_tile(tile_x, tile_y, face)
}

fn face_uvs_for_tile(tile_x: u8, tile_y: u8, face: usize) -> [[f32; 2]; 4] {
    let (u0, v0, u1, v1) = atlas_tile_uvs(tile_x, tile_y);

    match face {
        0 | 1 => [[u0, v0], [u0, v1], [u1, v1], [u1, v0]],
        2 | 5 => [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        _ => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
    }
}

fn block_tint(block: BlockId, foliage: Option<[f32; 3]>) -> [f32; 3] {
    match block {
        BlockId::Water => [0.4, 0.6, 0.95],
        BlockId::Leaves => foliage.unwrap_or([0.28, 0.71, 0.09]),
        BlockId::BirchLeaves => linear_rgb(128, 167, 85),
        BlockId::SpruceLeaves => linear_rgb(97, 153, 97),
        _ => [1.0, 1.0, 1.0],
    }
}

fn linear_rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let color = Color::srgb_u8(r, g, b).to_linear();
    [color.red, color.green, color.blue]
}

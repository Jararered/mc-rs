use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::Color;
use bevy::prelude::Mesh;
use bevy::render::render_resource::PrimitiveTopology;

use crate::world::block::block::BlockId;
use crate::world::block::properties::is_crossed_plant;
use crate::world::block::properties::is_opaque_cube;
use crate::world::block::properties::is_torch;
use crate::world::block::properties::torch_normal;
use crate::world::block::properties::torch_point;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPos;
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
    /// Crossed flower and tall-grass quads. A separate layer so the material
    /// can disable back-face culling without affecting grass sides or torches.
    pub plants: Mesh,
}

/// Block data surrounding a chunk, used for face culling and ambient
/// occlusion at its edges. Streaming supplies all eight neighbors before
/// meshing; isolated mesh helpers may leave them absent.
#[derive(Default)]
pub struct ChunkNeighbors<'a> {
    pub west: Option<&'a Chunk>,
    pub east: Option<&'a Chunk>,
    pub north: Option<&'a Chunk>,
    pub south: Option<&'a Chunk>,
    pub northwest: Option<&'a Chunk>,
    pub northeast: Option<&'a Chunk>,
    pub southwest: Option<&'a Chunk>,
    pub southeast: Option<&'a Chunk>,
}

impl ChunkNeighbors<'_> {
    fn get(&self, center: &Chunk, x: i32, y: i32, z: i32) -> Option<BlockId> {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return None;
        }
        if (0..CHUNK_SIZE as i32).contains(&x) && (0..CHUNK_SIZE as i32).contains(&z) {
            return center.get(x as usize, y as usize, z as usize);
        }
        let region_x = if x < 0 {
            -1
        } else if x >= CHUNK_SIZE as i32 {
            1
        } else {
            0
        };
        let region_z = if z < 0 {
            -1
        } else if z >= CHUNK_SIZE as i32 {
            1
        } else {
            0
        };
        let chunk = match (region_x, region_z) {
            (0, 0) => Some(center),
            (-1, 0) => self.west,
            (1, 0) => self.east,
            (0, -1) => self.north,
            (0, 1) => self.south,
            (-1, -1) => self.northwest,
            (1, -1) => self.northeast,
            (-1, 1) => self.southwest,
            (1, 1) => self.southeast,
            _ => None,
        }?;
        let local_x = (x - region_x * CHUNK_SIZE as i32) as usize;
        let local_z = (z - region_z * CHUNK_SIZE as i32) as usize;
        chunk.get(local_x, y as usize, local_z)
    }
}

/// Emit only faces touching air. A missing neighbor is treated as air for this isolated chunk.
pub fn mesh_chunk(chunk: &Chunk, skylight: &Skylight) -> Mesh {
    mesh_chunk_inner(
        chunk,
        &ChunkNeighbors::default(),
        skylight,
        None,
        true,
        true,
        false,
        0,
        0,
        0,
    )
    .opaque
}

/// Opaque mesh with neighboring block data available across chunk boundaries.
pub fn mesh_chunk_with_neighbors(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
) -> Mesh {
    mesh_chunk_inner(chunk, neighbors, skylight, None, true, true, false, 0, 0, 0).opaque
}

/// Like [`mesh_chunk`], with lighting and leaf graphics matching the settings menu.
pub fn mesh_chunk_with_settings(
    chunk: &Chunk,
    skylight: &Skylight,
    old_lighting: bool,
    fancy_graphics: bool,
) -> ChunkMeshes {
    mesh_chunk_inner(
        chunk,
        &ChunkNeighbors::default(),
        skylight,
        None,
        old_lighting,
        true,
        fancy_graphics,
        0,
        0,
        0,
    )
}

pub fn mesh_chunk_with_settings_and_smooth_lighting(
    chunk: &Chunk,
    skylight: &Skylight,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
    skylight_subtracted: u8,
) -> ChunkMeshes {
    mesh_chunk_inner(
        chunk,
        &ChunkNeighbors::default(),
        skylight,
        None,
        old_lighting,
        smooth_lighting,
        fancy_graphics,
        skylight_subtracted,
        0,
        0,
    )
}

/// Per-column biome tints applied to grass tops and leaves.
struct ColumnTints {
    grass: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
    foliage: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
}

pub(crate) fn mesh_chunk_with_biomes(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
    biomes: &BiomeMap,
    grass_colors: &GrassColors,
    foliage_colors: &FoliageColors,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
    skylight_subtracted: u8,
    position: ChunkPos,
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
        neighbors,
        skylight,
        Some(&tints),
        old_lighting,
        smooth_lighting,
        fancy_graphics,
        skylight_subtracted,
        position.x * CHUNK_SIZE as i32,
        position.z * CHUNK_SIZE as i32,
    )
}

const FACE_TOP: usize = 0;
const FACE_BOTTOM: usize = 1;
/// Keep the animated Beta-blue texture visible instead of washing it out
/// against the sky and lake bed through two stacked alpha layers.
const WATER_ALPHA: f32 = 0.8;
/// Two texels of a 16-pixel block, matching Beta's still-water surface drop.
const WATER_SURFACE_DROP: f32 = 2.0 / 16.0;
/// Lava uses the same inset surface plane as water, while remaining opaque.
const LAVA_SURFACE_DROP: f32 = 2.0 / 16.0;

#[derive(Default)]
struct MeshBuffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuffers {
    fn push_torch(&mut self, x: usize, y: usize, z: usize, block: BlockId) {
        // Build the same post for floor and wall attachments, then rotate its
        // vertices and normals together so the cap follows the shaft.
        for (face_index, face) in FACES.iter().enumerate() {
            if face_index == FACE_BOTTOM {
                continue;
            }
            let corners = face.corners.map(|corner| {
                torch_point(
                    block,
                    [
                        0.5 + (corner[0] - 0.5) * 0.125,
                        corner[1] * 0.625,
                        0.5 + (corner[2] - 0.5) * 0.125,
                    ],
                )
            });
            let rotated_face = Face {
                neighbor: face.neighbor,
                normal: torch_normal(block, face.normal),
                corners: face.corners,
                shade: face.shade,
            };
            let mut uvs = face_uvs_for_tile(0, 5, face_index);
            let (u0, v0, u1, v1) = atlas_tile_uvs(0, 5);
            let du = (u1 - u0) / 16.0;
            let dv = (v1 - v0) / 16.0;
            if face_index == FACE_TOP {
                uvs = [
                    [u0 + 7.0 * du, v0 + 6.0 * dv],
                    [u0 + 7.0 * du, v0 + 8.0 * dv],
                    [u0 + 9.0 * du, v0 + 8.0 * dv],
                    [u0 + 9.0 * du, v0 + 6.0 * dv],
                ];
            } else {
                // Only columns 7..8 and rows 6..15 contain the torch.
                // Sampling the whole transparent tile shrinks the shaft to
                // two pixels on a face that is already physically narrow.
                for uv in &mut uvs {
                    uv[0] = u0 + (7.0 + (uv[0] - u0) / (u1 - u0) * 2.0) * du;
                    uv[1] = v0 + (6.0 + (uv[1] - v0) / (v1 - v0) * 10.0) * dv;
                }
            }
            self.push_quad(
                x,
                y,
                z,
                &rotated_face,
                corners,
                uvs,
                [1.0; 4],
                [1.0; 4],
                [1.0; 4],
            );
        }
    }

    /// Two 1×1 squares rotated 45° around Y. One winding each; the plant
    /// material draws both sides.
    fn push_crossed_plant(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        block: BlockId,
        origin_x: i32,
        origin_z: i32,
        grass_tint: [f32; 3],
        brightness: f32,
    ) {
        let [dx, dy, dz] = crossed_plant_offset(origin_x + x as i32, y as i32, origin_z + z as i32);
        let center_x = 0.5 + dx;
        let center_z = 0.5 + dz;
        let half = 0.5 / std::f32::consts::SQRT_2;
        let tint = if matches!(block, BlockId::TallGrass | BlockId::Fern) {
            grass_tint
        } else {
            [1.0, 1.0, 1.0]
        };
        let color = [
            tint[0] * brightness,
            tint[1] * brightness,
            tint[2] * brightness,
            1.0,
        ];
        let (tile_x, tile_y) = block_tile(block, 0, false);
        let (u0, v0, u1, v1) = atlas_tile_uvs(tile_x, tile_y);
        let uvs = [[u0, v0], [u0, v1], [u1, v1], [u1, v0]];
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        let quads = [
            (
                [-inv_sqrt2, 0.0, inv_sqrt2],
                [
                    [center_x - half, 1.0 + dy, center_z - half],
                    [center_x - half, dy, center_z - half],
                    [center_x + half, dy, center_z + half],
                    [center_x + half, 1.0 + dy, center_z + half],
                ],
            ),
            (
                [inv_sqrt2, 0.0, inv_sqrt2],
                [
                    [center_x - half, 1.0 + dy, center_z + half],
                    [center_x - half, dy, center_z + half],
                    [center_x + half, dy, center_z - half],
                    [center_x + half, 1.0 + dy, center_z - half],
                ],
            ),
        ];
        for (normal, corners) in quads {
            let face = Face {
                neighbor: [0, 0, 0],
                normal,
                corners: [[0.0; 3]; 4],
                shade: 1.0,
            };
            self.push_quad(x, y, z, &face, corners, uvs, color, [1.0; 4], [1.0; 4]);
        }
    }

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
        let shape_height = if block == BlockId::SnowLayer {
            0.125
        } else {
            1.0
        };
        let corners = face
            .corners
            .map(|corner| [corner[0], corner[1] * shape_height - y_drop, corner[2]]);
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
            // Chunk block data remains in WorldChunks. After upload, Bevy can
            // release this mesh's CPU vertex and index buffers; a remesh
            // replaces the asset at the same handle with fresh geometry.
            RenderAssetUsages::RENDER_WORLD,
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
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
    tints: Option<&ColumnTints>,
    old_lighting: bool,
    smooth_lighting: bool,
    fancy_graphics: bool,
    skylight_subtracted: u8,
    origin_x: i32,
    origin_z: i32,
) -> ChunkMeshes {
    let mut opaque = MeshBuffers::default();
    let mut grass_overlay = MeshBuffers::default();
    let mut cutout = MeshBuffers::default();
    let mut water = MeshBuffers::default();
    let mut plants = MeshBuffers::default();

    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let block = chunk.get(x, y, z).unwrap();
                if block == BlockId::Air {
                    continue;
                }
                if is_torch(block) {
                    grass_overlay.push_torch(x, y, z, block);
                    continue;
                }
                if is_crossed_plant(block) {
                    let grass_tint = tints
                        .map(|tints| tints.grass[z * CHUNK_SIZE + x])
                        .unwrap_or([0.55, 0.8, 0.4]);
                    let level =
                        skylight.light_at(x as i32, y as i32, z as i32, skylight_subtracted);
                    let brightness = if old_lighting {
                        beta_brightness(level)
                    } else {
                        1.0
                    };
                    plants.push_crossed_plant(
                        x, y, z, block, origin_x, origin_z, grass_tint, brightness,
                    );
                    continue;
                }

                for (face_index, face) in FACES.iter().enumerate() {
                    if is_surface_liquid(block) && face_index != FACE_TOP {
                        continue;
                    }

                    let nx = x as i32 + face.neighbor[0];
                    let ny = y as i32 + face.neighbor[1];
                    let nz = z as i32 + face.neighbor[2];
                    let neighbor = neighbors.get(chunk, nx, ny, nz);
                    if neighbor.is_some_and(|neighbor| same_surface_liquid(block, neighbor)) {
                        continue;
                    }
                    if neighbor_hides_face(block, neighbor, fancy_graphics) {
                        continue;
                    }

                    let level = skylight.light_at(nx, ny, nz, skylight_subtracted);
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
                    let y_drop = match block {
                        BlockId::Water => WATER_SURFACE_DROP,
                        BlockId::Lava | BlockId::FlowingLava => LAVA_SURFACE_DROP,
                        _ => 0.0,
                    };
                    let corner_ao = if smooth_lighting {
                        face_corner_ao(chunk, neighbors, x, y, z, face)
                    } else {
                        [1.0; 4]
                    };
                    let corner_light = if old_lighting && smooth_lighting {
                        face_corner_light(chunk, skylight, x, y, z, face, skylight_subtracted)
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
        plants: plants.into_mesh(),
    }
}

/// Horizontal jitter from Beta `RenderBlocks.renderBlockReed`.
///
/// `x * 3129871` is a Java `int` multiply. The rest is signed 64-bit wrapping
/// arithmetic. The result is added to the block origin before the crossed quads
/// are built, so a plant sits slightly off center and sinks up to 0.2 blocks.
pub fn crossed_plant_offset(x: i32, y: i32, z: i32) -> [f32; 3] {
    let mut hash =
        (x.wrapping_mul(3_129_871) as i64) ^ (z as i64).wrapping_mul(116_129_781) ^ y as i64;
    hash = hash.wrapping_mul(hash).wrapping_mul(42_317_861) + hash.wrapping_mul(11);
    let nibble = |shift| ((hash >> shift) & 15_i64) as f32 / 15.0;
    [
        (nibble(16) - 0.5) * 0.5,
        (nibble(20) - 1.0) * 0.2,
        (nibble(24) - 0.5) * 0.5,
    ]
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
    skylight_subtracted: u8,
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
                let level =
                    skylight.light_at(position[0], position[1], position[2], skylight_subtracted);
                beta_brightness(level)
            })
            .sum();
        sum * face.shade * 0.25
    })
}

fn face_corner_ao(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
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

        let side_a = opaque_at(chunk, neighbors, x, y, z, side_a);
        let side_b = opaque_at(chunk, neighbors, x, y, z, side_b);
        let diagonal = opaque_at(chunk, neighbors, x, y, z, diagonal);
        let level = if side_a && side_b {
            3
        } else {
            side_a as u8 + side_b as u8 + diagonal as u8
        };
        1.0 - level as f32 * 0.2
    })
}

fn opaque_at(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
    offset: [i32; 3],
) -> bool {
    let position = [
        x as i32 + offset[0],
        y as i32 + offset[1],
        z as i32 + offset[2],
    ];
    neighbors
        .get(chunk, position[0], position[1], position[2])
        .is_some_and(is_opaque_cube)
}

fn is_leaf(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves
    )
}

fn is_surface_liquid(block: BlockId) -> bool {
    matches!(block, BlockId::Water | BlockId::Lava | BlockId::FlowingLava)
}

fn same_surface_liquid(block: BlockId, neighbor: BlockId) -> bool {
    matches!((block, neighbor), (BlockId::Water, BlockId::Water))
        || matches!(
            (block, neighbor),
            (
                BlockId::Lava | BlockId::FlowingLava,
                BlockId::Lava | BlockId::FlowingLava
            )
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
    if neighbor == BlockId::Air
        || neighbor == BlockId::SnowLayer
        || neighbor == BlockId::Water
        || neighbor == BlockId::FlowingWater
        || neighbor == BlockId::Lava
        || neighbor == BlockId::FlowingLava
        || neighbor == BlockId::MobSpawner
        || neighbor == BlockId::Chest
        || is_torch(neighbor)
        || is_crossed_plant(neighbor)
    {
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
    if block.is_lit_furnace() {
        // The active face is a small flame, but Beta's lit furnace body also
        // appears subtly brighter than the idle block.
        return [1.12, 1.12, 1.12];
    }
    match block {
        BlockId::Water => [0.4, 0.6, 0.95],
        BlockId::Leaves => foliage.unwrap_or([0.28, 0.71, 0.09]),
        BlockId::BirchLeaves => linear_rgb(128, 167, 85),
        BlockId::SpruceLeaves => linear_rgb(97, 153, 97),
        // Beta 1.7.3 only has the oak plank tile. These species variants use
        // that tile with a color multiplier, matching the game's tint path.
        BlockId::SprucePlanks => linear_rgb(214, 177, 131),
        BlockId::BirchPlanks => linear_rgb(255, 246, 218),
        _ => [1.0, 1.0, 1.0],
    }
}

fn linear_rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let color = Color::srgb_u8(r, g, b).to_linear();
    [color.red, color.green, color.blue]
}

/// A dropped block: the world cube, centered on the origin, with the same
/// face tiles, shade, and tints the chunk mesher uses. Fancy grass also
/// gets the side overlay. Fancy leaves are marked cutout.
pub struct DroppedBlockMeshes {
    pub body: Mesh,
    pub overlay: Option<Mesh>,
    pub cutout: bool,
}

pub fn dropped_block_meshes(
    block: BlockId,
    fancy_graphics: bool,
    grass_tint: [f32; 3],
    foliage_tint: [f32; 3],
) -> DroppedBlockMeshes {
    let mut body = MeshBuffers::default();
    let mut overlay = MeshBuffers::default();
    for (face_index, face) in FACES.iter().enumerate() {
        let grass_side =
            block == BlockId::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
        let tint = if block == BlockId::Grass && face_index == FACE_TOP {
            grass_tint
        } else {
            block_tint(block, Some(foliage_tint))
        };
        let shade = face.shade;
        let color = [tint[0] * shade, tint[1] * shade, tint[2] * shade, 1.0];
        let corners = face
            .corners
            .map(|corner| [corner[0] - 0.5, corner[1] - 0.5, corner[2] - 0.5]);
        body.push_quad(
            0,
            0,
            0,
            face,
            corners,
            face_uvs(block, face_index, fancy_graphics),
            color,
            [1.0; 4],
            [1.0; 4],
        );
        if grass_side && fancy_graphics {
            let overlay_color = [
                grass_tint[0] * shade,
                grass_tint[1] * shade,
                grass_tint[2] * shade,
                1.0,
            ];
            let corners = face.corners.map(|corner| {
                [
                    corner[0] - 0.5 + face.normal[0] * 0.001,
                    corner[1] - 0.5 + face.normal[1] * 0.001,
                    corner[2] - 0.5 + face.normal[2] * 0.001,
                ]
            });
            overlay.push_quad(
                0,
                0,
                0,
                face,
                corners,
                face_uvs_for_tile(6, 2, face_index),
                overlay_color,
                [1.0; 4],
                [1.0; 4],
            );
        }
    }
    DroppedBlockMeshes {
        body: body.into_mesh(),
        overlay: (!overlay.positions.is_empty()).then(|| overlay.into_mesh()),
        cutout: fancy_graphics && is_leaf(block),
    }
}

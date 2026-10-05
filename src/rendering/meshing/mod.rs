use std::ops::Range;

use bevy::prelude::Color;

use crate::block::blocks::Block;
use crate::block::blocks::species;
use crate::block::direction::Direction;
use crate::block::fluids::Fluid;
use crate::block::fluids::corner_height;
use crate::block::fluids::flow_vector;
use crate::block::properties::torch_normal;
use crate::block::properties::torch_point;
use crate::rendering::textures::FoliageColors;
use crate::rendering::textures::GrassColors;
use crate::rendering::textures::LAVA_FLOW_TILE;
use crate::rendering::textures::LAVA_STILL_TILE;
use crate::rendering::textures::SNOWY_GRASS_SIDE_TILE;
use crate::rendering::textures::WATER_FLOW_TILE;
use crate::rendering::textures::WATER_STILL_TILE;
use crate::rendering::textures::block_tile;
use crate::rendering::textures::crop_tile;
use crate::rendering::textures::farmland_top_tile;
use crate::world::biome::BiomeMap;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::lighting::Skylight;

pub(crate) mod geometry;
mod greedy;
mod quads;
mod vertex;

pub use self::quads::ATTRIBUTE_QUAD_CORNER;
pub use self::quads::QUAD_STEPS;
pub use self::quads::QUAD_WORDS;
pub use self::quads::QuadCorner;
pub use self::quads::QuadRecord;
pub use self::quads::layer_bytes;
pub use self::quads::pack_quad;
pub use self::quads::proxy_capacity;
pub use self::quads::proxy_mesh;
pub use self::vertex::ATTRIBUTE_BLOCK_VERTEX;
pub use self::vertex::AtlasTexel;
pub use self::vertex::BRIGHT_TINT;
pub use self::vertex::BlockGeometry;
pub use self::vertex::BlockLighting;
pub use self::vertex::BlockVertex;
pub use self::vertex::FULL_BRIGHT;
pub use self::vertex::PackedFields;
pub use self::vertex::face_shade;
pub use self::vertex::unpack_vertex;

use self::geometry::BlockFaceGeometry;
use self::geometry::FACE_BOTTOM;
use self::geometry::FACE_EAST;
use self::geometry::FACE_NORTH;
use self::geometry::FACE_SOUTH;
use self::geometry::FACE_TOP;
use self::geometry::FACE_WEST;
use self::geometry::FaceGeometry;
use self::greedy::PLANE;
use self::greedy::mesh_binary_plane;

struct Face {
    neighbor: [i32; 3],
    normal: [f32; 3],
}

const FACES: [Face; 6] = [
    Face {
        neighbor: [0, 1, 0],
        normal: [0.0, 1.0, 0.0],
    },
    Face {
        neighbor: [0, -1, 0],
        normal: [0.0, -1.0, 0.0],
    },
    Face {
        neighbor: [1, 0, 0],
        normal: [1.0, 0.0, 0.0],
    },
    Face {
        neighbor: [-1, 0, 0],
        normal: [-1.0, 0.0, 0.0],
    },
    Face {
        neighbor: [0, 0, 1],
        normal: [0.0, 0.0, 1.0],
    },
    Face {
        neighbor: [0, 0, -1],
        normal: [0.0, 0.0, -1.0],
    },
];

/// Opaque terrain, cutout leaves, and a separate translucent water surface.
pub struct ChunkMeshes {
    pub opaque: BlockGeometry,
    pub grass_overlay: BlockGeometry,
    pub cutout: BlockGeometry,
    /// Static alpha-masked geometry such as cactus faces and crossed plants.
    pub masked: BlockGeometry,
    pub water: BlockGeometry,
}

impl ChunkMeshes {
    pub fn is_empty(&self) -> bool {
        self.layers().iter().all(|layer| layer.is_empty())
    }

    /// Layers in render order: opaque, grass overlay, cutout, water, masked.
    pub fn layers(&self) -> [&BlockGeometry; 5] {
        [
            &self.opaque,
            &self.grass_overlay,
            &self.cutout,
            &self.water,
            &self.masked,
        ]
    }

    pub fn into_layers(self) -> [BlockGeometry; 5] {
        [
            self.opaque,
            self.grass_overlay,
            self.cutout,
            self.water,
            self.masked,
        ]
    }
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
    fn get(&self, center: &Chunk, x: i32, y: i32, z: i32) -> Option<Block> {
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

    /// The block and metadata at a chunk-local cell, reading into the
    /// neighbors. Missing cells read as air.
    fn cell(&self, center: &Chunk, x: i32, y: i32, z: i32) -> (Block, u8) {
        let Some(block) = self.get(center, x, y, z) else {
            return (Block::Air, 0);
        };
        let region = |value: i32| value.div_euclid(CHUNK_SIZE as i32);
        let chunk = match (region(x), region(z)) {
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
        };
        let metadata = chunk.map_or(0, |chunk| {
            chunk.metadata(
                x.rem_euclid(CHUNK_SIZE as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_SIZE as i32) as usize,
            )
        });
        (block, metadata)
    }
}

/// Emit only faces touching air. A missing neighbor is treated as air for this isolated chunk.
pub fn mesh_chunk(chunk: &Chunk, skylight: &Skylight) -> BlockGeometry {
    mesh_chunk_with_neighbors(chunk, &ChunkNeighbors::default(), skylight)
}

/// Opaque geometry with neighboring block data available across chunk boundaries.
pub fn mesh_chunk_with_neighbors(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
) -> BlockGeometry {
    Mesher::new(chunk, neighbors, skylight, None, false, 0, 0, None)
        .region(0..CHUNK_HEIGHT, 0)
        .opaque
}

/// Like [`mesh_chunk`], with the leaf graphics matching the settings menu.
/// Lighting settings do not change geometry; see [`BlockLighting`].
pub fn mesh_chunk_with_settings(
    chunk: &Chunk,
    skylight: &Skylight,
    fancy_graphics: bool,
) -> ChunkMeshes {
    mesh_chunk_filtered(chunk, skylight, fancy_graphics, None)
}

/// Like [`mesh_chunk_with_settings`], but omit every block other than `only`
/// when that filter is set. Neighbor culling is unchanged, so the remaining
/// faces are that block's shell against air and other blocks.
pub fn mesh_chunk_filtered(
    chunk: &Chunk,
    skylight: &Skylight,
    fancy_graphics: bool,
    only: Option<Block>,
) -> ChunkMeshes {
    Mesher::new(
        chunk,
        &ChunkNeighbors::default(),
        skylight,
        None,
        fancy_graphics,
        0,
        0,
        only,
    )
    .region(0..CHUNK_HEIGHT, 0)
}

/// Per-column biome tints applied to grass tops and leaves.
struct ColumnTints {
    grass: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
    foliage: [[f32; 3]; CHUNK_SIZE * CHUNK_SIZE],
}

impl ColumnTints {
    fn sample(
        biomes: &BiomeMap,
        grass_colors: &GrassColors,
        foliage_colors: &FoliageColors,
    ) -> Self {
        Self {
            grass: std::array::from_fn(|index| {
                grass_colors.sample(biomes.get(index % CHUNK_SIZE, index / CHUNK_SIZE))
            }),
            foliage: std::array::from_fn(|index| {
                foliage_colors.sample(biomes.get(index % CHUNK_SIZE, index / CHUNK_SIZE))
            }),
        }
    }
}

/// Mesh a whole chunk with its per-column climate colors applied to grass and plants.
pub fn mesh_chunk_with_biomes(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
    biomes: &BiomeMap,
    grass_colors: &GrassColors,
    foliage_colors: &FoliageColors,
    fancy_graphics: bool,
    position: ChunkPosition,
) -> ChunkMeshes {
    SectionMesher::new(
        chunk,
        neighbors,
        skylight,
        biomes,
        grass_colors,
        foliage_colors,
        fancy_graphics,
        position,
        None,
    )
    .mesher
    .region(0..CHUNK_HEIGHT, 0)
}

/// Meshes one 16-block-tall render section at a time. Vertex positions are
/// relative to the section origin, `(0, section * SECTION_HEIGHT, 0)` in the
/// chunk, which keeps them inside the packed vertex range.
pub struct SectionMesher<'a> {
    mesher: Mesher<'a>,
}

impl<'a> SectionMesher<'a> {
    pub fn new(
        chunk: &'a Chunk,
        neighbors: &'a ChunkNeighbors<'a>,
        skylight: &'a Skylight,
        biomes: &BiomeMap,
        grass_colors: &GrassColors,
        foliage_colors: &FoliageColors,
        fancy_graphics: bool,
        position: ChunkPosition,
        only: Option<Block>,
    ) -> Self {
        Self {
            mesher: Mesher::new(
                chunk,
                neighbors,
                skylight,
                Some(ColumnTints::sample(biomes, grass_colors, foliage_colors)),
                fancy_graphics,
                position.x * CHUNK_SIZE as i32,
                position.z * CHUNK_SIZE as i32,
                only,
            ),
        }
    }

    pub fn mesh(&self, section: usize) -> ChunkMeshes {
        let origin = section * SECTION_HEIGHT;
        self.mesher.region(origin..origin + SECTION_HEIGHT, origin)
    }
}

/// Keep the animated Beta-blue texture visible instead of washing it out
/// against the sky and lake bed through two stacked alpha layers. Applied as
/// the water material's base alpha.
pub const WATER_ALPHA: f32 = 0.8;
const DEFAULT_GRASS_TINT: [f32; 3] = [0.55, 0.8, 0.4];
const WATER_TINT: [f32; 3] = [0.4, 0.6, 0.95];
/// `RenderBlocks.renderBlockCrops` sinks crops a pixel into their farmland.
const CROP_DROP: f32 = 1.0 / 16.0;

/// Light and occlusion for one quad's four corners.
#[derive(Clone, Copy)]
struct CornerShading {
    light: [[u8; 4]; 4],
    ao: [u8; 4],
    shade: bool,
}

impl CornerShading {
    const FULL_BRIGHT: Self = Self {
        light: [FULL_BRIGHT; 4],
        ao: [0; 4],
        shade: false,
    };
}

impl BlockGeometry {
    /// Beta's ladder is one transparent wall plane. The saved facing names the
    /// supporting wall; draw the texture toward the room side.
    fn push_ladder(&mut self, origin: [f32; 3], metadata: u8) {
        let (face_index, coordinate) = match Block::Ladder.support_offset(metadata) {
            Some([0, 0, -1]) => (FACE_SOUTH, 0.125),
            Some([0, 0, 1]) => (FACE_NORTH, 0.875),
            Some([1, 0, 0]) => (FACE_WEST, 0.875),
            Some([-1, 0, 0]) => (FACE_EAST, 0.125),
            _ => (FACE_NORTH, 0.125),
        };
        let corners = BlockFaceGeometry::unit_cube()
            .face(face_index)
            .corners
            .map(|mut corner| {
                let axis = if face_index == FACE_EAST || face_index == FACE_WEST {
                    0
                } else {
                    2
                };
                corner[axis] = coordinate;
                corner
            });
        self.push_block_quad(
            origin,
            FACES[face_index].normal,
            corners,
            face_texels(3, 5, face_index),
            [1.0; 3],
            CornerShading::FULL_BRIGHT,
        );
    }

    /// Build the same post for floor and wall attachments, then rotate its
    /// vertices and normals together so the cap follows the shaft.
    fn push_torch(&mut self, origin: [f32; 3], metadata: u8) {
        let facing = Block::Torch.facing(metadata);
        let unit_cube = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            if face_index == FACE_BOTTOM {
                continue;
            }
            let corners = unit_cube.face(face_index).corners.map(|corner| {
                torch_point(
                    facing,
                    [
                        0.5 + (corner[0] - 0.5) * 0.125,
                        corner[1] * 0.625,
                        0.5 + (corner[2] - 0.5) * 0.125,
                    ],
                )
            });
            let texels = if face_index == FACE_TOP {
                [
                    AtlasTexel::new(0, 5, 7, 6),
                    AtlasTexel::new(0, 5, 7, 8),
                    AtlasTexel::new(0, 5, 9, 8),
                    AtlasTexel::new(0, 5, 9, 6),
                ]
            } else {
                // Only columns 7..8 and rows 6..15 contain the torch.
                // Sampling the whole transparent tile shrinks the shaft to
                // two pixels on a face that is already physically narrow.
                face_texels(0, 5, face_index).map(|texel| {
                    let [u, v] = texel.texel;
                    AtlasTexel::new(0, 5, 7 + u / 16 * 2, 6 + v / 16 * 10)
                })
            };
            self.push_block_quad(
                origin,
                torch_normal(facing, face.normal),
                corners,
                texels,
                [1.0; 3],
                CornerShading::FULL_BRIGHT,
            );
        }
    }

    /// `RenderBlocks.renderCrossedSquares`: two diagonal planes with endpoints
    /// 0.45 block either side of the cell center, each drawn once per side
    /// because the plant material culls back faces.
    fn push_crossed_plant(
        &mut self,
        origin: [f32; 3],
        world: [i32; 3],
        block: Block,
        metadata: u8,
        grass_tint: [f32; 3],
        light: u8,
    ) {
        // Beta jitters only Block.tallGrass in renderBlockReed. Fern is its
        // metadata-2 equivalent here; flowers and mushrooms stay centered.
        let [dx, dy, dz] = if matches!(block, Block::TallGrass) {
            crossed_plant_offset(world[0], world[1], world[2])
        } else {
            [0.0; 3]
        };
        let center_x = 0.5 + dx;
        let center_z = 0.5 + dz;
        let half = 0.45;
        let tint = if matches!(block, Block::TallGrass) {
            grass_tint
        } else {
            [1.0, 1.0, 1.0]
        };
        let (tile_x, tile_y) = block_tile(block, metadata, 0, false);
        let texels = tile_texels(tile_x, tile_y, [[0, 0], [0, 16], [16, 16], [16, 0]]);
        let (x0, x1) = (center_x - half, center_x + half);
        let (z0, z1) = (center_z - half, center_z + half);
        let (bottom, top) = (dy, 1.0 + dy);
        let quads = [
            [[x0, z0], [x0, z0], [x1, z1], [x1, z1]],
            [[x0, z1], [x0, z1], [x1, z0], [x1, z0]],
        ];
        // Crossed squares take the plant cell's own light, with no face
        // shade or corner occlusion.
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: false,
        };
        for xz in quads {
            let heights = [top, bottom, bottom, top];
            let corners = std::array::from_fn(|i| [xz[i][0], heights[i], xz[i][1]]);
            self.push_two_sided_quad(origin, corners, texels, tint, shading);
        }
    }

    /// `RenderBlocks.renderBlockCrops`: four upright planes a quarter block
    /// in from each side, in a `#` pattern. One winding each; the plant
    /// material draws both sides.
    fn push_crops(&mut self, origin: [f32; 3], stage: u8, light: u8) {
        let (tile_x, tile_y) = crop_tile(stage);
        let texels = tile_texels(tile_x, tile_y, [[0, 0], [0, 16], [16, 16], [16, 0]]);
        let (bottom, top) = (-CROP_DROP, 1.0 - CROP_DROP);
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: false,
        };
        for offset in [0.25, 0.75] {
            self.push_two_sided_quad(
                origin,
                [
                    [offset, top, 0.0],
                    [offset, bottom, 0.0],
                    [offset, bottom, 1.0],
                    [offset, top, 1.0],
                ],
                texels,
                [1.0; 3],
                shading,
            );
            self.push_two_sided_quad(
                origin,
                [
                    [0.0, top, offset],
                    [0.0, bottom, offset],
                    [1.0, bottom, offset],
                    [1.0, top, offset],
                ],
                texels,
                [1.0; 3],
                shading,
            );
        }
    }

    fn push_face(
        &mut self,
        origin: [f32; 3],
        face: &Face,
        geometry: FaceGeometry,
        face_index: usize,
        tint: [f32; 3],
        shading: CornerShading,
        block: Block,
        tile: (u8, u8),
    ) {
        let texels = face_texels(tile.0, tile.1, face_index);
        let shape_height = match block {
            Block::SnowLayer => 0.125,
            Block::Farmland => 15.0 / 16.0,
            _ => 1.0,
        };
        let corners = geometry
            .corners
            .map(|corner| [corner[0], corner[1] * shape_height, corner[2]]);
        self.push_block_quad(origin, face.normal, corners, texels, tint, shading);
    }

    /// Beta's fancy grass pass: the transparent overlay tile contains only
    /// the hanging grass pixels, leaving the normal dirt side unmodified.
    /// Coplanar with the dirt side face, not nudged outward: both quads pack
    /// their corners into the same quantized vertex positions (see
    /// `vertex.rs`) with identical triangulation. Greedy base faces must also
    /// split at overlay tint boundaries to preserve that depth equality.
    fn push_grass_overlay(
        &mut self,
        origin: [f32; 3],
        face: &Face,
        face_index: usize,
        tint: [f32; 3],
        shading: CornerShading,
    ) {
        let corners = BlockFaceGeometry::unit_cube().face(face_index).corners;
        self.push_block_quad(
            origin,
            face.normal,
            corners,
            face_texels(6, 2, face_index),
            tint,
            shading,
        );
    }

    /// A quad plus its reverse, as Beta's crossed squares and crops emit them:
    /// the back quad starts at the far corner with the same texels, so the
    /// sprite reads the same from both sides under back-face culling.
    fn push_two_sided_quad(
        &mut self,
        origin: [f32; 3],
        corners: [[f32; 3]; 4],
        texels: [AtlasTexel; 4],
        tint: [f32; 3],
        shading: CornerShading,
    ) {
        let edge =
            |from: [f32; 3], to: [f32; 3]| std::array::from_fn::<f32, 3, _>(|i| to[i] - from[i]);
        let (a, b) = (edge(corners[0], corners[1]), edge(corners[1], corners[2]));
        let cross = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = cross.iter().map(|v| v * v).sum::<f32>().sqrt();
        let normal = cross.map(|v| v / length);
        self.push_block_quad(origin, normal, corners, texels, tint, shading);
        let reversed = [corners[3], corners[2], corners[1], corners[0]];
        self.push_block_quad(origin, normal.map(|v| -v), reversed, texels, tint, shading);
    }

    fn push_block_quad(
        &mut self,
        origin: [f32; 3],
        normal: [f32; 3],
        corners: [[f32; 3]; 4],
        texels: [AtlasTexel; 4],
        tint: [f32; 3],
        shading: CornerShading,
    ) {
        self.push_quad(std::array::from_fn(|corner| BlockVertex {
            position: std::array::from_fn(|axis| origin[axis] + corners[corner][axis]),
            normal,
            texel: texels[corner],
            tint,
            light: shading.light[corner],
            ao: shading.ao[corner],
            shade: shading.shade,
            repeat_uv: false,
        }));
    }
}

struct Mesher<'a> {
    chunk: &'a Chunk,
    neighbors: &'a ChunkNeighbors<'a>,
    skylight: &'a Skylight,
    tints: Option<ColumnTints>,
    fancy_graphics: bool,
    origin_x: i32,
    origin_z: i32,
    /// Omit every other block. Face culling still reads the real neighbors.
    only: Option<Block>,
}

impl<'a> Mesher<'a> {
    fn new(
        chunk: &'a Chunk,
        neighbors: &'a ChunkNeighbors<'a>,
        skylight: &'a Skylight,
        tints: Option<ColumnTints>,
        fancy_graphics: bool,
        origin_x: i32,
        origin_z: i32,
        only: Option<Block>,
    ) -> Self {
        Self {
            chunk,
            neighbors,
            skylight,
            tints,
            fancy_graphics,
            origin_x,
            origin_z,
            only,
        }
    }

    /// `RenderBlocks.renderBlockFluids`: a surface whose corners follow the
    /// levels around them, sides where the neighbor is open, and a bottom
    /// over open space. Water goes in the translucent layer, lava is opaque.
    fn push_fluid(
        &self,
        meshes: &mut ChunkMeshes,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
        fluid: Fluid,
    ) {
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let (xi, yi, zi) = (x as i32, y as i32, z as i32);
        let cell = |cx: i32, cy: i32, cz: i32| neighbors.cell(chunk, cx, cy, cz);
        // `BlockFluid.shouldSideBeRendered`: never against the same fluid or
        // ice; the top whenever it is open to something else; other sides
        // unless an opaque cube covers them.
        let renders = |face: &Face| {
            let [dx, dy, dz] = face.neighbor;
            if yi + dy < 0 {
                return false;
            }
            match neighbors.get(chunk, xi + dx, yi + dy, zi + dz) {
                None => true,
                Some(neighbor) => {
                    Fluid::of(neighbor) != Some(fluid)
                        && neighbor != Block::Ice
                        && (dy > 0 || !neighbor.is_opaque_cube())
                }
            }
        };
        // Most water sits inside a lake with every face hidden.
        let visible = FACES.each_ref().map(|face| renders(face));
        if !visible.contains(&true) {
            return;
        }
        let height = |cx: i32, cz: i32| corner_height(fluid, cx, yi, cz, cell);
        let heights = [
            [height(xi, zi), height(xi, zi + 1)],
            [height(xi + 1, zi), height(xi + 1, zi + 1)],
        ];
        let corner_top = |corner: [f32; 3]| heights[corner[0] as usize][corner[2] as usize];
        let (still, flow, tint, layer) = match fluid {
            Fluid::Water => (
                WATER_STILL_TILE,
                WATER_FLOW_TILE,
                WATER_TINT,
                &mut meshes.water,
            ),
            Fluid::Lava => (
                LAVA_STILL_TILE,
                LAVA_FLOW_TILE,
                [1.0; 3],
                &mut meshes.opaque,
            ),
        };
        let unit_cube = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            if !visible[face_index] {
                continue;
            }
            let unit = unit_cube.face(face_index);
            // Shade from the unit face: the light sampling side of a corner
            // must not flip for a low surface.
            let shading = CornerShading {
                light: face_corner_light(self.skylight, x, y, z, face, unit),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, unit),
                shade: true,
            };
            let corners = unit.corners.map(|corner| {
                let top = if corner[1] > 0.5 {
                    corner_top(corner)
                } else {
                    0.0
                };
                [corner[0], top, corner[2]]
            });
            let texels = if face_index == FACE_TOP {
                fluid_top_texels(fluid, xi, yi, zi, still, flow, &cell)
            } else if face_index == FACE_BOTTOM {
                face_texels(still.0, still.1, face_index)
            } else {
                // Sides show the flowing tile cut off at the surface.
                let mut texels = face_texels(flow.0, flow.1, face_index);
                for (texel, corner) in texels.iter_mut().zip(corners) {
                    if texel.texel[1] == 0 {
                        texel.texel[1] = ((1.0 - corner[1]) * 16.0).round() as u8;
                    }
                }
                texels
            };
            layer.push_block_quad(origin, face.normal, corners, texels, tint, shading);
        }
    }

    /// Mesh the chunk layers in `rows`, with positions relative to `y_origin`.
    ///
    /// Full cubes are merged inside each 16-block band. A section mesh is one
    /// band, so a rectangle never crosses a section boundary.
    fn region(&self, rows: Range<usize>, y_origin: usize) -> ChunkMeshes {
        let mut meshes = empty_meshes();
        let mut start = rows.start;
        while start < rows.end {
            let end = (start + SECTION_HEIGHT).min(rows.end);
            self.mesh_band(&mut meshes, start..end, y_origin);
            start = end;
        }
        meshes
    }

    fn mesh_band(&self, meshes: &mut ChunkMeshes, rows: Range<usize>, y_origin: usize) {
        let band_start = rows.start;
        let mut planes = std::array::from_fn(|_| Vec::<Plane>::new());
        let chunk = self.chunk;
        let skylight = self.skylight;
        for y in rows {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let block = chunk.get(x, y, z).unwrap();
                    if block == Block::Air || self.only.is_some_and(|only| block != only) {
                        continue;
                    }
                    let origin = [x as f32, (y - y_origin) as f32, z as f32];
                    let metadata = chunk.metadata(x, y, z);
                    if block.is_torch() {
                        meshes.grass_overlay.push_torch(origin, metadata);
                        continue;
                    }
                    if block.is_ladder() {
                        meshes.masked.push_ladder(origin, metadata);
                        continue;
                    }
                    let column = z * CHUNK_SIZE + x;
                    let grass_tint = self
                        .tints
                        .as_ref()
                        .map_or(DEFAULT_GRASS_TINT, |tints| tints.grass[column]);
                    if let Some(fluid) = Fluid::of(block) {
                        // A wireframe filter draws the open fluid surface as full
                        // cubes so it can share the greedy planes. Faces against
                        // opaque blocks stay on those blocks. Gameplay water
                        // keeps its sloped, per-cell surface.
                        if self.only == Some(block) {
                            self.queue_cube(
                                meshes,
                                &mut planes,
                                band_start,
                                y_origin,
                                [x, y, z],
                                block,
                                metadata,
                                grass_tint,
                            );
                        } else {
                            self.push_fluid(meshes, origin, x, y, z, fluid);
                        }
                        continue;
                    }
                    if block == Block::Crops {
                        let light = skylight.channels_at(x as i32, y as i32, z as i32);
                        meshes
                            .masked
                            .push_crops(origin, chunk.metadata(x, y, z), light);
                        continue;
                    }
                    if block.is_crossed_plant() {
                        let light = skylight.channels_at(x as i32, y as i32, z as i32);
                        meshes.masked.push_crossed_plant(
                            origin,
                            [self.origin_x + x as i32, y as i32, self.origin_z + z as i32],
                            block,
                            metadata,
                            grass_tint,
                            light,
                        );
                        continue;
                    }
                    if shaped_block(block) {
                        self.emit_shaped(meshes, origin, [x, y, z], block, metadata, grass_tint);
                        continue;
                    }
                    self.queue_cube(
                        meshes,
                        &mut planes,
                        band_start,
                        y_origin,
                        [x, y, z],
                        block,
                        metadata,
                        grass_tint,
                    );
                }
            }
        }
        flush_planes(meshes, planes, band_start, y_origin);
    }

    /// Cactus, farmland, and chests keep per-face geometry.
    fn emit_shaped(
        &self,
        meshes: &mut ChunkMeshes,
        origin: [f32; 3],
        at: [usize; 3],
        block: Block,
        metadata: u8,
        grass_tint: [f32; 3],
    ) {
        let [x, y, z] = at;
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let fancy_graphics = self.fancy_graphics;
        let column = z * CHUNK_SIZE + x;
        let chest_pair = if block.is_chest() {
            chest_pair_direction(chunk, neighbors, x, y, z)
        } else {
            None
        };
        let block_geometry = if block.is_chest() {
            chest_geometry(chest_pair)
        } else {
            BlockFaceGeometry::for_block(block)
        };
        for (face_index, face) in FACES.iter().enumerate() {
            let face_geometry = block_geometry.face(face_index);
            if chest_pair == Some(face.neighbor) {
                continue;
            }
            let nx = x as i32 + face.neighbor[0];
            let ny = y as i32 + face.neighbor[1];
            let nz = z as i32 + face.neighbor[2];
            if ny < 0 {
                continue;
            }
            let neighbor = neighbors.get(chunk, nx, ny, nz);
            if neighbor_hides_face(block, neighbor, fancy_graphics) {
                continue;
            }
            let grass_side =
                block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
            let base = if block == Block::Grass && face_index == FACE_TOP {
                grass_tint
            } else {
                block_tint(
                    block,
                    metadata,
                    self.tints.as_ref().map(|tints| tints.foliage[column]),
                )
            };
            let layer = if block == Block::Cactus {
                &mut meshes.masked
            } else if fancy_graphics && block.is_leaves() {
                &mut meshes.cutout
            } else {
                &mut meshes.opaque
            };
            let chest_tile = chest_pair
                .map(|direction| double_chest_tile(block, metadata, direction, face_index));
            let wet_farmland_top =
                block == Block::Farmland && face_index == FACE_TOP && metadata > 0;
            let tile = wet_farmland_top
                .then(|| farmland_top_tile(true))
                .or(chest_tile)
                .unwrap_or_else(|| block_tile(block, metadata, face_index, fancy_graphics));
            let shading = CornerShading {
                light: face_corner_light(skylight, x, y, z, face, face_geometry),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, face_geometry),
                shade: true,
            };
            let side_tint = if grass_side { [1.0; 3] } else { base };
            layer.push_face(
                origin,
                face,
                face_geometry,
                face_index,
                side_tint,
                shading,
                block,
                tile,
            );
            if grass_side && fancy_graphics {
                meshes
                    .grass_overlay
                    .push_grass_overlay(origin, face, face_index, grass_tint, shading);
            }
        }
    }

    /// Full unit-cube faces. Flat shading joins a bit plane; a corner gradient
    /// stays a single quad so smooth light is unchanged.
    #[allow(clippy::too_many_arguments)]
    fn queue_cube(
        &self,
        meshes: &mut ChunkMeshes,
        planes: &mut [Vec<Plane>; 6],
        band_start: usize,
        y_origin: usize,
        at: [usize; 3],
        block: Block,
        metadata: u8,
        grass_tint: [f32; 3],
    ) {
        let [x, y, z] = at;
        let origin = [x as f32, (y - y_origin) as f32, z as f32];
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let fancy_graphics = self.fancy_graphics;
        let column = z * CHUNK_SIZE + x;
        let foliage = self.tints.as_ref().map(|tints| tints.foliage[column]);
        let unit = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            let ny = y as i32 + face.neighbor[1];
            if ny < 0 {
                continue;
            }
            let neighbor = neighbors.get(
                chunk,
                x as i32 + face.neighbor[0],
                ny,
                z as i32 + face.neighbor[2],
            );
            // A filtered fluid keeps the open surface only. Faces against
            // sand, dirt, and other opaque cubes belong to those blocks.
            let filtered_fluid = (self.only == Some(block))
                .then(|| Fluid::of(block))
                .flatten();
            if let Some(fluid) = filtered_fluid {
                if !fluid_shell_face_visible(fluid, neighbor, face.neighbor[1]) {
                    continue;
                }
            } else if (block == Block::SnowLayer
                && face_index != FACE_TOP
                && face_index != FACE_BOTTOM
                && neighbor == Some(Block::SnowLayer))
                || neighbor_hides_face(block, neighbor, fancy_graphics)
            {
                // Equal-height snow layers share a side, including across chunks.
                continue;
            }
            let geometry = unit.face(face_index);
            let shading = CornerShading {
                light: face_corner_light(skylight, x, y, z, face, geometry),
                ao: face_corner_ao(chunk, neighbors, x, y, z, face, geometry),
                shade: true,
            };
            let grass_side =
                block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
            // A side under snow is not a grass side any more: `RenderBlocks`
            // only draws the overlay for tile 3, so the snowy tile gets none.
            let snow_covered = grass_side && snow_above(chunk, neighbors, x, y, z);
            let grass_overlay = grass_side && fancy_graphics && !snow_covered;
            let base = if block == Block::Grass && face_index == FACE_TOP {
                grass_tint
            } else {
                block_tint(block, metadata, foliage)
            };
            let side_tint = if grass_side { [1.0; 3] } else { base };
            let (tile_x, tile_y) = if snow_covered {
                SNOWY_GRASS_SIDE_TILE
            } else {
                block_tile(block, metadata, face_index, fancy_graphics)
            };
            let layer = if filtered_fluid == Some(Fluid::Water) {
                LAYER_WATER
            } else if fancy_graphics && block.is_leaves() {
                LAYER_CUTOUT
            } else {
                LAYER_OPAQUE
            };
            // Gameplay keeps a corner gradient as its own quad so smooth light
            // stays exact. A block filter is a wireframe, so light and occlusion
            // drop out of the merge key and a long wall becomes one rectangle.
            let recorded = if self.only.is_some() {
                CornerShading {
                    light: [FULL_BRIGHT; 4],
                    ao: [0; 4],
                    shade: shading.shade,
                }
            } else {
                shading
            };
            if self.only.is_some() || uniform_shading(&shading) {
                let (fixed, row, bit) = plane_coords(face_index, x, y, z, band_start);
                let mut base_key = plane_key(layer, [tile_x, tile_y], side_tint, &recorded);
                if block == Block::SnowLayer {
                    base_key.snow_layer = true;
                    // Snow sides can merge horizontally, but never up a full
                    // block: each layer is only an eighth of a block high.
                    if face_index != FACE_TOP && face_index != FACE_BOTTOM {
                        base_key.snow_side_y = Some(y as u8);
                    }
                }
                if grass_overlay {
                    // The coplanar passes must rasterize identical triangles.
                    // Dirt is untinted, but merging it across an overlay tint
                    // boundary gives the two layers different depth rounding.
                    base_key.overlay_tint = Some(quantized_tint(grass_tint));
                }
                insert_plane(
                    planes, face_index, base_key, side_tint, recorded, fixed, row, bit,
                );
                if grass_overlay {
                    insert_plane(
                        planes,
                        face_index,
                        plane_key(LAYER_OVERLAY, GRASS_OVERLAY_TILE, grass_tint, &recorded),
                        grass_tint,
                        recorded,
                        fixed,
                        row,
                        bit,
                    );
                }
            } else {
                let target = layer_mut(meshes, layer);
                target.push_face(
                    origin,
                    face,
                    geometry,
                    face_index,
                    side_tint,
                    shading,
                    block,
                    (tile_x, tile_y),
                );
                if grass_overlay {
                    meshes
                        .grass_overlay
                        .push_grass_overlay(origin, face, face_index, grass_tint, shading);
                }
            }
        }
    }
}

const LAYER_OPAQUE: u8 = 0;
const LAYER_OVERLAY: u8 = 1;
const LAYER_CUTOUT: u8 = 2;
const LAYER_WATER: u8 = 3;
const GRASS_OVERLAY_TILE: [u8; 2] = [6, 2];

fn empty_meshes() -> ChunkMeshes {
    ChunkMeshes {
        opaque: BlockGeometry::default(),
        grass_overlay: BlockGeometry::default(),
        cutout: BlockGeometry::default(),
        masked: BlockGeometry::default(),
        water: BlockGeometry::default(),
    }
}

fn shaped_block(block: Block) -> bool {
    block == Block::Cactus || block == Block::Farmland || block.is_chest()
}

/// Faces whose four corners already match can share a rectangle. The merged
/// quad copies that one shading, so it matches the 1×1 quads it replaces.
fn uniform_shading(shading: &CornerShading) -> bool {
    let ao = shading.ao[0];
    let light = shading.light[0];
    shading.ao.iter().all(|value| *value == ao)
        && shading.light.iter().all(|sample| *sample == light)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MergeKey {
    layer: u8,
    tile: [u8; 2],
    tint: [u8; 3],
    bright: bool,
    ao: u8,
    light: [u8; 4],
    shade: bool,
    /// Keep an untinted grass side's rectangles in step with its overlay.
    overlay_tint: Option<([u8; 3], bool)>,
    snow_layer: bool,
    snow_side_y: Option<u8>,
}

struct Plane {
    key: MergeKey,
    tint: [f32; 3],
    shading: CornerShading,
    fixed: u8,
    rows: [u16; PLANE],
}

fn plane_key(layer: u8, tile: [u8; 2], tint: [f32; 3], shading: &CornerShading) -> MergeKey {
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
fn quantized_tint(tint: [f32; 3]) -> ([u8; 3], bool) {
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
fn plane_coords(
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

fn set_plane_bit(rows: &mut [u16; PLANE], row: usize, bit: usize) {
    let shift = u32::try_from(bit).expect("greedy plane bit");
    rows[row] |= 1u16.checked_shl(shift).expect("greedy plane bit");
}

fn insert_plane(
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

fn flush_planes(
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

fn cell_origin(
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

fn layer_mut(meshes: &mut ChunkMeshes, layer: u8) -> &mut BlockGeometry {
    match layer {
        LAYER_OVERLAY => &mut meshes.grass_overlay,
        LAYER_CUTOUT => &mut meshes.cutout,
        LAYER_WATER => &mut meshes.water,
        _ => &mut meshes.opaque,
    }
}

fn push_greedy_quad(
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

fn merged_corners(face: usize, w: f32, h: f32) -> [[f32; 3]; 4] {
    match face {
        FACE_TOP => [[0.0, 1.0, 0.0], [0.0, 1.0, h], [w, 1.0, h], [w, 1.0, 0.0]],
        FACE_BOTTOM => [[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, 0.0, h], [0.0, 0.0, h]],
        FACE_EAST => [[1.0, 0.0, 0.0], [1.0, h, 0.0], [1.0, h, w], [1.0, 0.0, w]],
        FACE_WEST => [[0.0, 0.0, 0.0], [0.0, 0.0, w], [0.0, h, w], [0.0, h, 0.0]],
        FACE_SOUTH => [[0.0, 0.0, 1.0], [w, 0.0, 1.0], [w, h, 1.0], [0.0, h, 1.0]],
        _ => [[0.0, 0.0, 0.0], [0.0, h, 0.0], [w, h, 0.0], [w, 0.0, 0.0]],
    }
}

/// A fluid's top texture. Still fluid uses the still tile; moving fluid uses
/// the flowing 2×2 tiles rotated toward `BlockFluid.getFlowDirection`.
fn fluid_top_texels(
    fluid: Fluid,
    x: i32,
    y: i32,
    z: i32,
    still: (u8, u8),
    flow: (u8, u8),
    cell: &impl Fn(i32, i32, i32) -> (Block, u8),
) -> [AtlasTexel; 4] {
    let [flow_x, flow_z] = flow_vector(fluid, x, y, z, cell);
    if flow_x == 0.0 && flow_z == 0.0 {
        return face_texels(still.0, still.1, FACE_TOP);
    }
    let angle = flow_z.atan2(flow_x) - std::f32::consts::FRAC_PI_2;
    let (sin, cos) = angle.sin_cos();
    // The top face's corners in `UNIT_FACE_CORNERS` order, as -1/+1 offsets
    // from the block center.
    [(-1, -1), (-1, 1), (1, 1), (1, -1)].map(|(dx, dz): (i32, i32)| {
        // Beta centers the flow texture at texel 16 of its repeated 2×2
        // region. The 5-bit packed texels cover the full rotated footprint.
        let u = 16.0 + 8.0 * (dx as f32 * cos + dz as f32 * sin);
        let v = 16.0 + 8.0 * (dz as f32 * cos - dx as f32 * sin);
        AtlasTexel::new(flow.0, flow.1, u.round() as u8, v.round() as u8)
    })
}

/// Whether the mesher draws two block states identically, including how
/// they shape their neighbors' faces. Metadata shows in fluid levels, crop
/// stages, whether farmland is wet, and the species and facing
/// ([`Block::appearance_metadata`]); a flowing fluid and its still block draw
/// alike.
pub fn same_appearance(block: Block, metadata: u8, other: Block, other_metadata: u8) -> bool {
    let key = |block: Block, metadata: u8| match Fluid::of(block) {
        Some(fluid) => (fluid.still(), metadata),
        None => match block {
            Block::Crops => (block, metadata),
            Block::Farmland => (block, u8::from(metadata > 0)),
            _ => (block, block.appearance_metadata(metadata)),
        },
    };
    key(block, metadata) == key(other, other_metadata)
}

/// Horizontal jitter from Beta `RenderBlocks.renderBlockReed`.
///
/// `x * 3129871` is a Java `int` multiply. The rest is signed 64-bit wrapping
/// arithmetic. The result is added to the block origin before the crossed quads
/// are built, so a plant sits slightly off center and sinks up to 0.2 blocks.
pub fn crossed_plant_offset(x: i32, y: i32, z: i32) -> [f32; 3] {
    let mut hash =
        (x.wrapping_mul(3_129_871) as i64) ^ (z as i64).wrapping_mul(116_129_781) ^ y as i64;
    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(42_317_861)
        .wrapping_add(hash.wrapping_mul(11));
    let nibble = |shift| ((hash >> shift) & 15_i64) as f32 / 15.0;
    [
        (nibble(16) - 0.5) * 0.5,
        (nibble(20) - 1.0) * 0.2,
        (nibble(24) - 0.5) * 0.5,
    ]
}

fn tangent_axes(face: &Face) -> [usize; 2] {
    match face.neighbor {
        [0, _, 0] => [0, 2],
        [_, 0, 0] => [1, 2],
        _ => [0, 1],
    }
}

/// The four light samples of each corner, as Beta's smooth lighting reads
/// them: the face neighbor, the two neighbors beside it toward the corner,
/// and the diagonal. The first sample alone is the flat-lighting value.
fn face_corner_light(
    skylight: &Skylight,
    x: usize,
    y: usize,
    z: usize,
    face: &Face,
    geometry: FaceGeometry,
) -> [[u8; 4]; 4] {
    let tangent_axes = tangent_axes(face);
    std::array::from_fn(|corner_index| {
        let corner = geometry.corners[corner_index];
        let directions = [
            if corner[tangent_axes[0]] < 0.5 { -1 } else { 1 },
            if corner[tangent_axes[1]] < 0.5 { -1 } else { 1 },
        ];
        let samples = [[0, 0], [directions[0], 0], [0, directions[1]], directions];
        samples.map(|sample| {
            let mut offset = face.neighbor;
            offset[tangent_axes[0]] += sample[0];
            offset[tangent_axes[1]] += sample[1];
            skylight.channels_at(
                x as i32 + offset[0],
                y as i32 + offset[1],
                z as i32 + offset[2],
            )
        })
    })
}

/// Per-corner ambient occlusion levels matching the original voxel renderer.
/// A corner is darkened by its two face-adjacent blocks and its diagonal
/// block; when both side blocks are present the diagonal is treated as
/// occluded too.
fn face_corner_ao(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
    face: &Face,
    geometry: FaceGeometry,
) -> [u8; 4] {
    let tangent_axes = tangent_axes(face);
    std::array::from_fn(|corner_index| {
        let corner = geometry.corners[corner_index];
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
        if side_a && side_b {
            3
        } else {
            u8::from(side_a) + u8::from(side_b) + u8::from(diagonal)
        }
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
        .is_some_and(Block::is_opaque_cube)
}

/// Return the sole, reciprocal chest neighbor for a valid pair.
fn chest_pair_direction(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
) -> Option<[i32; 3]> {
    let position = [x as i32, y as i32, z as i32];
    let directions = [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1]];
    let adjacent = directions
        .into_iter()
        .filter(|direction| {
            neighbors
                .get(
                    chunk,
                    position[0] + direction[0],
                    position[1],
                    position[2] + direction[2],
                )
                .is_some_and(Block::is_chest)
        })
        .collect::<Vec<_>>();
    let [direction] = adjacent.as_slice() else {
        return None;
    };
    let partner = [
        position[0] + direction[0],
        position[1],
        position[2] + direction[2],
    ];
    let reciprocal_neighbors = directions
        .into_iter()
        .filter(|other| {
            let neighbor = [partner[0] + other[0], partner[1], partner[2] + other[2]];
            neighbor != position
                && neighbors
                    .get(chunk, neighbor[0], neighbor[1], neighbor[2])
                    .is_some_and(Block::is_chest)
        })
        .count();
    (reciprocal_neighbors == 0).then_some(*direction)
}

fn chest_geometry(pair_direction: Option<[i32; 3]>) -> BlockFaceGeometry {
    let inset = 1.0 / 16.0;
    let mut bounds = [inset, 0.0, inset, 1.0 - inset, 14.0 / 16.0, 1.0 - inset];
    match pair_direction {
        Some([-1, 0, 0]) => bounds[0] = 0.0,
        Some([1, 0, 0]) => bounds[3] = 1.0,
        Some([0, 0, -1]) => bounds[2] = 0.0,
        Some([0, 0, 1]) => bounds[5] = 1.0,
        _ => {}
    }
    BlockFaceGeometry::from_bounds(bounds)
}

fn double_chest_tile(
    block: Block,
    metadata: u8,
    pair_direction: [i32; 3],
    face: usize,
) -> (u8, u8) {
    if face == FACE_TOP || face == FACE_BOTTOM {
        return (9, 1);
    }
    let facing = block.facing(metadata).unwrap_or_default();
    let front = facing.face_index();
    let back = match front {
        FACE_EAST => FACE_WEST,
        FACE_WEST => FACE_EAST,
        FACE_SOUTH => FACE_NORTH,
        _ => FACE_SOUTH,
    };
    if face != front && face != back {
        return (10, 1);
    }

    // The atlas stores each long double-chest face as two adjacent tiles.
    // Keep the halves in the same left-to-right order when viewed from front.
    let first_half = pair_direction[0] > 0 || pair_direction[2] > 0;
    let first_is_left = matches!(facing, Direction::North | Direction::East);
    let left_half = first_half == first_is_left;
    let tile_x = if left_half { 9 } else { 10 };
    let tile_y = if face == front { 2 } else { 3 };
    (tile_x, tile_y)
}

/// Same visibility as [`Mesher::push_fluid`]: either id of this fluid and ice
/// cover a face, and an opaque cube covers the sides and bottom. An open top
/// stays visible, which is the flat water surface in a filtered wireframe.
fn fluid_shell_face_visible(fluid: Fluid, neighbor: Option<Block>, dy: i32) -> bool {
    match neighbor {
        None => true,
        Some(neighbor) => {
            Fluid::of(neighbor) != Some(fluid)
                && neighbor != Block::Ice
                && (dy > 0 || !neighbor.is_opaque_cube())
        }
    }
}

/// `BlockGrass.getBlockTexture` reads the block above and swaps a grass side
/// for the snow-capped tile when that material is `Material.snow` (the snow
/// layer) or `Material.builtSnow` (the snow block). The block is always in the
/// same column as its cover, so this never reaches into a neighbour chunk.
fn snow_above(chunk: &Chunk, neighbors: &ChunkNeighbors<'_>, x: usize, y: usize, z: usize) -> bool {
    matches!(
        neighbors.get(chunk, x as i32, y as i32 + 1, z as i32),
        Some(Block::SnowLayer | Block::Snow)
    )
}

/// Fast leaves hide every non-air neighbour, like any solid cube. Fancy leaves
/// are cutout, so leaf-to-leaf faces stay visible and solid faces towards a
/// canopy are not covered. Water never hides a neighbour, so lake beds and
/// walls stay visible under the surface plane.
fn neighbor_hides_face(block: Block, neighbor: Option<Block>, fancy_graphics: bool) -> bool {
    let Some(neighbor) = neighbor else {
        return false;
    };
    if neighbor == Block::Air
        || neighbor == Block::SnowLayer
        || neighbor == Block::Cactus
        || neighbor == Block::Farmland
        || neighbor == Block::Crops
        || neighbor == Block::Water
        || neighbor == Block::FlowingWater
        || neighbor == Block::Lava
        || neighbor == Block::FlowingLava
        || neighbor == Block::MobSpawner
        || neighbor.is_chest()
        || neighbor.is_ladder()
        || neighbor.is_torch()
        || neighbor.is_crossed_plant()
    {
        return false;
    }
    if neighbor == Block::Glass && block != Block::Glass {
        return false;
    }
    if fancy_graphics && block.is_leaves() && neighbor.is_leaves() {
        return false;
    }
    if fancy_graphics && neighbor.is_leaves() && !block.is_leaves() {
        return false;
    }
    true
}

/// Tile corners in the winding each face's geometry uses.
fn face_texels(tile_x: u8, tile_y: u8, face: usize) -> [AtlasTexel; 4] {
    let corners = match face {
        0 | 1 => [[0, 0], [0, 16], [16, 16], [16, 0]],
        2 | 5 => [[0, 16], [0, 0], [16, 0], [16, 16]],
        _ => [[0, 16], [16, 16], [16, 0], [0, 0]],
    };
    tile_texels(tile_x, tile_y, corners)
}

fn tile_texels(tile_x: u8, tile_y: u8, corners: [[u8; 2]; 4]) -> [AtlasTexel; 4] {
    corners.map(|[u, v]| AtlasTexel::new(tile_x, tile_y, u, v))
}

fn block_tint(block: Block, metadata: u8, foliage: Option<[f32; 3]>) -> [f32; 3] {
    if block == Block::LitFurnace {
        // The active face is a small flame, but Beta's lit furnace body also
        // appears subtly brighter than the idle block.
        return [BRIGHT_TINT; 3];
    }
    match block {
        Block::Water | Block::FlowingWater => WATER_TINT,
        Block::Leaves => match metadata & 3 {
            species::BIRCH => linear_rgb(128, 167, 85),
            species::SPRUCE => linear_rgb(97, 153, 97),
            _ => foliage.unwrap_or([0.28, 0.71, 0.09]),
        },
        // Beta 1.7.3 only has the oak plank tile. The other species use that
        // tile with a color multiplier, matching the game's tint path.
        Block::WoodenPlanks => match metadata & 3 {
            species::SPRUCE => linear_rgb(214, 177, 131),
            species::BIRCH => linear_rgb(255, 246, 218),
            _ => [1.0, 1.0, 1.0],
        },
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
    pub body: BlockGeometry,
    pub overlay: Option<BlockGeometry>,
    pub cutout: bool,
    pub alpha_masked: bool,
}

pub fn dropped_block_meshes(
    block: Block,
    metadata: u8,
    fancy_graphics: bool,
    grass_tint: [f32; 3],
    foliage_tint: [f32; 3],
) -> DroppedBlockMeshes {
    let mut body = BlockGeometry::default();
    let mut overlay = BlockGeometry::default();
    let block_geometry = BlockFaceGeometry::for_block(block);
    let centered = [-0.5; 3];
    for (face_index, face) in FACES.iter().enumerate() {
        let face_geometry = block_geometry.face(face_index);
        let grass_side =
            block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
        let tint = if block == Block::Grass && face_index == FACE_TOP {
            grass_tint
        } else {
            block_tint(block, metadata, Some(foliage_tint))
        };
        // Items carry their face shade in the tint and ignore world light, so
        // they look the same under every lighting mode and time of day.
        let shade = face_shade(face.normal);
        body.push_block_quad(
            centered,
            face.normal,
            face_geometry.corners,
            {
                let (tile_x, tile_y) = block_tile(block, metadata, face_index, fancy_graphics);
                face_texels(tile_x, tile_y, face_index)
            },
            tint.map(|channel| channel * shade),
            CornerShading::FULL_BRIGHT,
        );
        if grass_side && fancy_graphics {
            overlay.push_grass_overlay(
                centered,
                face,
                face_index,
                grass_tint.map(|channel| channel * shade),
                CornerShading::FULL_BRIGHT,
            );
        }
    }
    DroppedBlockMeshes {
        body,
        overlay: (!overlay.is_empty()).then_some(overlay),
        cutout: fancy_graphics && block.is_leaves(),
        alpha_masked: block == Block::Cactus,
    }
}

impl crate::world::block_ticks::BlockChange {
    /// Whether any mesh can differ after the change. A flowing fluid settling
    /// into its still block, a leaf's decay flag, or a cactus's age change
    /// what the world simulates but nothing it draws.
    pub fn needs_remesh(&self) -> bool {
        self.changes_light()
            || !same_appearance(
                self.previous,
                self.previous_metadata,
                self.block,
                self.metadata,
            )
    }
}

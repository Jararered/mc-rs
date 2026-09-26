use std::ops::Range;

use bevy::prelude::Color;

use crate::block::id::Id;
use crate::block::properties::is_crossed_plant;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::is_torch;
use crate::block::properties::torch_normal;
use crate::block::properties::torch_point;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::generation::BiomeMap;
use crate::world::lighting::Skylight;
use crate::world::textures::FoliageColors;
use crate::world::textures::GrassColors;
use crate::world::textures::block_tile;
use crate::world::textures::farmland_top_tile;

pub(crate) mod geometry;
mod vertex;

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
    fn get(&self, center: &Chunk, x: i32, y: i32, z: i32) -> Option<Id> {
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
pub fn mesh_chunk(chunk: &Chunk, skylight: &Skylight) -> BlockGeometry {
    mesh_chunk_with_neighbors(chunk, &ChunkNeighbors::default(), skylight)
}

/// Opaque geometry with neighboring block data available across chunk boundaries.
pub fn mesh_chunk_with_neighbors(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    skylight: &Skylight,
) -> BlockGeometry {
    Mesher::new(chunk, neighbors, skylight, None, false, 0, 0)
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
    Mesher::new(
        chunk,
        &ChunkNeighbors::default(),
        skylight,
        None,
        fancy_graphics,
        0,
        0,
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
/// Two texels of a 16-pixel block, matching Beta's still-water surface drop.
const WATER_SURFACE_DROP: f32 = 2.0 / 16.0;
/// Lava uses the same inset surface plane as water, while remaining opaque.
const LAVA_SURFACE_DROP: f32 = 2.0 / 16.0;
const DEFAULT_GRASS_TINT: [f32; 3] = [0.55, 0.8, 0.4];

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
    fn push_ladder(&mut self, origin: [f32; 3], block: Id) {
        let (face_index, coordinate) = match block.ladder_support_offset() {
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
    fn push_torch(&mut self, origin: [f32; 3], block: Id) {
        let unit_cube = BlockFaceGeometry::unit_cube();
        for (face_index, face) in FACES.iter().enumerate() {
            if face_index == FACE_BOTTOM {
                continue;
            }
            let corners = unit_cube.face(face_index).corners.map(|corner| {
                torch_point(
                    block,
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
                torch_normal(block, face.normal),
                corners,
                texels,
                [1.0; 3],
                CornerShading::FULL_BRIGHT,
            );
        }
    }

    /// Two 1×1 squares rotated 45° around Y. One winding each; the plant
    /// material draws both sides.
    fn push_crossed_plant(
        &mut self,
        origin: [f32; 3],
        world: [i32; 3],
        block: Id,
        grass_tint: [f32; 3],
        light: u8,
    ) {
        // Beta jitters only Block.tallGrass in renderBlockReed. Fern is its
        // metadata-2 equivalent here; flowers and mushrooms stay centered.
        let [dx, mut dy, dz] = if matches!(block, Id::TallGrass | Id::Fern) {
            crossed_plant_offset(world[0], world[1], world[2])
        } else {
            [0.0; 3]
        };
        if matches!(block, Id::BrownMushroom | Id::RedMushroom) {
            dy = 2.0 / 16.0;
        }
        let center_x = 0.5 + dx;
        let center_z = 0.5 + dz;
        // RenderBlocks.renderCrossedSquares uses endpoints at +/-0.45 block.
        let half = if block == Id::SugarCane {
            0.45
        } else {
            0.5 / std::f32::consts::SQRT_2
        };
        let tint = if matches!(block, Id::TallGrass | Id::Fern) {
            grass_tint
        } else {
            [1.0, 1.0, 1.0]
        };
        let (tile_x, tile_y) = block_tile(block, 0, false);
        let texels = tile_texels(tile_x, tile_y, [[0, 0], [0, 16], [16, 16], [16, 0]]);
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
        // Crossed squares take the plant cell's own light, with no face
        // shade or corner occlusion.
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: false,
        };
        for (normal, corners) in quads {
            self.push_block_quad(origin, normal, corners, texels, tint, shading);
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
        block: Id,
        fancy_graphics: bool,
        y_drop: f32,
        tile_override: Option<(u8, u8)>,
    ) {
        let texels = tile_override.map_or_else(
            || face_texels_for_block(block, face_index, fancy_graphics),
            |(tile_x, tile_y)| face_texels(tile_x, tile_y, face_index),
        );
        let shape_height = match block {
            Id::SnowLayer => 0.125,
            Id::Farmland => 15.0 / 16.0,
            _ => 1.0,
        };
        let corners = geometry
            .corners
            .map(|corner| [corner[0], corner[1] * shape_height - y_drop, corner[2]]);
        self.push_block_quad(origin, face.normal, corners, texels, tint, shading);
    }

    /// Beta's fancy grass pass: the transparent overlay tile contains only
    /// the hanging grass pixels, leaving the normal dirt side unmodified.
    /// Coplanar with the dirt side face, not nudged outward: both quads pack
    /// their corners into the same quantized vertex positions (see
    /// `vertex.rs`) and share the opaque layer's depth test, so the mask
    /// layer's fragments resolve deterministically without z-fighting.
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
    ) -> Self {
        Self {
            chunk,
            neighbors,
            skylight,
            tints,
            fancy_graphics,
            origin_x,
            origin_z,
        }
    }

    /// Mesh the chunk layers in `rows`, with positions relative to `y_origin`.
    fn region(&self, rows: Range<usize>, y_origin: usize) -> ChunkMeshes {
        let chunk = self.chunk;
        let neighbors = self.neighbors;
        let skylight = self.skylight;
        let fancy_graphics = self.fancy_graphics;
        let mut meshes = ChunkMeshes {
            opaque: BlockGeometry::default(),
            grass_overlay: BlockGeometry::default(),
            cutout: BlockGeometry::default(),
            masked: BlockGeometry::default(),
            water: BlockGeometry::default(),
        };

        for y in rows {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let block = chunk.get(x, y, z).unwrap();
                    if block == Id::Air {
                        continue;
                    }
                    let origin = [x as f32, (y - y_origin) as f32, z as f32];
                    if is_torch(block) {
                        meshes.grass_overlay.push_torch(origin, block);
                        continue;
                    }
                    if block.is_ladder() {
                        meshes.masked.push_ladder(origin, block);
                        continue;
                    }
                    let column = z * CHUNK_SIZE + x;
                    let grass_tint = self
                        .tints
                        .as_ref()
                        .map_or(DEFAULT_GRASS_TINT, |tints| tints.grass[column]);
                    if is_crossed_plant(block) {
                        let light = skylight.channels_at(x as i32, y as i32, z as i32);
                        meshes.masked.push_crossed_plant(
                            origin,
                            [self.origin_x + x as i32, y as i32, self.origin_z + z as i32],
                            block,
                            grass_tint,
                            light,
                        );
                        continue;
                    }

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
                        if is_surface_liquid(block) && face_index != FACE_TOP {
                            continue;
                        }

                        let nx = x as i32 + face.neighbor[0];
                        let ny = y as i32 + face.neighbor[1];
                        let nz = z as i32 + face.neighbor[2];
                        // Nothing can see the underside of the world's bottom
                        // layer from inside the world.
                        if ny < 0 {
                            continue;
                        }
                        let neighbor = neighbors.get(chunk, nx, ny, nz);
                        if neighbor.is_some_and(|neighbor| same_surface_liquid(block, neighbor)) {
                            continue;
                        }
                        if neighbor_hides_face(block, neighbor, fancy_graphics) {
                            continue;
                        }

                        let grass_side = block == Id::Grass
                            && face_index != FACE_TOP
                            && face_index != FACE_BOTTOM;
                        let base = if block == Id::Grass && face_index == FACE_TOP {
                            grass_tint
                        } else {
                            block_tint(
                                block,
                                self.tints.as_ref().map(|tints| tints.foliage[column]),
                            )
                        };
                        let layer = if block == Id::Water {
                            &mut meshes.water
                        } else if block == Id::Cactus {
                            &mut meshes.masked
                        } else if fancy_graphics && is_leaf(block) {
                            &mut meshes.cutout
                        } else {
                            &mut meshes.opaque
                        };
                        let y_drop = match block {
                            Id::Water => WATER_SURFACE_DROP,
                            Id::Lava | Id::FlowingLava => LAVA_SURFACE_DROP,
                            _ => 0.0,
                        };
                        let chest_tile = chest_pair
                            .map(|direction| double_chest_tile(block, direction, face_index));
                        let wet_farmland_top = block == Id::Farmland
                            && face_index == FACE_TOP
                            && farmland_has_nearby_water(chunk, neighbors, x, y, z);
                        let tile_override = wet_farmland_top
                            .then(|| farmland_top_tile(true))
                            .or(chest_tile);
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
                            fancy_graphics,
                            y_drop,
                            tile_override,
                        );
                        if grass_side && fancy_graphics {
                            meshes
                                .grass_overlay
                                .push_grass_overlay(origin, face, face_index, grass_tint, shading);
                        }
                    }
                }
            }
        }
        meshes
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
        .is_some_and(is_opaque_cube)
}

fn is_leaf(block: Id) -> bool {
    matches!(block, Id::Leaves | Id::SpruceLeaves | Id::BirchLeaves)
}

fn is_surface_liquid(block: Id) -> bool {
    matches!(block, Id::Water | Id::Lava | Id::FlowingLava)
}

fn same_surface_liquid(block: Id, neighbor: Id) -> bool {
    matches!((block, neighbor), (Id::Water, Id::Water))
        || matches!(
            (block, neighbor),
            (Id::Lava | Id::FlowingLava, Id::Lava | Id::FlowingLava)
        )
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
                .is_some_and(Id::is_chest)
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
                    .is_some_and(Id::is_chest)
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

fn double_chest_tile(block: Id, pair_direction: [i32; 3], face: usize) -> (u8, u8) {
    if face == FACE_TOP || face == FACE_BOTTOM {
        return (9, 1);
    }
    let facing = block.chest_facing().unwrap_or_default();
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
    let first_is_left = matches!(
        facing,
        crate::block::id::FurnaceFacing::North | crate::block::id::FurnaceFacing::East
    );
    let left_half = first_half == first_is_left;
    let tile_x = if left_half { 9 } else { 10 };
    let tile_y = if face == front { 2 } else { 3 };
    (tile_x, tile_y)
}

fn farmland_has_nearby_water(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
) -> bool {
    for dy in 0..=1 {
        for dz in -4..=4 {
            for dx in -4..=4 {
                if neighbors
                    .get(chunk, x as i32 + dx, y as i32 + dy, z as i32 + dz)
                    .is_some_and(|block| matches!(block, Id::Water | Id::FlowingWater))
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Fast leaves hide every non-air neighbour, like any solid cube. Fancy leaves
/// are cutout, so leaf-to-leaf faces stay visible and solid faces towards a
/// canopy are not covered. Water never hides a neighbour, so lake beds and
/// walls stay visible under the surface plane.
fn neighbor_hides_face(block: Id, neighbor: Option<Id>, fancy_graphics: bool) -> bool {
    let Some(neighbor) = neighbor else {
        return false;
    };
    if neighbor == Id::Air
        || neighbor == Id::SnowLayer
        || neighbor == Id::Cactus
        || neighbor == Id::Farmland
        || neighbor == Id::Water
        || neighbor == Id::FlowingWater
        || neighbor == Id::Lava
        || neighbor == Id::FlowingLava
        || neighbor == Id::MobSpawner
        || neighbor.is_chest()
        || neighbor.is_ladder()
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

fn face_texels_for_block(block: Id, face: usize, fancy_graphics: bool) -> [AtlasTexel; 4] {
    let (tile_x, tile_y) = block_tile(block, face, fancy_graphics);
    face_texels(tile_x, tile_y, face)
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

fn block_tint(block: Id, foliage: Option<[f32; 3]>) -> [f32; 3] {
    if block.is_lit_furnace() {
        // The active face is a small flame, but Beta's lit furnace body also
        // appears subtly brighter than the idle block.
        return [BRIGHT_TINT; 3];
    }
    match block {
        Id::Water => [0.4, 0.6, 0.95],
        Id::Leaves => foliage.unwrap_or([0.28, 0.71, 0.09]),
        Id::BirchLeaves => linear_rgb(128, 167, 85),
        Id::SpruceLeaves => linear_rgb(97, 153, 97),
        // Beta 1.7.3 only has the oak plank tile. These species variants use
        // that tile with a color multiplier, matching the game's tint path.
        Id::SprucePlanks => linear_rgb(214, 177, 131),
        Id::BirchPlanks => linear_rgb(255, 246, 218),
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
    block: Id,
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
        let grass_side = block == Id::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
        let tint = if block == Id::Grass && face_index == FACE_TOP {
            grass_tint
        } else {
            block_tint(block, Some(foliage_tint))
        };
        // Items carry their face shade in the tint and ignore world light, so
        // they look the same under every lighting mode and time of day.
        let shade = face_shade(face.normal);
        body.push_block_quad(
            centered,
            face.normal,
            face_geometry.corners,
            face_texels_for_block(block, face_index, fancy_graphics),
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
        cutout: fancy_graphics && is_leaf(block),
        alpha_masked: block == Id::Cactus,
    }
}

use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::block::fluids::flow_vector;
use crate::rendering::textures::FoliageColors;
use crate::rendering::textures::GrassColors;
use crate::world::biome::BiomeMap;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;
use crate::world::lighting::Skylight;

mod bed;
mod fire;
pub(crate) mod geometry;
mod greedy;
mod piston;
mod quads;
mod redstone;
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
use self::geometry::FACE_TOP;

mod dropped;
mod faces;
mod mesher;
mod planes;
mod shapes;

pub use dropped::DroppedBlockMeshes;
pub use dropped::dropped_block_meshes;
use faces::box_texels;
use faces::face_corner_ao;
use faces::face_corner_light;
use faces::face_texels;
use faces::neighbor_hides_face_at;
use faces::torch_texels;
use mesher::Mesher;

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

const LAYER_OPAQUE: u8 = 0;
const LAYER_OVERLAY: u8 = 1;
const LAYER_CUTOUT: u8 = 2;
const LAYER_WATER: u8 = 3;
const LAYER_MASKED: u8 = 4;
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

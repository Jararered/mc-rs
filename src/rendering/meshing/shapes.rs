//! Quads for the shapes that are not cubes: ladders, portals, torches,
//! crossed plants and crops, plus the face and quad writers the rest use.

use super::CROP_DROP;
use super::CornerShading;
use super::FACES;
use super::Face;
use super::crossed_plant_offset;
use super::faces::face_texels;
use super::faces::tile_texels;
use super::faces::torch_texels;
use super::geometry::BlockFaceGeometry;
use super::geometry::FACE_BOTTOM;
use super::geometry::FACE_EAST;
use super::geometry::FACE_NORTH;
use super::geometry::FACE_SOUTH;
use super::geometry::FACE_WEST;
use super::geometry::FaceGeometry;
use super::vertex::AtlasTexel;
use super::vertex::BlockGeometry;
use super::vertex::BlockVertex;
use crate::block::blocks::Block;
use crate::block::properties::torch_normal;
use crate::block::properties::torch_point;
use crate::rendering::textures::block_tile;
use crate::rendering::textures::crop_tile;

impl BlockGeometry {
    /// Beta's ladder is one transparent wall plane. The saved facing names the
    /// supporting wall; draw the texture toward the room side.
    pub(super) fn push_ladder(&mut self, origin: [f32; 3], metadata: u8) {
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

    /// `BlockPortal.setBlockBoundsBasedOnState`: a pane a quarter block
    /// thick, of which only the two broad faces are ever drawn. `along_x`
    /// says the portal's plane runs along x, so the pane is thin in z.
    pub(super) fn push_portal(&mut self, origin: [f32; 3], along_x: bool) {
        let unit_cube = BlockFaceGeometry::unit_cube();
        let (axis, faces) = if along_x {
            (2, [FACE_SOUTH, FACE_NORTH])
        } else {
            (0, [FACE_EAST, FACE_WEST])
        };
        let (tile_x, tile_y) = crate::rendering::textures::PORTAL_TILE;
        for face_index in faces {
            let normal = FACES[face_index].normal;
            let coordinate = if normal[axis] > 0.0 { 0.625 } else { 0.375 };
            let corners = unit_cube.face(face_index).corners.map(|mut corner| {
                corner[axis] = coordinate;
                corner
            });
            self.push_block_quad(
                origin,
                normal,
                corners,
                face_texels(tile_x, tile_y, face_index),
                [1.0; 3],
                CornerShading::FULL_BRIGHT,
            );
        }
    }

    /// Build the same post for floor and wall attachments, then rotate its
    /// vertices and normals together so the cap follows the shaft. A redstone
    /// torch shares the shape and swaps the tile.
    pub(super) fn push_torch(&mut self, origin: [f32; 3], block: Block, metadata: u8) {
        let facing = Block::Torch.facing(metadata & 7);
        let tile = match block {
            Block::RedstoneTorch => (3, 6),
            Block::UnlitRedstoneTorch => (3, 7),
            _ => (0, 5),
        };
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
            self.push_block_quad(
                origin,
                torch_normal(facing, face.normal),
                corners,
                torch_texels(tile, face_index),
                [1.0; 3],
                CornerShading::FULL_BRIGHT,
            );
        }
    }

    /// `RenderBlocks.renderCrossedSquares`: two diagonal planes with endpoints
    /// 0.45 block either side of the cell center, each drawn once per side
    /// because the plant material culls back faces.
    pub(super) fn push_crossed_plant(
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
    pub(super) fn push_crops(&mut self, origin: [f32; 3], stage: u8, light: u8) {
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

    pub(super) fn push_face(
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
    pub(super) fn push_grass_overlay(
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
    pub(super) fn push_two_sided_quad(
        &mut self,
        origin: [f32; 3],
        corners: [[f32; 3]; 4],
        texels: [AtlasTexel; 4],
        tint: [f32; 3],
        shading: CornerShading,
    ) {
        self.push_quad_both_ways(origin, corners, texels, tint, shading, false);
    }

    /// A quad plus its reverse with every texel staying on its own corner, as
    /// `renderBlockMinecartTrack` emits them. The reverse is the side seen
    /// from above, so a curve keeps the shape its texture has.
    pub(super) fn push_glued_two_sided_quad(
        &mut self,
        origin: [f32; 3],
        corners: [[f32; 3]; 4],
        texels: [AtlasTexel; 4],
        tint: [f32; 3],
        shading: CornerShading,
    ) {
        self.push_quad_both_ways(origin, corners, texels, tint, shading, true);
    }

    pub(super) fn push_quad_both_ways(
        &mut self,
        origin: [f32; 3],
        corners: [[f32; 3]; 4],
        texels: [AtlasTexel; 4],
        tint: [f32; 3],
        shading: CornerShading,
        glued: bool,
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
        let back = if glued {
            [texels[3], texels[2], texels[1], texels[0]]
        } else {
            texels
        };
        let shades = if glued {
            CornerShading {
                light: [
                    shading.light[3],
                    shading.light[2],
                    shading.light[1],
                    shading.light[0],
                ],
                ao: [shading.ao[3], shading.ao[2], shading.ao[1], shading.ao[0]],
                shade: shading.shade,
            }
        } else {
            shading
        };
        self.push_block_quad(origin, normal.map(|v| -v), reversed, back, tint, shades);
    }

    pub(super) fn push_block_quad(
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

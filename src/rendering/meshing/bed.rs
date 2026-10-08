//! `RenderBlocks.renderBlockBed`: a low box whose underside is raised off
//! the floor, whose top turns with the bed, and whose two halves do not draw
//! the face where they meet.

use crate::block::bed;
use crate::block::blocks::Block;
use crate::rendering::textures::block_tile;

use super::AtlasTexel;
use super::BlockFaceGeometry;
use super::BlockGeometry;
use super::CornerShading;
use super::FACE_BOTTOM;
use super::FACE_TOP;
use super::FACES;
use super::Mesher;
use super::box_texels;
use super::face_corner_ao;
use super::face_corner_light;
use super::neighbor_hides_face_at;

/// Java sides (down, up, north, south, west, east) as `FACES` indices.
const JAVA_FACE: [usize; 6] = [1, 0, 5, 4, 3, 2];
/// `ModelBed.headInvisibleFace`.
const HEAD_INVISIBLE_FACE: [usize; 4] = [3, 4, 2, 5];
/// `ModelBed.footInvisibleFaceRemap`.
const FOOT_INVISIBLE_FACE_REMAP: [usize; 4] = [2, 3, 0, 1];
/// The side `renderBlockBed` draws with `flipTexture`, by direction.
const FLIPPED_FACE: [usize; 4] = [5, 3, 4, 2];
/// The mattress top and the underside of the frame.
const TOP: f32 = 0.5625;
const UNDERSIDE: f32 = 0.1875;

impl Mesher<'_> {
    pub(super) fn push_bed(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let direction = bed::direction(metadata);
        let hidden = JAVA_FACE[if bed::is_foot(metadata) {
            HEAD_INVISIBLE_FACE[FOOT_INVISIBLE_FACE_REMAP[direction]]
        } else {
            HEAD_INVISIBLE_FACE[direction]
        }];
        let flipped = JAVA_FACE[FLIPPED_FACE[direction]];
        let own_light = [[self.skylight.channels_at(x as i32, y as i32, z as i32); 4]; 4];
        let body = BlockFaceGeometry::from_bounds([0.0, 0.0, 0.0, 1.0, TOP, 1.0]);
        let raised = BlockFaceGeometry::from_bounds([0.0, UNDERSIDE, 0.0, 1.0, TOP, 1.0]);
        for (face_index, face) in FACES.iter().enumerate() {
            let tile = block_tile(Block::Bed, metadata, face_index, self.fancy_graphics);
            let (face_geometry, texels, shading) = match face_index {
                FACE_BOTTOM => {
                    let face_geometry = raised.face(face_index);
                    let texels = face_geometry.corners.map(|[u, _, v]| texel(tile, u, v));
                    let shading = CornerShading {
                        light: own_light,
                        ao: [0; 4],
                        shade: true,
                    };
                    (face_geometry, texels, shading)
                }
                FACE_TOP => {
                    let face_geometry = body.face(face_index);
                    let texels = face_geometry.corners.map(|[x, _, z]| {
                        let (u, v) = match direction {
                            0 => (z, x),
                            1 => (1.0 - x, z),
                            2 => (1.0 - z, 1.0 - x),
                            _ => (x, 1.0 - z),
                        };
                        texel(tile, u, v)
                    });
                    let shading = CornerShading {
                        light: face_corner_light(self.skylight, x, y, z, face, face_geometry),
                        ao: [0; 4],
                        shade: true,
                    };
                    (face_geometry, texels, shading)
                }
                _ => {
                    if face_index == hidden {
                        continue;
                    }
                    let [dx, dy, dz] = face.neighbor;
                    if neighbor_hides_face_at(
                        Block::Bed,
                        self.neighbors,
                        self.chunk,
                        (x as i32 + dx, y as i32 + dy, z as i32 + dz),
                        face_index,
                        self.fancy_graphics,
                    ) {
                        continue;
                    }
                    let face_geometry = body.face(face_index);
                    let texels = box_texels(
                        tile,
                        face_index,
                        face_geometry.corners,
                        face_index == flipped,
                    );
                    let shading = CornerShading {
                        light: face_corner_light(self.skylight, x, y, z, face, face_geometry),
                        ao: face_corner_ao(
                            self.chunk,
                            self.neighbors,
                            x,
                            y,
                            z,
                            face,
                            face_geometry,
                        ),
                        shade: true,
                    };
                    (face_geometry, texels, shading)
                }
            };
            mesh.push_block_quad(
                origin,
                face.normal,
                face_geometry.corners,
                texels,
                [1.0; 3],
                shading,
            );
        }
    }
}

fn texel(tile: (u8, u8), u: f32, v: f32) -> AtlasTexel {
    AtlasTexel::new(
        tile.0,
        tile.1,
        (u * 16.0).round() as u8,
        (v * 16.0).round() as u8,
    )
}

//! Facing-aware piston bodies and the plate-and-rod extension.

use crate::block::blocks::Block;
use crate::block::definition::redstone_bounds;

use super::AtlasTexel;
use super::BlockFaceGeometry;
use super::BlockGeometry;
use super::CornerShading;
use super::FACES;
use super::Mesher;
use super::face_corner_ao;
use super::face_corner_light;
use super::neighbor_hides_face_at;

// Java sides: down, up, north, south, west, east. FACES stores top,
// bottom, east, west, south, north instead.
const PISTON_FACE: [usize; 6] = [1, 0, 5, 4, 3, 2];
const OPPOSITE: [usize; 6] = [1, 0, 3, 2, 5, 4];
const AXIS: [usize; 6] = [1, 1, 0, 0, 2, 2];

impl Mesher<'_> {
    pub(super) fn push_piston(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
        block: Block,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let direction = (metadata & 7) as usize;
        if direction >= 6 {
            return;
        }
        let facing = PISTON_FACE[direction];
        let extended = metadata & 8 != 0;
        let head = block == Block::PistonHead;
        let sticky = if head {
            let [dx, dy, dz] = FACES[facing].neighbor;
            let behind = self
                .neighbors
                .cell(self.chunk, x as i32 - dx, y as i32 - dy, z as i32 - dz)
                .0;
            behind == Block::StickyPiston || extended
        } else {
            block == Block::StickyPiston
        };
        let (min, max) = redstone_bounds(block, metadata).unwrap_or(([0.0; 3], [1.0; 3]));
        let geometry =
            BlockFaceGeometry::from_bounds([min[0], min[1], min[2], max[0], max[1], max[2]]);
        for (face_index, face) in FACES.iter().enumerate() {
            let face_geometry = geometry.face(face_index);
            let [dx, dy, dz] = face.neighbor;
            if y as i32 + dy < 0 {
                continue;
            }
            let neighbor = (x as i32 + dx, y as i32 + dy, z as i32 + dz);
            let axis = AXIS[face_index];
            let at_boundary = if face.neighbor[axis] > 0 {
                max[axis] == 1.0
            } else {
                min[axis] == 0.0
            };
            if at_boundary
                && neighbor_hides_face_at(
                    block,
                    self.neighbors,
                    self.chunk,
                    neighbor,
                    face_index,
                    self.fancy_graphics,
                )
            {
                continue;
            }
            let tile = if face_index == facing {
                if head || !extended {
                    (if sticky { 10 } else { 11 }, 6)
                } else {
                    (14, 6)
                }
            } else if face_index == OPPOSITE[facing] {
                (if head { 11 } else { 13 }, 6)
            } else {
                (12, 6)
            };
            let texels = face_geometry.corners.map(|corner| {
                if face_index == facing || face_index == OPPOSITE[facing] {
                    piston_texel(tile, face_index, corner)
                } else {
                    piston_side_texel(tile, direction, face_index, corner)
                }
            });
            let shading = CornerShading {
                light: face_corner_light(self.skylight, x, y, z, face, face_geometry),
                ao: face_corner_ao(self.chunk, self.neighbors, x, y, z, face, face_geometry),
                shade: true,
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

        if !head {
            return;
        }
        let axis = AXIS[facing];
        let mut rod_min = [0.375; 3];
        let mut rod_max = [0.625; 3];
        if direction % 2 == 0 {
            rod_min[axis] = max[axis];
            rod_max[axis] = 1.25;
        } else {
            rod_min[axis] = -0.25;
            rod_max[axis] = min[axis];
        }
        let rod = BlockFaceGeometry::from_bounds([
            rod_min[0], rod_min[1], rod_min[2], rod_max[0], rod_max[1], rod_max[2],
        ]);
        for (face_index, face) in FACES.iter().enumerate() {
            if face_index == facing || face_index == OPPOSITE[facing] {
                continue;
            }
            let face_geometry = rod.face(face_index);
            let cross_axis = (0..3)
                .find(|&index| index != axis && face.normal[index] == 0.0)
                .unwrap();
            let texels = face_geometry.corners.map(|corner| {
                let u = (corner[axis] - rod_min[axis]) / (rod_max[axis] - rod_min[axis]);
                let v = corner[cross_axis] - rod_min[cross_axis];
                AtlasTexel::new(12, 6, (u * 16.0).round() as u8, (v * 16.0).round() as u8)
            });
            let shading = CornerShading {
                light: face_corner_light(self.skylight, x, y, z, face, face_geometry),
                ao: [0; 4],
                shade: true,
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

fn piston_texel(tile: (u8, u8), face: usize, [x, y, z]: [f32; 3]) -> AtlasTexel {
    let (u, v) = match face {
        0 => (x, z),
        1 => (z, x),
        2 | 3 => (z, 1.0 - y),
        _ => (x, 1.0 - y),
    };
    AtlasTexel::new(
        tile.0,
        tile.1,
        (u * 16.0).round() as u8,
        (v * 16.0).round() as u8,
    )
}

fn piston_side_texel(
    tile: (u8, u8),
    direction: usize,
    face: usize,
    corner: [f32; 3],
) -> AtlasTexel {
    // Texture 108 has the head rim in its first four V texels. Rotate the
    // side texture so V always measures distance away from the piston face.
    let depth_axis = AXIS[PISTON_FACE[direction]];
    let width_axis = (0..3)
        .find(|&axis| axis != depth_axis && FACES[face].normal[axis] == 0.0)
        .unwrap();
    let u = corner[width_axis];
    let v = if direction % 2 == 0 {
        corner[depth_axis]
    } else {
        1.0 - corner[depth_axis]
    };
    AtlasTexel::new(
        tile.0,
        tile.1,
        (u * 16.0).round() as u8,
        (v * 16.0).round() as u8,
    )
}

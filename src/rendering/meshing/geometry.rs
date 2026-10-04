//! Reusable face-local geometry for block meshes.
//!
//! A block's faces own independent corner arrays, so moving one face does not
//! require deforming the neighboring face's shared vertices.

use crate::block::blocks::Block;

pub(crate) const FACE_TOP: usize = 0;
pub(crate) const FACE_BOTTOM: usize = 1;
pub(crate) const FACE_EAST: usize = 2;
pub(crate) const FACE_WEST: usize = 3;
pub(crate) const FACE_SOUTH: usize = 4;
pub(crate) const FACE_NORTH: usize = 5;
pub(crate) const FACE_COUNT: usize = 6;
pub(crate) const CACTUS_FACE_INSET: f32 = 1.0 / 16.0;

pub(crate) const FACE_NORMALS: [[f32; 3]; FACE_COUNT] = [
    [0.0, 1.0, 0.0],
    [0.0, -1.0, 0.0],
    [1.0, 0.0, 0.0],
    [-1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, -1.0],
];

const UNIT_FACE_CORNERS: [[[f32; 3]; 4]; FACE_COUNT] = [
    [
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
    ],
    [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
    ],
    [
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
        [1.0, 0.0, 1.0],
    ],
    [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [0.0, 1.0, 0.0],
    ],
    [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ],
    [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ],
];

const UNIT_FACE_UVS: [[[f32; 2]; 4]; FACE_COUNT] = [
    [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
    [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
    [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct FaceGeometry {
    pub normal: [f32; 3],
    pub corners: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BlockFaceGeometry {
    faces: [FaceGeometry; FACE_COUNT],
}

impl BlockFaceGeometry {
    pub(crate) fn unit_cube() -> Self {
        Self {
            faces: std::array::from_fn(|index| FaceGeometry {
                normal: FACE_NORMALS[index],
                corners: UNIT_FACE_CORNERS[index],
                uvs: UNIT_FACE_UVS[index],
            }),
        }
    }

    pub(crate) fn from_bounds([x0, y0, z0, x1, y1, z1]: [f32; 6]) -> Self {
        let mut geometry = Self::unit_cube();
        for face in &mut geometry.faces {
            for corner in &mut face.corners {
                corner[0] = x0 + corner[0] * (x1 - x0);
                corner[1] = y0 + corner[1] * (y1 - y0);
                corner[2] = z0 + corner[2] * (z1 - z0);
            }
        }
        geometry
    }

    pub(crate) fn cactus() -> Self {
        let mut geometry = Self::from_bounds([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        for face_index in [FACE_EAST, FACE_WEST, FACE_SOUTH, FACE_NORTH] {
            let normal = geometry.faces[face_index].normal;
            for corner in &mut geometry.faces[face_index].corners {
                for (axis, component) in normal.into_iter().enumerate() {
                    corner[axis] -= component * CACTUS_FACE_INSET;
                }
            }
        }
        geometry
    }

    pub(crate) fn for_block(block: Block) -> Self {
        if block == Block::Cactus {
            Self::cactus()
        } else {
            Self::unit_cube()
        }
    }

    pub(crate) fn face(&self, index: usize) -> FaceGeometry {
        self.faces[index]
    }

    pub(crate) fn faces(&self) -> &[FaceGeometry; FACE_COUNT] {
        &self.faces
    }
}

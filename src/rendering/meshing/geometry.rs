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

/// One axis-aligned box of a shaped block, as `[x0, y0, z0, x1, y1, z1]`
/// inside its cell.
pub(crate) type Bounds = [f32; 6];

/// The boxes a shaped block is drawn as. A fence with rails both ways has
/// five; everything else fewer.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BlockBoxes {
    boxes: [Bounds; 5],
    len: usize,
}

impl BlockBoxes {
    fn push(&mut self, bounds: Bounds) {
        self.boxes[self.len] = bounds;
        self.len += 1;
    }

    pub(crate) fn as_slice(&self) -> &[Bounds] {
        &self.boxes[..self.len]
    }
}

fn flat((min, max): ([f32; 3], [f32; 3])) -> Bounds {
    [min[0], min[1], min[2], max[0], max[1], max[2]]
}

/// Whether [`block_boxes`] draws `block`.
pub(crate) fn is_box_shape(block: Block) -> bool {
    matches!(
        block,
        Block::StoneSlab
            | Block::Fence
            | Block::Trapdoor
            | Block::StoneButton
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::StandingSign
            | Block::WallSign
    ) || block.is_stairs()
        || block.is_door()
}

/// Beta's box render types: `BlockStep`'s half block, `renderBlockStairs`,
/// `renderBlockFence`, and the door and trapdoor panels. `fence_links` says
/// whether the west, east, north, and south neighbors are fences.
pub(crate) fn block_boxes(block: Block, metadata: u8, fence_links: [bool; 4]) -> BlockBoxes {
    let mut boxes = BlockBoxes::default();
    match block {
        Block::StoneSlab => boxes.push([0.0, 0.0, 0.0, 1.0, 0.5, 1.0]),
        Block::WoodenStairs | Block::CobblestoneStairs => {
            for bounds in block.collision_boxes_for(metadata).into_iter().flatten() {
                boxes.push(flat(bounds));
            }
        }
        Block::WoodenDoor
        | Block::IronDoor
        | Block::Trapdoor
        | Block::StoneButton
        | Block::StonePressurePlate
        | Block::WoodenPressurePlate => {
            boxes.push(flat(block.selection_bounds_for(metadata)));
        }
        Block::WallSign => boxes.push(flat(block.selection_bounds_for(metadata))),
        // `ModelSign` at two thirds: a post and a board on it. The board
        // turns in sixteenths of a circle; boxes follow the nearer axis.
        Block::StandingSign => {
            let (near, far) = (0.5 - 1.0 / 24.0, 0.5 + 1.0 / 24.0);
            let top = 7.0 / 12.0;
            boxes.push([near, 0.0, near, far, top, far]);
            if (metadata + 2) % 8 < 4 {
                boxes.push([0.0, top, near, 1.0, top + 0.5, far]);
            } else {
                boxes.push([near, top, 0.0, far, top + 0.5, 1.0]);
            }
        }
        Block::Fence => {
            let [west, east, north, south] = fence_links;
            boxes.push([0.375, 0.0, 0.375, 0.625, 1.0, 0.625]);
            let along_z = north || south;
            // A lone post still gets stub rails along x.
            let along_x = west || east || !along_z;
            let (near, far) = (0.4375, 0.5625);
            let x0 = if west { 0.0 } else { near };
            let x1 = if east { 1.0 } else { far };
            let z0 = if north { 0.0 } else { near };
            let z1 = if south { 1.0 } else { far };
            for (y0, y1) in [(0.75, 0.9375), (0.375, 0.5625)] {
                if along_x {
                    boxes.push([x0, y0, near, x1, y1, far]);
                }
                if along_z {
                    boxes.push([near, y0, z0, far, y1, z1]);
                }
            }
        }
        _ => {}
    }
    boxes
}

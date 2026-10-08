//! Redstone dust, repeaters, rails, and levers: the parts of Beta's redstone
//! family that are not a plain box (`renderBlockRedstoneWire`,
//! `renderBlockRepeater`, `renderBlockMinecartTrack`, `renderBlockLever`).

use std::f32::consts::FRAC_PI_2;
use std::f32::consts::PI;

use bevy::prelude::Color;

use crate::block::blocks::Block;
use crate::rendering::textures::block_tile;
use crate::world::block_ticks::behavior;

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
use super::face_texels;
use super::torch_texels;

const WIRE_Y: f32 = 1.0 / 64.0;
const HIGHLIGHT_OFFSET: f32 = 1.0 / 128.0;

impl Mesher<'_> {
    fn wire_connects(&self, x: i32, y: i32, z: i32, side: i8) -> bool {
        let (block, metadata) = self.neighbors.cell(self.chunk, x, y, z);
        if block == Block::RedstoneWire {
            return true;
        }
        // A source on the step above or below (`side` -1) still connects;
        // only a repeater has to face the dust.
        if matches!(block, Block::Repeater | Block::PoweredRepeater) {
            return i16::from(side) == [2, 3, 0, 1][usize::from(metadata & 3)];
        }
        behavior(block).can_provide_power()
    }

    /// `BlockRedstoneWire.colorMultiplier`: dark red at rest, brightening and
    /// yellowing with the power level.
    fn wire_tint(power: u8) -> [f32; 3] {
        let power = f32::from(power) / 15.0;
        let red = if power == 0.0 { 0.3 } else { power * 0.6 + 0.4 };
        let green = (power * power * 0.7 - 0.5).max(0.0);
        let blue = (power * power * 0.6 - 0.7).max(0.0);
        let linear = Color::srgb(red, green, blue).to_linear();
        [linear.red, linear.green, linear.blue]
    }

    /// Beta's repeater is a thin stone plate with a rotating top texture and
    /// two short redstone torches. The moving torch indicates the delay.
    pub(super) fn push_repeater(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
        block: Block,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let facing = metadata & 3;
        let delay = (metadata >> 2) & 3;
        let plate = BlockFaceGeometry::from_bounds([0.0, 0.0, 0.0, 1.0, 2.0 / 16.0, 1.0]);
        for (face_index, face) in FACES.iter().enumerate() {
            // `BlockRedstoneRepeater.shouldSideBeRendered` hides both the
            // underside and standard top; `RenderBlocks` draws the top rotated.
            if face_index == FACE_BOTTOM {
                continue;
            }
            let geometry = plate.face(face_index);
            let (tile_x, tile_y) = block_tile(block, metadata, face_index, false);
            let texels = if face_index == FACE_TOP {
                geometry.corners.map(|[cx, _, cz]| {
                    let (u, v) = match facing {
                        0 => (cx, cz),
                        1 => (cz, 1.0 - cx),
                        2 => (1.0 - cx, 1.0 - cz),
                        _ => (1.0 - cz, cx),
                    };
                    AtlasTexel::new(tile_x, tile_y, (u * 16.0) as u8, (v * 16.0) as u8)
                })
            } else {
                face_texels(tile_x, tile_y, face_index)
            };
            let shading = CornerShading {
                light: face_corner_light(self.skylight, x, y, z, face, geometry),
                ao: face_corner_ao(self.chunk, self.neighbors, x, y, z, face, geometry),
                shade: true,
            };
            mesh.push_block_quad(
                origin,
                face.normal,
                geometry.corners,
                texels,
                [1.0; 3],
                shading,
            );
        }

        // `renderBlockRepeater` puts a fixed torch at +/-5/16 and slides the
        // other in 2/16 increments as the delay increases.
        let movable = [-1.0, 1.0, 3.0, 5.0][delay as usize] / 16.0;
        let (fixed, adjustable) = match facing {
            0 => ([0.0, -5.0 / 16.0], [0.0, movable]),
            1 => ([5.0 / 16.0, 0.0], [-movable, 0.0]),
            2 => ([0.0, 5.0 / 16.0], [0.0, -movable]),
            _ => ([-5.0 / 16.0, 0.0], [movable, 0.0]),
        };
        let tile = if block == Block::PoweredRepeater {
            (3, 6)
        } else {
            (3, 7)
        };
        let light = self.skylight.channels_at(x as i32, y as i32, z as i32);
        for [dx, dz] in [fixed, adjustable] {
            let (cx, cz) = (0.5 + dx, 0.5 + dz);
            let torch = BlockFaceGeometry::from_bounds([
                cx - 1.0 / 16.0,
                2.0 / 16.0,
                cz - 1.0 / 16.0,
                cx + 1.0 / 16.0,
                7.0 / 16.0,
                cz + 1.0 / 16.0,
            ]);
            for (face_index, face) in FACES.iter().enumerate() {
                if face_index == FACE_BOTTOM {
                    continue;
                }
                mesh.push_block_quad(
                    origin,
                    face.normal,
                    torch.face(face_index).corners,
                    torch_texels(tile, face_index),
                    [1.0; 3],
                    CornerShading {
                        light: [[light; 4]; 4],
                        ao: [0; 4],
                        shade: false,
                    },
                );
            }
        }
    }

    pub(super) fn push_redstone_wire(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
    ) {
        let (x, y, z) = (x as i32, y as i32, z as i32);
        let cell = |dx, dy, dz| self.neighbors.cell(self.chunk, x + dx, y + dy, z + dz).0;
        let normal_cube = |dx, dy, dz| cell(dx, dy, dz).is_opaque_cube();
        let connects = |dx, dy, dz, side| self.wire_connects(x + dx, y + dy, z + dz, side);
        let directions = [(-1, 0, 1), (1, 0, 3), (0, -1, 2), (0, 1, 0)];
        let mut connected = directions.map(|(dx, dz, side)| {
            connects(dx, 0, dz, side) || (!normal_cube(dx, 0, dz) && connects(dx, -1, dz, -1))
        });
        if !normal_cube(0, 1, 0) {
            for (connection, (dx, dz, _)) in connected.iter_mut().zip(directions) {
                *connection |= normal_cube(dx, 0, dz) && connects(dx, 1, dz, -1);
            }
        }
        let [west, east, north, south] = connected;
        let straight_x = (west || east) && !north && !south;
        let straight_z = (north || south) && !west && !east;
        let tile_x = if straight_x || straight_z { 5 } else { 4 };
        let (x0, x1, z0, z1) = if straight_x || straight_z || !connected.contains(&true) {
            (0.0, 1.0, 0.0, 1.0)
        } else {
            (
                if west { 0.0 } else { 5.0 / 16.0 },
                if east { 1.0 } else { 11.0 / 16.0 },
                if north { 0.0 } else { 5.0 / 16.0 },
                if south { 1.0 } else { 11.0 / 16.0 },
            )
        };
        let tint = Self::wire_tint(self.chunk.metadata(x as usize, y as usize, z as usize));
        let light = self.skylight.channels_at(x, y, z);
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: false,
        };

        // The second row of dust tiles contains the pale highlights. Keep
        // them just above the tinted trace so the depth test cannot hide them.
        for (row, color, height) in [
            (10, tint, WIRE_Y),
            (11, [1.0; 3], WIRE_Y + HIGHLIGHT_OFFSET),
        ] {
            let corners = [
                [x0, height, z0],
                [x0, height, z1],
                [x1, height, z1],
                [x1, height, z0],
            ];
            let texels = corners.map(|[cx, _, cz]| {
                let (u, v) = if straight_z { (cz, cx) } else { (cx, cz) };
                AtlasTexel::new(tile_x, row, (u * 16.0) as u8, (v * 16.0) as u8)
            });
            mesh.push_block_quad(origin, [0.0, 1.0, 0.0], corners, texels, color, shading);
        }

        if normal_cube(0, 1, 0) {
            return;
        }
        for (dx, dz, face) in [(-1, 0, 0), (1, 0, 1), (0, -1, 2), (0, 1, 3)] {
            if !normal_cube(dx, 0, dz) || cell(dx, 1, dz) != Block::RedstoneWire {
                continue;
            }
            let normal = match face {
                0 => [1.0, 0.0, 0.0],
                1 => [-1.0, 0.0, 0.0],
                2 => [0.0, 0.0, 1.0],
                _ => [0.0, 0.0, -1.0],
            };
            for (row, color, offset) in [(10, tint, 0.0), (11, [1.0; 3], HIGHLIGHT_OFFSET)] {
                let plane = match face {
                    0 => 1.0 / 64.0 + offset,
                    1 => 1.0 - 1.0 / 64.0 - offset,
                    2 => 1.0 / 64.0 + offset,
                    _ => 1.0 - 1.0 / 64.0 - offset,
                };
                // The written order winds towards -x and +z; the faces
                // looking the other way (+x and -z) need it reversed or the
                // strip is back-face culled.
                let (corners, texels) = if face < 2 {
                    let mut corners = [
                        [plane, 0.0, 0.0],
                        [plane, 0.0, 1.0],
                        [plane, 1.0, 1.0],
                        [plane, 1.0, 0.0],
                    ];
                    if face == 0 {
                        corners.reverse();
                    }
                    let texels = corners.map(|[_, cy, cz]| {
                        AtlasTexel::new(5, row, (cy * 16.0) as u8, (cz * 16.0) as u8)
                    });
                    (corners, texels)
                } else {
                    let mut corners = [
                        [0.0, 0.0, plane],
                        [1.0, 0.0, plane],
                        [1.0, 1.0, plane],
                        [0.0, 1.0, plane],
                    ];
                    if face == 3 {
                        corners.reverse();
                    }
                    let texels = corners.map(|[cx, cy, _]| {
                        AtlasTexel::new(5, row, (cy * 16.0) as u8, (cx * 16.0) as u8)
                    });
                    (corners, texels)
                };
                mesh.push_block_quad(origin, normal, corners, texels, color, shading);
            }
        }
    }

    /// `renderBlockMinecartTrack`: one quad a sixteenth above the floor, with
    /// its ends lifted a block for an ascending rail, drawn from both sides.
    pub(super) fn push_rail(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
        block: Block,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let (tile_x, tile_y) = block_tile(block, metadata, FACE_TOP, false);
        // Powered rails keep the power bit above the shape.
        let shape = if block == Block::Rail {
            metadata & 15
        } else {
            metadata & 7
        };
        // Vertex order follows Beta: (+x, -z), (+x, +z), (-x, +z), (-x, -z).
        let mut xs = [1.0, 1.0, 0.0, 0.0];
        let mut zs = [0.0, 1.0, 1.0, 0.0];
        match shape {
            1..=3 | 7 => {
                xs = [1.0, 0.0, 0.0, 1.0];
                zs = [1.0, 1.0, 0.0, 0.0];
            }
            8 => {
                xs = [0.0, 0.0, 1.0, 1.0];
                zs = [1.0, 0.0, 0.0, 1.0];
            }
            9 => {
                xs = [0.0, 1.0, 1.0, 0.0];
                zs = [0.0, 0.0, 1.0, 1.0];
            }
            _ => {}
        }
        let mut ys = [1.0 / 16.0; 4];
        match shape {
            2 | 4 => {
                ys[0] += 1.0;
                ys[3] += 1.0;
            }
            3 | 5 => {
                ys[1] += 1.0;
                ys[2] += 1.0;
            }
            _ => {}
        }
        let corners: [[f32; 3]; 4] = std::array::from_fn(|i| [xs[i], ys[i], zs[i]]);
        let texels = [
            AtlasTexel::new(tile_x, tile_y, 16, 0),
            AtlasTexel::new(tile_x, tile_y, 16, 16),
            AtlasTexel::new(tile_x, tile_y, 0, 16),
            AtlasTexel::new(tile_x, tile_y, 0, 0),
        ];
        let light = self.skylight.channels_at(x as i32, y as i32, z as i32);
        mesh.push_two_sided_quad(
            origin,
            corners,
            texels,
            [1.0; 3],
            CornerShading {
                light: [[light; 4]; 4],
                ao: [0; 4],
                shade: false,
            },
        );
    }

    /// `renderBlockLever`: a cobblestone base against its support and a
    /// stick that leans one way when on and the other when off.
    pub(super) fn push_lever(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let facing = metadata & 7;
        let light = self.skylight.channels_at(x as i32, y as i32, z as i32);
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: true,
        };
        // The base never moves.
        let base = BlockFaceGeometry::from_bounds(lever_base_bounds(facing));
        for (face_index, face) in FACES.iter().enumerate() {
            let corners = base.face(face_index).corners;
            mesh.push_block_quad(
                origin,
                face.normal,
                corners,
                box_texels((0, 1), face_index, corners, false),
                [1.0; 3],
                shading,
            );
        }
        // The stick is Beta's eight vertices, drawn with texels 7..9 of its
        // tile: rows 6..8 on the two ends and 6..16 along the sides.
        let stick = lever_stick(facing, metadata & 8 != 0);
        let centre: [f32; 3] =
            std::array::from_fn(|axis| stick.iter().map(|v| v[axis]).sum::<f32>() / 8.0);
        for (index, order) in LEVER_STICK_FACES.iter().enumerate() {
            let mut corners = order.map(|vertex| stick[vertex]);
            let (top, bottom) = if index < 2 { (6, 8) } else { (6, 16) };
            let mut texels = [(7, bottom), (9, bottom), (9, top), (7, top)]
                .map(|(u, v)| AtlasTexel::new(0, 6, u, v));
            let edge = |a: [f32; 3], b: [f32; 3]| [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let (first, second) = (edge(corners[0], corners[1]), edge(corners[1], corners[2]));
            let mut normal = [
                first[1] * second[2] - first[2] * second[1],
                first[2] * second[0] - first[0] * second[2],
                first[0] * second[1] - first[1] * second[0],
            ];
            let middle: [f32; 3] =
                std::array::from_fn(|axis| corners.iter().map(|c| c[axis]).sum::<f32>() / 4.0);
            let outward: f32 = (0..3)
                .map(|axis| normal[axis] * (middle[axis] - centre[axis]))
                .sum();
            if outward < 0.0 {
                // Beta's winding is for a client that culls differently.
                corners.reverse();
                texels.reverse();
                normal = normal.map(|v| -v);
            }
            let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt();
            mesh.push_block_quad(
                origin,
                normal.map(|v| v / length),
                corners,
                texels,
                [1.0; 3],
                shading,
            );
        }
    }
}

/// `renderBlockLever`'s `setBlockBounds` for the cobblestone base: 5 and 6 sit
/// on the floor along x and z, 1 to 4 against the support on that side.
fn lever_base_bounds(facing: u8) -> [f32; 6] {
    const LONG: f32 = 0.25;
    const SHORT: f32 = 0.1875;
    match facing {
        6 => [0.5 - LONG, 0.0, 0.5 - SHORT, 0.5 + LONG, SHORT, 0.5 + SHORT],
        4 => [
            0.5 - SHORT,
            0.5 - LONG,
            1.0 - SHORT,
            0.5 + SHORT,
            0.5 + LONG,
            1.0,
        ],
        3 => [0.5 - SHORT, 0.5 - LONG, 0.0, 0.5 + SHORT, 0.5 + LONG, SHORT],
        2 => [
            1.0 - SHORT,
            0.5 - LONG,
            0.5 - SHORT,
            1.0,
            0.5 + LONG,
            0.5 + SHORT,
        ],
        1 => [0.0, 0.5 - LONG, 0.5 - SHORT, SHORT, 0.5 + LONG, 0.5 + SHORT],
        _ => [0.5 - SHORT, 0.0, 0.5 - LONG, 0.5 + SHORT, SHORT, 0.5 + LONG],
    }
}

/// The stick's vertices, indexed as in Beta: 0..4 the bottom, 4..8 the top,
/// each going round the square.
const LEVER_STICK_FACES: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [7, 6, 5, 4],
    [1, 0, 4, 5],
    [2, 1, 5, 6],
    [3, 2, 6, 7],
    [0, 3, 7, 4],
];

/// `Vec3D.rotateAroundX`.
fn rotate_x([x, y, z]: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    [x, y * cos + z * sin, z * cos - y * sin]
}

/// `Vec3D.rotateAroundY`.
fn rotate_y([x, y, z]: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    [x * cos + z * sin, y, z * cos - x * sin]
}

/// The eight stick vertices `renderBlockLever` builds, in cell coordinates:
/// a 2/16 square, 10/16 long, shifted and tipped toward the lever's on or off
/// side, then turned to the support.
fn lever_stick(facing: u8, on: bool) -> [[f32; 3]; 8] {
    const HALF: f32 = 0.0625;
    const LENGTH: f32 = 0.625;
    const TILT: f32 = 0.69813174;
    std::array::from_fn(|vertex| {
        let corner = [[-HALF, -HALF], [HALF, -HALF], [HALF, HALF], [-HALF, HALF]][vertex % 4];
        let mut point = [corner[0], if vertex < 4 { 0.0 } else { LENGTH }, corner[1]];
        if on {
            point[2] -= HALF;
            point = rotate_x(point, TILT);
        } else {
            point[2] += HALF;
            point = rotate_x(point, -TILT);
        }
        if facing == 6 {
            point = rotate_y(point, FRAC_PI_2);
        }
        if (1..5).contains(&facing) {
            point[1] -= 0.375;
            point = rotate_x(point, FRAC_PI_2);
            match facing {
                3 => point = rotate_y(point, PI),
                2 => point = rotate_y(point, FRAC_PI_2),
                1 => point = rotate_y(point, -FRAC_PI_2),
                _ => {}
            }
            [point[0] + 0.5, point[1] + 0.5, point[2] + 0.5]
        } else {
            [point[0] + 0.5, point[1] + 0.125, point[2] + 0.5]
        }
    })
}

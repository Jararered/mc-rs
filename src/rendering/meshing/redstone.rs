//! Redstone dust, repeaters, rails, and levers: the parts of Beta's redstone
//! family that are not a plain box (`renderBlockRedstoneWire`,
//! `renderBlockRepeater`, `renderBlockMinecartTrack`, `renderBlockLever`).

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
        if side < 0 {
            return false;
        }
        if matches!(block, Block::Repeater | Block::PoweredRepeater) {
            return metadata & 3 == [2, 3, 0, 1][side as usize % 4];
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
                let (corners, texels) = if face < 2 {
                    let corners = [
                        [plane, 0.0, 0.0],
                        [plane, 0.0, 1.0],
                        [plane, 1.0, 1.0],
                        [plane, 1.0, 0.0],
                    ];
                    let texels = corners.map(|[_, cy, cz]| {
                        AtlasTexel::new(5, row, (cy * 16.0) as u8, (cz * 16.0) as u8)
                    });
                    (corners, texels)
                } else {
                    let corners = [
                        [0.0, 0.0, plane],
                        [1.0, 0.0, plane],
                        [1.0, 1.0, plane],
                        [0.0, 1.0, plane],
                    ];
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
        let on = metadata & 8 != 0;
        // Rotation taking the floor lever's up axis onto the support's
        // outward normal, and the local direction the stick leans when on.
        let (to_world, lean): (fn([f32; 3]) -> [f32; 3], [f32; 3]) = match metadata & 7 {
            1 => (|[x, y, z]| [y, -x, z], [-1.0, 0.0, 0.0]),
            2 => (|[x, y, z]| [-y, x, z], [1.0, 0.0, 0.0]),
            3 => (|[x, y, z]| [x, -z, y], [0.0, 0.0, -1.0]),
            4 => (|[x, y, z]| [x, z, -y], [0.0, 0.0, 1.0]),
            _ => (|point| point, [1.0, 0.0, 0.0]),
        };
        let light = self.skylight.channels_at(x as i32, y as i32, z as i32);
        let shading = CornerShading {
            light: [[light; 4]; 4],
            ao: [0; 4],
            shade: true,
        };
        let centre = [0.5; 3];
        let place = |point: [f32; 3]| {
            let local = [
                point[0] - centre[0],
                point[1] - centre[1],
                point[2] - centre[2],
            ];
            let world = to_world(local);
            [
                world[0] + centre[0],
                world[1] + centre[1],
                world[2] + centre[2],
            ]
        };
        // The base never moves.
        let base = BlockFaceGeometry::from_bounds([0.25, 0.0, 0.25, 0.75, 0.1875, 0.75]);
        for (face_index, face) in FACES.iter().enumerate() {
            let corners = base.face(face_index).corners.map(place);
            mesh.push_block_quad(
                origin,
                to_world(face.normal),
                corners,
                box_texels((0, 1), face_index, base.face(face_index).corners, false),
                [1.0; 3],
                shading,
            );
        }
        // The stick pivots on top of the base.
        let tilt = if on { 0.6_f32 } else { -0.6 };
        let (sin, cos) = tilt.sin_cos();
        let pivot = [0.5, 0.1875, 0.5];
        let lean_x = lean[0];
        let lean_z = lean[2];
        let lever = |point: [f32; 3]| {
            let rel = [
                point[0] - pivot[0],
                point[1] - pivot[1],
                point[2] - pivot[2],
            ];
            // Tip the up axis toward `lean` by `tilt`.
            let along = rel[0] * lean_x + rel[2] * lean_z;
            let across = [rel[0] - along * lean_x, rel[2] - along * lean_z];
            let tipped_along = along * cos + rel[1] * sin;
            let tipped_up = -along * sin + rel[1] * cos;
            place([
                pivot[0] + across[0] + tipped_along * lean_x,
                pivot[1] + tipped_up,
                pivot[2] + across[1] + tipped_along * lean_z,
            ])
        };
        let stick =
            BlockFaceGeometry::from_bounds([0.4375, 0.1875, 0.4375, 0.5625, 0.8125, 0.5625]);
        for (face_index, face) in FACES.iter().enumerate() {
            let local = stick.face(face_index).corners;
            let corners = local.map(lever);
            // Rotate the face normal with the stick.
            let normal = {
                let n = face.normal;
                let along = n[0] * lean_x + n[2] * lean_z;
                let across = [n[0] - along * lean_x, n[2] - along * lean_z];
                let tipped_along = along * cos + n[1] * sin;
                let tipped_up = -along * sin + n[1] * cos;
                to_world([
                    across[0] + tipped_along * lean_x,
                    tipped_up,
                    across[1] + tipped_along * lean_z,
                ])
            };
            mesh.push_block_quad(
                origin,
                normal,
                corners,
                box_texels((0, 6), face_index, local, false),
                [1.0; 3],
                shading,
            );
        }
    }
}

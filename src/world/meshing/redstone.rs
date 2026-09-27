//! The two-layer dust traces and wall climbs from Beta's renderBlockRedstoneWire.

use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::world::block_ticks::behavior;
use crate::world::textures::block_tile;

use super::AtlasTexel;
use super::BlockFaceGeometry;
use super::BlockGeometry;
use super::CornerShading;
use super::FACE_BOTTOM;
use super::FACE_TOP;
use super::FACES;
use super::Mesher;
use super::face_corner_ao;
use super::face_corner_light;
use super::face_texels;
use super::torch_texels;

const WIRE_Y: f32 = 1.0 / 64.0;
const HIGHLIGHT_OFFSET: f32 = 1.0 / 128.0;

impl Mesher<'_> {
    fn wire_connects(&self, x: i32, y: i32, z: i32, side: i8) -> bool {
        let (block, metadata) = self.neighbors.cell(self.chunk, x, y, z);
        if block == Id::RedstoneWire {
            return true;
        }
        if side < 0 {
            return false;
        }
        if matches!(block, Id::Repeater | Id::PoweredRepeater) {
            return metadata & 3 == [2, 3, 0, 1][side as usize % 4];
        }
        behavior(block).can_provide_power()
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
        block: Id,
    ) {
        let metadata = self.chunk.metadata(x, y, z);
        let facing = metadata & 3;
        let delay = (metadata >> 2) & 3;
        let plate = BlockFaceGeometry::from_bounds([0.0, 0.0, 0.0, 1.0, 2.0 / 16.0, 1.0]);
        for (face_index, face) in FACES.iter().enumerate() {
            // BlockRedstoneRepeater.shouldSideBeRendered hides both the
            // underside and standard top; RenderBlocks draws the top rotated.
            if face_index == FACE_BOTTOM {
                continue;
            }
            let geometry = plate.face(face_index);
            let (tile_x, tile_y) = block_tile(block, face_index, false);
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

        // RenderBlocks.renderBlockRepeater puts a fixed torch at +/-5/16
        // and slides the other in 2/16 increments as the delay increases.
        let movable = [-1.0, 1.0, 3.0, 5.0][delay as usize] / 16.0;
        let (fixed, adjustable) = match facing {
            0 => ([0.0, -5.0 / 16.0], [0.0, movable]),
            1 => ([5.0 / 16.0, 0.0], [-movable, 0.0]),
            2 => ([0.0, 5.0 / 16.0], [0.0, -movable]),
            _ => ([-5.0 / 16.0, 0.0], [movable, 0.0]),
        };
        let tile = block_tile(block, FACE_BOTTOM, false);
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
        let normal_cube = |dx, dy, dz| is_opaque_cube(cell(dx, dy, dz));
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
        let power = f32::from(self.chunk.metadata(x as usize, y as usize, z as usize)) / 15.0;
        let red = if power == 0.0 { 0.3 } else { power * 0.6 + 0.4 };
        let green = (power * power * 0.7 - 0.5).max(0.0);
        let blue = (power * power * 0.6 - 0.7).max(0.0);
        let linear = bevy::prelude::Color::srgb(red, green, blue).to_linear();
        let tint = [linear.red, linear.green, linear.blue];
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
            if !normal_cube(dx, 0, dz) || cell(dx, 1, dz) != Id::RedstoneWire {
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
}

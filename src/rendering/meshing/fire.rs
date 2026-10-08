//! Fire: `RenderBlocks.renderBlockFire`. Fire standing on something is eight
//! leaning sheets; fire with nothing under it clings to each flammable
//! neighbor instead.

use crate::block::properties::can_catch_fire;
use crate::rendering::textures::FIRE_TILE;
use crate::rendering::textures::FIRE_TILE_ALT;

use super::AtlasTexel;
use super::BlockGeometry;
use super::CornerShading;
use super::Mesher;

const HEIGHT: f32 = 1.4;
/// How far a clinging sheet leans out from its wall, and a hanging one droops.
const LEAN: f32 = 0.2;
const WALL_RAISE: f32 = 1.0 / 16.0;

/// One corner: a position in the cell, then which end of the tile it takes
/// (`u` 0 is Beta's `var10`, `v` 0 the top).
type Corner = ([f32; 3], [u8; 2]);

struct Flames<'a> {
    mesh: &'a mut BlockGeometry,
    origin: [f32; 3],
    shading: CornerShading,
    mirror: bool,
}

impl Flames<'_> {
    /// A sheet in Beta's vertex order, which faces the same way here.
    fn sheet(&mut self, tile: (u8, u8), corners: [Corner; 4]) {
        let positions = corners.map(|(position, _)| position);
        let texels = corners.map(|(_, [u, v])| {
            let u = if self.mirror { 1 - u } else { u };
            AtlasTexel::new(tile.0, tile.1, u * 16, v * 16)
        });
        let edge =
            |from: [f32; 3], to: [f32; 3]| std::array::from_fn::<f32, 3, _>(|i| to[i] - from[i]);
        let (a, b) = (
            edge(positions[0], positions[1]),
            edge(positions[1], positions[2]),
        );
        let cross = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = cross.iter().map(|v| v * v).sum::<f32>().sqrt();
        self.mesh.push_block_quad(
            self.origin,
            cross.map(|v| v / length),
            positions,
            texels,
            [1.0; 3],
            self.shading,
        );
    }

    /// A sheet and its back, each corner keeping its texel.
    fn two_sided(&mut self, tile: (u8, u8), corners: [Corner; 4]) {
        self.sheet(tile, corners);
        self.sheet(tile, [corners[3], corners[2], corners[1], corners[0]]);
    }
}

impl Mesher<'_> {
    pub(super) fn push_fire(
        &self,
        mesh: &mut BlockGeometry,
        origin: [f32; 3],
        x: usize,
        y: usize,
        z: usize,
    ) {
        let (lx, ly, lz) = (x as i32, y as i32, z as i32);
        let block = |dx: i32, dy: i32, dz: i32| {
            self.neighbors.cell(self.chunk, lx + dx, ly + dy, lz + dz).0
        };
        let burns = |dx: i32, dy: i32, dz: i32| can_catch_fire(block(dx, dy, dz));
        // Fire takes its own cell's light, which it fills.
        let light = self.skylight.channels_at(lx, ly, lz);
        let mut flames = Flames {
            mesh,
            origin,
            shading: CornerShading {
                light: [[light; 4]; 4],
                ao: [0; 4],
                shade: false,
            },
            mirror: false,
        };
        let h = HEIGHT;

        let below = block(0, -1, 0);
        if below.is_normal_cube() || can_catch_fire(below) {
            let (a, b) = (FIRE_TILE, FIRE_TILE_ALT);
            let sheets: [((u8, u8), [Corner; 4]); 8] = [
                (
                    a,
                    [
                        ([0.2, h, 1.0], [1, 0]),
                        ([0.7, 0.0, 1.0], [1, 1]),
                        ([0.7, 0.0, 0.0], [0, 1]),
                        ([0.2, h, 0.0], [0, 0]),
                    ],
                ),
                (
                    a,
                    [
                        ([0.8, h, 0.0], [1, 0]),
                        ([0.3, 0.0, 0.0], [1, 1]),
                        ([0.3, 0.0, 1.0], [0, 1]),
                        ([0.8, h, 1.0], [0, 0]),
                    ],
                ),
                (
                    b,
                    [
                        ([1.0, h, 0.8], [1, 0]),
                        ([1.0, 0.0, 0.3], [1, 1]),
                        ([0.0, 0.0, 0.3], [0, 1]),
                        ([0.0, h, 0.8], [0, 0]),
                    ],
                ),
                (
                    b,
                    [
                        ([0.0, h, 0.2], [1, 0]),
                        ([0.0, 0.0, 0.7], [1, 1]),
                        ([1.0, 0.0, 0.7], [0, 1]),
                        ([1.0, h, 0.2], [0, 0]),
                    ],
                ),
                (
                    b,
                    [
                        ([0.1, h, 0.0], [0, 0]),
                        ([0.0, 0.0, 0.0], [0, 1]),
                        ([0.0, 0.0, 1.0], [1, 1]),
                        ([0.1, h, 1.0], [1, 0]),
                    ],
                ),
                (
                    b,
                    [
                        ([0.9, h, 1.0], [0, 0]),
                        ([1.0, 0.0, 1.0], [0, 1]),
                        ([1.0, 0.0, 0.0], [1, 1]),
                        ([0.9, h, 0.0], [1, 0]),
                    ],
                ),
                (
                    a,
                    [
                        ([0.0, h, 0.9], [0, 0]),
                        ([0.0, 0.0, 1.0], [0, 1]),
                        ([1.0, 0.0, 1.0], [1, 1]),
                        ([1.0, h, 0.9], [1, 0]),
                    ],
                ),
                (
                    a,
                    [
                        ([1.0, h, 0.1], [0, 0]),
                        ([1.0, 0.0, 0.0], [0, 1]),
                        ([0.0, 0.0, 0.0], [1, 1]),
                        ([0.0, h, 0.1], [1, 0]),
                    ],
                ),
            ];
            for (tile, corners) in sheets {
                flames.sheet(tile, corners);
            }
            return;
        }

        // World coordinates pick the tile and its mirroring, with Java's
        // truncating division.
        let (wx, wz) = (self.origin_x + lx, self.origin_z + lz);
        let tile = if (wx + ly + wz) & 1 == 1 {
            FIRE_TILE_ALT
        } else {
            FIRE_TILE
        };
        flames.mirror = (wx / 2 + ly / 2 + wz / 2) & 1 == 1;
        let (low, high) = (WALL_RAISE, HEIGHT + WALL_RAISE);
        if burns(-1, 0, 0) {
            flames.two_sided(
                tile,
                [
                    ([LEAN, high, 1.0], [1, 0]),
                    ([0.0, low, 1.0], [1, 1]),
                    ([0.0, low, 0.0], [0, 1]),
                    ([LEAN, high, 0.0], [0, 0]),
                ],
            );
        }
        if burns(1, 0, 0) {
            flames.two_sided(
                tile,
                [
                    ([1.0 - LEAN, high, 0.0], [0, 0]),
                    ([1.0, low, 0.0], [0, 1]),
                    ([1.0, low, 1.0], [1, 1]),
                    ([1.0 - LEAN, high, 1.0], [1, 0]),
                ],
            );
        }
        if burns(0, 0, -1) {
            flames.two_sided(
                tile,
                [
                    ([0.0, high, LEAN], [1, 0]),
                    ([0.0, low, 0.0], [1, 1]),
                    ([1.0, low, 0.0], [0, 1]),
                    ([1.0, high, LEAN], [0, 0]),
                ],
            );
        }
        if burns(0, 0, 1) {
            flames.two_sided(
                tile,
                [
                    ([1.0, high, 1.0 - LEAN], [0, 0]),
                    ([1.0, low, 1.0], [0, 1]),
                    ([0.0, low, 1.0], [1, 1]),
                    ([0.0, high, 1.0 - LEAN], [1, 0]),
                ],
            );
        }
        if burns(0, 1, 0) {
            // Two sheets hanging from the block above, one per tile.
            flames.mirror = false;
            let droop = 1.0 - LEAN;
            if (wx + ly + 1 + wz) & 1 == 0 {
                flames.sheet(
                    FIRE_TILE,
                    [
                        ([0.0, droop, 0.0], [1, 0]),
                        ([1.0, 1.0, 0.0], [1, 1]),
                        ([1.0, 1.0, 1.0], [0, 1]),
                        ([0.0, droop, 1.0], [0, 0]),
                    ],
                );
                flames.sheet(
                    FIRE_TILE_ALT,
                    [
                        ([1.0, droop, 1.0], [1, 0]),
                        ([0.0, 1.0, 1.0], [1, 1]),
                        ([0.0, 1.0, 0.0], [0, 1]),
                        ([1.0, droop, 0.0], [0, 0]),
                    ],
                );
            } else {
                flames.sheet(
                    FIRE_TILE,
                    [
                        ([0.0, droop, 1.0], [1, 0]),
                        ([0.0, 1.0, 0.0], [1, 1]),
                        ([1.0, 1.0, 0.0], [0, 1]),
                        ([1.0, droop, 1.0], [0, 0]),
                    ],
                );
                flames.sheet(
                    FIRE_TILE_ALT,
                    [
                        ([1.0, droop, 0.0], [1, 0]),
                        ([1.0, 1.0, 1.0], [1, 1]),
                        ([0.0, 1.0, 1.0], [0, 1]),
                        ([0.0, droop, 0.0], [0, 0]),
                    ],
                );
            }
        }
    }
}

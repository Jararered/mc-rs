//! What one block face needs: whether a neighbor hides it, its corner
//! light and ambient occlusion, its atlas texels, and its tint.

use super::ChunkNeighbors;
use super::FACES;
use super::Face;
use super::WATER_TINT;
use super::geometry::BlockFaceGeometry;
use super::geometry::FACE_BOTTOM;
use super::geometry::FACE_EAST;
use super::geometry::FACE_NORTH;
use super::geometry::FACE_SOUTH;
use super::geometry::FACE_TOP;
use super::geometry::FACE_WEST;
use super::geometry::FaceGeometry;
use super::vertex::AtlasTexel;
use super::vertex::BRIGHT_TINT;
use crate::block::blocks::Block;
use crate::block::blocks::species;
use crate::block::fluids::Fluid;
use crate::world::chunk::Chunk;
use crate::world::lighting::Skylight;
use bevy::prelude::Color;

pub(super) fn tangent_axes(face: &Face) -> [usize; 2] {
    match face.neighbor {
        [0, _, 0] => [0, 2],
        [_, 0, 0] => [1, 2],
        _ => [0, 1],
    }
}

/// The four light samples of each corner, as Beta's smooth lighting reads
/// them: the face neighbor, the two neighbors beside it toward the corner,
/// and the diagonal. The first sample alone is the flat-lighting value.
pub(super) fn face_corner_light(
    skylight: &Skylight,
    x: usize,
    y: usize,
    z: usize,
    face: &Face,
    geometry: FaceGeometry,
) -> [[u8; 4]; 4] {
    let tangent_axes = tangent_axes(face);
    std::array::from_fn(|corner_index| {
        let corner = geometry.corners[corner_index];
        let directions = [
            if corner[tangent_axes[0]] < 0.5 { -1 } else { 1 },
            if corner[tangent_axes[1]] < 0.5 { -1 } else { 1 },
        ];
        let samples = [[0, 0], [directions[0], 0], [0, directions[1]], directions];
        samples.map(|sample| {
            let mut offset = face.neighbor;
            offset[tangent_axes[0]] += sample[0];
            offset[tangent_axes[1]] += sample[1];
            skylight.channels_at(
                x as i32 + offset[0],
                y as i32 + offset[1],
                z as i32 + offset[2],
            )
        })
    })
}

/// Per-corner ambient occlusion levels matching the original voxel renderer.
/// A corner is darkened by its two face-adjacent blocks and its diagonal
/// block; when both side blocks are present the diagonal is treated as
/// occluded too.
pub(super) fn face_corner_ao(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
    face: &Face,
    geometry: FaceGeometry,
) -> [u8; 4] {
    let tangent_axes = tangent_axes(face);
    std::array::from_fn(|corner_index| {
        let corner = geometry.corners[corner_index];
        let tangent_direction = [
            if corner[tangent_axes[0]] < 0.5 { -1 } else { 1 },
            if corner[tangent_axes[1]] < 0.5 { -1 } else { 1 },
        ];
        let mut side_a = face.neighbor;
        let mut side_b = face.neighbor;
        let mut diagonal = face.neighbor;
        side_a[tangent_axes[0]] += tangent_direction[0];
        side_b[tangent_axes[1]] += tangent_direction[1];
        diagonal[tangent_axes[0]] += tangent_direction[0];
        diagonal[tangent_axes[1]] += tangent_direction[1];

        let side_a = opaque_at(chunk, neighbors, x, y, z, side_a);
        let side_b = opaque_at(chunk, neighbors, x, y, z, side_b);
        let diagonal = opaque_at(chunk, neighbors, x, y, z, diagonal);
        if side_a && side_b {
            3
        } else {
            u8::from(side_a) + u8::from(side_b) + u8::from(diagonal)
        }
    })
}

pub(super) fn opaque_at(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
    offset: [i32; 3],
) -> bool {
    let position = [
        x as i32 + offset[0],
        y as i32 + offset[1],
        z as i32 + offset[2],
    ];
    neighbors
        .get(chunk, position[0], position[1], position[2])
        .is_some_and(Block::is_opaque_cube)
}

/// Return the sole, reciprocal chest neighbor for a valid pair.
pub(super) fn chest_pair_direction(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
) -> Option<[i32; 3]> {
    let position = [x as i32, y as i32, z as i32];
    let directions = [[-1, 0, 0], [1, 0, 0], [0, 0, -1], [0, 0, 1]];
    let adjacent = directions
        .into_iter()
        .filter(|direction| {
            neighbors
                .get(
                    chunk,
                    position[0] + direction[0],
                    position[1],
                    position[2] + direction[2],
                )
                .is_some_and(Block::is_chest)
        })
        .collect::<Vec<_>>();
    let [direction] = adjacent.as_slice() else {
        return None;
    };
    let partner = [
        position[0] + direction[0],
        position[1],
        position[2] + direction[2],
    ];
    let reciprocal_neighbors = directions
        .into_iter()
        .filter(|other| {
            let neighbor = [partner[0] + other[0], partner[1], partner[2] + other[2]];
            neighbor != position
                && neighbors
                    .get(chunk, neighbor[0], neighbor[1], neighbor[2])
                    .is_some_and(Block::is_chest)
        })
        .count();
    (reciprocal_neighbors == 0).then_some(*direction)
}

pub(super) fn chest_geometry(pair_direction: Option<[i32; 3]>) -> BlockFaceGeometry {
    let inset = 1.0 / 16.0;
    let mut bounds = [inset, 0.0, inset, 1.0 - inset, 14.0 / 16.0, 1.0 - inset];
    match pair_direction {
        Some([-1, 0, 0]) => bounds[0] = 0.0,
        Some([1, 0, 0]) => bounds[3] = 1.0,
        Some([0, 0, -1]) => bounds[2] = 0.0,
        Some([0, 0, 1]) => bounds[5] = 1.0,
        _ => {}
    }
    BlockFaceGeometry::from_bounds(bounds)
}

pub(super) fn double_chest_tile(
    block: Block,
    metadata: u8,
    pair_direction: [i32; 3],
    face: usize,
) -> (u8, u8) {
    if face == FACE_TOP || face == FACE_BOTTOM {
        return (9, 1);
    }
    let facing = block.facing(metadata).unwrap_or_default();
    let front = facing.face_index();
    let back = match front {
        FACE_EAST => FACE_WEST,
        FACE_WEST => FACE_EAST,
        FACE_SOUTH => FACE_NORTH,
        _ => FACE_SOUTH,
    };
    if face != front && face != back {
        return (10, 1);
    }

    // `BlockChest.getBlockTexture`: the atlas stores each long face as two
    // adjacent tiles, left then right as seen from outside. Seen from the
    // south or west, the half toward -x or -z is on the left; from the north
    // or east it is on the right.
    let partner_ahead = pair_direction[0] > 0 || pair_direction[2] > 0;
    let behind_is_left = face == FACE_SOUTH || face == FACE_WEST;
    let left_half = partner_ahead == behind_is_left;
    let tile_x = if left_half { 9 } else { 10 };
    let tile_y = if face == front { 2 } else { 3 };
    (tile_x, tile_y)
}

/// Same visibility as [`Mesher::push_fluid`]: either id of this fluid and ice
/// cover a face, and an opaque cube covers the sides and bottom. An open top
/// stays visible, which is the flat water surface in a filtered wireframe.
pub(super) fn fluid_shell_face_visible(fluid: Fluid, neighbor: Option<Block>, dy: i32) -> bool {
    match neighbor {
        None => true,
        Some(neighbor) => {
            Fluid::of(neighbor) != Some(fluid)
                && neighbor != Block::Ice
                && (dy > 0 || !neighbor.is_opaque_cube())
        }
    }
}

/// `BlockGrass.getBlockTexture` reads the block above and swaps a grass side
/// for the snow-capped tile when that material is `Material.snow` (the snow
/// layer) or `Material.builtSnow` (the snow block). The block is always in the
/// same column as its cover, so this never reaches into a neighbour chunk.
pub(super) fn snow_above(
    chunk: &Chunk,
    neighbors: &ChunkNeighbors<'_>,
    x: usize,
    y: usize,
    z: usize,
) -> bool {
    matches!(
        neighbors.get(chunk, x as i32, y as i32 + 1, z as i32),
        Some(Block::SnowLayer | Block::Snow)
    )
}

/// Fast leaves hide every non-air neighbour, like any solid cube. Fancy leaves
/// are cutout, so leaf-to-leaf faces stay visible and solid faces towards a
/// canopy are not covered. Water never hides a neighbour, so lake beds and
/// walls stay visible under the surface plane.
pub(super) fn neighbor_hides_face(
    block: Block,
    neighbor: Option<Block>,
    fancy_graphics: bool,
) -> bool {
    let Some(neighbor) = neighbor else {
        return false;
    };
    if neighbor == Block::Air
        || neighbor == Block::SnowLayer
        || neighbor == Block::Cactus
        || neighbor == Block::Farmland
        || neighbor == Block::Crops
        || neighbor == Block::Water
        || neighbor == Block::FlowingWater
        || neighbor == Block::Lava
        || neighbor == Block::FlowingLava
        || neighbor == Block::MobSpawner
        || neighbor == Block::NetherPortal
        || neighbor == Block::Bed
        || neighbor.is_chest()
        || neighbor.is_ladder()
        || neighbor.is_torch()
        || partial_redstone_block(neighbor)
        || neighbor.is_crossed_plant()
        || neighbor == Block::Cobweb
        || neighbor == Block::Fence
        || neighbor == Block::Trapdoor
        || neighbor.is_stairs()
        || neighbor.is_door()
    {
        return false;
    }
    // `BlockStep.shouldSideBeRendered`: slabs of equal height share a side.
    if neighbor == Block::StoneSlab {
        return block == Block::StoneSlab;
    }
    if neighbor == Block::Glass && block != Block::Glass {
        return false;
    }
    if fancy_graphics && block.is_leaves() && neighbor.is_leaves() {
        return false;
    }
    if fancy_graphics && neighbor.is_leaves() && !block.is_leaves() {
        return false;
    }
    true
}

/// Redstone parts that fill only part of their cell, so they never hide a
/// neighbor's face. An extended piston base also leaves a strip along its
/// sides, which [`neighbor_hides_face_at`] checks with metadata.
pub(super) fn partial_redstone_block(block: Block) -> bool {
    matches!(
        block,
        Block::RedstoneWire
            | Block::Repeater
            | Block::PoweredRepeater
            | Block::RedstoneTorch
            | Block::UnlitRedstoneTorch
            | Block::Lever
            | Block::StoneButton
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::Rail
            | Block::PoweredRail
            | Block::DetectorRail
            | Block::PistonHead
            | Block::MovingPiston
    )
}

/// [`neighbor_hides_face`] for the cell at `(x, y, z)` relative to `chunk`,
/// seen through `face` of `block`. It also reads the neighbor's metadata:
/// the recessed quarter of an extended piston base leaves a strip along every
/// side, so only its back covers a whole neighboring face.
pub(super) fn neighbor_hides_face_at(
    block: Block,
    neighbors: &ChunkNeighbors<'_>,
    chunk: &Chunk,
    (x, y, z): (i32, i32, i32),
    face: usize,
    fancy_graphics: bool,
) -> bool {
    let neighbor = neighbors.get(chunk, x, y, z);
    if matches!(neighbor, Some(Block::Piston | Block::StickyPiston)) {
        let metadata = neighbors.cell(chunk, x, y, z).1;
        if metadata & 8 != 0
            && let Some((min, max)) =
                crate::block::definition::redstone_bounds(neighbor.unwrap_or(Block::Air), metadata)
        {
            let covers_plane = match face {
                FACE_TOP => min[1] == 0.0,
                FACE_BOTTOM => max[1] == 1.0,
                FACE_EAST => min[0] == 0.0,
                FACE_WEST => max[0] == 1.0,
                FACE_SOUTH => min[2] == 0.0,
                _ => max[2] == 1.0,
            };
            let covers_face = (0..3)
                .filter(|&axis| FACES[face].neighbor[axis] == 0)
                .all(|axis| min[axis] == 0.0 && max[axis] == 1.0);
            if !(covers_plane && covers_face) {
                return false;
            }
        }
    }
    neighbor_hides_face(block, neighbor, fancy_graphics)
}

/// Tile corners in the winding each face's geometry uses.
pub(super) fn face_texels(tile_x: u8, tile_y: u8, face: usize) -> [AtlasTexel; 4] {
    let corners = match face {
        0 | 1 => [[0, 0], [0, 16], [16, 16], [16, 0]],
        2 | 5 => [[0, 16], [0, 0], [16, 0], [16, 16]],
        _ => [[0, 16], [16, 16], [16, 0], [0, 0]],
    };
    tile_texels(tile_x, tile_y, corners)
}

/// Texels for a face of a box inside the cell: the tile is cropped to the
/// face's position, as Beta maps a block with custom bounds. `flip` mirrors
/// the tile, for `RenderBlocks.flipTexture`.
pub(super) fn box_texels(
    tile: (u8, u8),
    face: usize,
    corners: [[f32; 3]; 4],
    flip: bool,
) -> [AtlasTexel; 4] {
    corners.map(|[x, y, z]| {
        let (u, v) = match face {
            FACE_TOP => (x, z),
            FACE_BOTTOM => (z, x),
            // `renderSouthFace` and `renderEastFace` run u against the
            // axis, so a tile reads the same way round from outside on
            // every side.
            FACE_EAST => (1.0 - z, 1.0 - y),
            FACE_WEST => (z, 1.0 - y),
            FACE_NORTH => (1.0 - x, 1.0 - y),
            _ => (x, 1.0 - y),
        };
        let u = if flip { 1.0 - u } else { u };
        AtlasTexel::new(
            tile.0,
            tile.1,
            (u * 16.0).round() as u8,
            (v * 16.0).round() as u8,
        )
    })
}

/// Box shapes whose tiles have transparent texels.
pub(super) fn box_shape_masked(block: Block) -> bool {
    block == Block::Trapdoor || block.is_door()
}

/// Sample just the visible shaft and tip of a torch atlas tile.
pub(super) fn torch_texels(tile: (u8, u8), face_index: usize) -> [AtlasTexel; 4] {
    if face_index == FACE_TOP {
        return [
            AtlasTexel::new(tile.0, tile.1, 7, 6),
            AtlasTexel::new(tile.0, tile.1, 7, 8),
            AtlasTexel::new(tile.0, tile.1, 9, 8),
            AtlasTexel::new(tile.0, tile.1, 9, 6),
        ];
    }
    // Only columns 7..8 and rows 6..15 contain the torch. Sampling the whole
    // transparent tile shrinks the shaft to two pixels on a face that is
    // already physically narrow.
    face_texels(tile.0, tile.1, face_index).map(|texel| {
        let [u, v] = texel.texel;
        AtlasTexel::new(tile.0, tile.1, 7 + u / 16 * 2, 6 + v / 16 * 10)
    })
}

pub(super) fn tile_texels(tile_x: u8, tile_y: u8, corners: [[u8; 2]; 4]) -> [AtlasTexel; 4] {
    corners.map(|[u, v]| AtlasTexel::new(tile_x, tile_y, u, v))
}

pub(super) fn block_tint(block: Block, metadata: u8, foliage: Option<[f32; 3]>) -> [f32; 3] {
    if block == Block::LitFurnace {
        // The active face is a small flame, but Beta's lit furnace body also
        // appears subtly brighter than the idle block.
        return [BRIGHT_TINT; 3];
    }
    match block {
        Block::Water | Block::FlowingWater => WATER_TINT,
        Block::Leaves => match metadata & 3 {
            species::BIRCH => linear_rgb(128, 167, 85),
            species::SPRUCE => linear_rgb(97, 153, 97),
            _ => foliage.unwrap_or([0.28, 0.71, 0.09]),
        },
        // Beta 1.7.3 only has the oak plank tile. The other species use that
        // tile with a color multiplier, matching the game's tint path.
        Block::WoodenPlanks => match metadata & 3 {
            species::SPRUCE => linear_rgb(214, 177, 131),
            species::BIRCH => linear_rgb(255, 246, 218),
            _ => [1.0, 1.0, 1.0],
        },
        _ => [1.0, 1.0, 1.0],
    }
}

pub(super) fn linear_rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let color = Color::srgb_u8(r, g, b).to_linear();
    [color.red, color.green, color.blue]
}

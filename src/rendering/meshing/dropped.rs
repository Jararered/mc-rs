//! The mesh of one block on its own, for dropped and falling blocks.

use super::CornerShading;
use super::FACES;
use super::faces::block_tint;
use super::faces::box_shape_masked;
use super::faces::box_texels;
use super::faces::face_texels;
use super::geometry::BlockFaceGeometry;
use super::geometry::FACE_BOTTOM;
use super::geometry::FACE_TOP;
use super::geometry::block_boxes;
use super::geometry::is_box_shape;
use super::vertex::BlockGeometry;
use super::vertex::face_shade;
use crate::block::blocks::Block;
use crate::rendering::textures::block_tile;

/// A dropped block: the world cube, centered on the origin, with the same
/// face tiles, shade, and tints the chunk mesher uses. Fancy grass also
/// gets the side overlay. Fancy leaves are marked cutout.
pub struct DroppedBlockMeshes {
    pub body: BlockGeometry,
    pub overlay: Option<BlockGeometry>,
    pub cutout: bool,
    pub alpha_masked: bool,
}

pub fn dropped_block_meshes(
    block: Block,
    metadata: u8,
    fancy_graphics: bool,
    grass_tint: [f32; 3],
    foliage_tint: [f32; 3],
) -> DroppedBlockMeshes {
    let mut body = BlockGeometry::default();
    let mut overlay = BlockGeometry::default();
    let centered = [-0.5; 3];
    if is_box_shape(block) {
        for bounds in block_boxes(block, metadata, [false; 4]).as_slice() {
            let geometry = BlockFaceGeometry::from_bounds(*bounds);
            for (face_index, face) in FACES.iter().enumerate() {
                let corners = geometry.face(face_index).corners;
                let tile = block_tile(block, metadata, face_index, fancy_graphics);
                let shade = face_shade(face.normal);
                body.push_block_quad(
                    centered,
                    face.normal,
                    corners,
                    box_texels(tile, face_index, corners, false),
                    [shade; 3],
                    CornerShading::FULL_BRIGHT,
                );
            }
        }
        return DroppedBlockMeshes {
            body,
            overlay: None,
            cutout: false,
            alpha_masked: box_shape_masked(block),
        };
    }
    let block_geometry = BlockFaceGeometry::for_block(block);
    for (face_index, face) in FACES.iter().enumerate() {
        let face_geometry = block_geometry.face(face_index);
        let grass_side =
            block == Block::Grass && face_index != FACE_TOP && face_index != FACE_BOTTOM;
        let tint = if block == Block::Grass && face_index == FACE_TOP {
            grass_tint
        } else {
            block_tint(block, metadata, Some(foliage_tint))
        };
        // Items carry their face shade in the tint and ignore world light, so
        // they look the same under every lighting mode and time of day.
        let shade = face_shade(face.normal);
        body.push_block_quad(
            centered,
            face.normal,
            face_geometry.corners,
            {
                let (tile_x, tile_y) = block_tile(block, metadata, face_index, fancy_graphics);
                face_texels(tile_x, tile_y, face_index)
            },
            tint.map(|channel| channel * shade),
            CornerShading::FULL_BRIGHT,
        );
        if grass_side && fancy_graphics {
            overlay.push_grass_overlay(
                centered,
                face,
                face_index,
                grass_tint.map(|channel| channel * shade),
                CornerShading::FULL_BRIGHT,
            );
        }
    }
    DroppedBlockMeshes {
        body,
        overlay: (!overlay.is_empty()).then_some(overlay),
        cutout: fancy_graphics && block.is_leaves(),
        alpha_masked: matches!(block, Block::Cactus | Block::Glass | Block::MobSpawner),
    }
}

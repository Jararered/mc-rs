//! The mesh of a block a piston is moving (`TileEntityRendererPiston`).

use super::ChunkMeshes;
use super::ChunkNeighbors;
use super::mesher::Mesher;
use crate::block::blocks::Block;
use crate::world::chunk::Chunk;
use crate::world::lighting::Skylight;

/// Where the block sits in its scratch chunk. Vertices come out relative to
/// `(0, MOVING_CELL[1], 0)`, so subtract this from them to center the
/// block's cell on the origin.
pub const MOVING_CELL: [usize; 3] = [8, 64, 8];

/// One block alone in an empty chunk, meshed by the chunk mesher: every face
/// is drawn, as `renderAllFaces` does, and lit by one `(sky, block)` light.
pub fn moving_block_meshes(
    block: Block,
    metadata: u8,
    light: (u8, u8),
    fancy_graphics: bool,
    short_arm: bool,
) -> ChunkMeshes {
    let [x, y, z] = MOVING_CELL;
    let mut chunk = Chunk::new();
    chunk.set_with_metadata(x, y, z, block, metadata);
    let skylight = Skylight::uniform(light.0, light.1);
    let neighbors = ChunkNeighbors::default();
    let mut mesher = Mesher::new(
        &chunk,
        &neighbors,
        &skylight,
        None,
        fancy_graphics,
        0,
        0,
        None,
    );
    mesher.short_arm = short_arm;
    mesher.region(y..y + 1, y)
}

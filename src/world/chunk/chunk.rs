use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::world::block::block::BlockId;
use crate::world::generation::GeneratedChunk;

use super::ChunkPos;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 128;

#[derive(Clone)]
pub struct Chunk {
    blocks: Box<[BlockId]>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: vec![BlockId::Air; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE].into_boxed_slice(),
        }
    }

    /// Rebuild a chunk from a flat block array, as produced by [`Self::blocks`].
    pub fn from_blocks(blocks: Vec<BlockId>) -> Self {
        assert_eq!(
            blocks.len(),
            CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE,
            "a chunk holds exactly one block per position"
        );
        Self {
            blocks: blocks.into_boxed_slice(),
        }
    }

    /// The flat block array in [`Self::index`] order.
    pub fn blocks(&self) -> &[BlockId] {
        &self.blocks
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<BlockId> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(self.blocks[Self::index(x, y, z)])
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, block: BlockId) {
        assert!(x < CHUNK_SIZE && y < CHUNK_HEIGHT && z < CHUNK_SIZE);
        self.blocks[Self::index(x, y, z)] = block;
    }

    pub(crate) const fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Resource, Default)]
pub struct WorldChunks {
    chunks: HashMap<ChunkPos, GeneratedChunk>,
}

impl WorldChunks {
    pub fn insert(&mut self, position: ChunkPos, chunk: GeneratedChunk) {
        self.chunks.insert(position, chunk);
    }

    pub fn get(&self, position: ChunkPos) -> Option<&GeneratedChunk> {
        self.chunks.get(&position)
    }

    pub fn get_mut(&mut self, position: ChunkPos) -> Option<&mut GeneratedChunk> {
        self.chunks.get_mut(&position)
    }

    pub fn contains(&self, position: ChunkPos) -> bool {
        self.chunks.contains_key(&position)
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks.keys().copied()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn remove(&mut self, position: ChunkPos) -> Option<GeneratedChunk> {
        self.chunks.remove(&position)
    }

    /// Block at a world-space integer position, if that chunk is loaded and `y`
    /// is inside the world height.
    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<BlockId> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let chunk = self.get(ChunkPos::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        chunk.chunk.get(local_x, y as usize, local_z)
    }

    /// Replace a loaded block and refresh that column's heightmap.
    ///
    /// Returns the previous block, or `None` when the cell is outside the world
    /// or its chunk is not loaded.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: BlockId) -> Option<BlockId> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let generated = self.get_mut(ChunkPos::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let previous = generated.chunk.get(local_x, y as usize, local_z)?;
        if previous != block {
            generated.chunk.set(local_x, y as usize, local_z, block);
            generated
                .heightmap
                .recompute_column(&generated.chunk, local_x, local_z);
        }
        Some(previous)
    }
}

/// Chunks whose meshes can change when the block at `(x, z)` is edited.
///
/// The edited chunk is always included. A neighbour is included when the block
/// sits on that shared face, because meshing currently treats a missing
/// neighbour as air.
pub fn remesh_chunks_touching(x: i32, z: i32) -> Vec<ChunkPos> {
    let position = ChunkPos::from_block(x, z);
    let local_x = x.rem_euclid(CHUNK_SIZE as i32);
    let local_z = z.rem_euclid(CHUNK_SIZE as i32);
    let mut positions = vec![position];
    if local_x == 0 {
        positions.push(ChunkPos {
            x: position.x - 1,
            z: position.z,
        });
    } else if local_x == CHUNK_SIZE as i32 - 1 {
        positions.push(ChunkPos {
            x: position.x + 1,
            z: position.z,
        });
    }
    if local_z == 0 {
        positions.push(ChunkPos {
            x: position.x,
            z: position.z - 1,
        });
    } else if local_z == CHUNK_SIZE as i32 - 1 {
        positions.push(ChunkPos {
            x: position.x,
            z: position.z + 1,
        });
    }
    positions
}

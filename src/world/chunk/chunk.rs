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

    pub fn positions(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.chunks.keys().copied()
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn remove(&mut self, position: ChunkPos) -> Option<GeneratedChunk> {
        self.chunks.remove(&position)
    }
}

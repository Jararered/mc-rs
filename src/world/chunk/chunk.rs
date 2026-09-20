use std::collections::HashMap;

use bevy::prelude::Resource;

use crate::world::{block::block::BlockId, generation::GeneratedChunk};

use super::ChunkPos;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 128;

pub struct Chunk {
    blocks: Box<[BlockId]>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: vec![BlockId::Air; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE].into_boxed_slice(),
        }
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

    pub fn contains(&self, position: ChunkPos) -> bool {
        self.chunks.contains_key(&position)
    }

    pub fn remove(&mut self, position: ChunkPos) -> Option<GeneratedChunk> {
        self.chunks.remove(&position)
    }
}

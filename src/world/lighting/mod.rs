use crate::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk},
};

/// Direct sunlight through air, computed from the top of each column.
pub struct Skylight {
    levels: Box<[u8]>,
}

impl Skylight {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        let mut levels = vec![0; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE].into_boxed_slice();

        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let mut sunlight = 15;
                for y in (0..CHUNK_HEIGHT).rev() {
                    if chunk.get(x, y, z) != Some(BlockId::Air) {
                        sunlight = 0;
                    }
                    levels[Chunk::index(x, y, z)] = sunlight;
                }
            }
        }

        Self { levels }
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<u8> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(self.levels[Chunk::index(x, y, z)])
    }
}

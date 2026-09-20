use crate::world::block::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;

/// First air/water position above solid terrain in each column.
pub struct Heightmap {
    heights: [u8; CHUNK_SIZE * CHUNK_SIZE],
}

impl Heightmap {
    pub fn from_chunk(chunk: &Chunk) -> Self {
        let mut heights = [0; CHUNK_SIZE * CHUNK_SIZE];
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let top = (0..CHUNK_HEIGHT)
                    .rev()
                    .find(|&y| !matches!(chunk.get(x, y, z), Some(BlockId::Air | BlockId::Water)))
                    .map_or(0, |y| y + 1);
                heights[x * CHUNK_SIZE + z] = top as u8;
            }
        }
        Self { heights }
    }

    /// Rebuild a heightmap from stored column heights, as produced by
    /// [`Self::heights`].
    pub fn from_heights(heights: [u8; CHUNK_SIZE * CHUNK_SIZE]) -> Self {
        Self { heights }
    }

    /// The raw column heights in `x * CHUNK_SIZE + z` order.
    pub fn heights(&self) -> &[u8; CHUNK_SIZE * CHUNK_SIZE] {
        &self.heights
    }

    pub fn get(&self, x: usize, z: usize) -> u8 {
        self.heights[x * CHUNK_SIZE + z]
    }

    pub fn max(&self) -> u8 {
        self.heights.iter().copied().max().unwrap_or(0)
    }
}

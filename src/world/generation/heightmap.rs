use crate::world::block::block::BlockId;
use crate::world::block::properties::is_crossed_plant;
use crate::world::block::properties::is_torch;
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
                    .find(|&y| chunk.get(x, y, z).is_some_and(occupies_column))
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

    /// Recompute one column after a block in that column changes.
    pub fn recompute_column(&mut self, chunk: &Chunk, x: usize, z: usize) {
        let top = (0..CHUNK_HEIGHT)
            .rev()
            .find(|&y| chunk.get(x, y, z).is_some_and(occupies_column))
            .map_or(0, |y| y + 1);
        self.heights[x * CHUNK_SIZE + z] = top as u8;
    }

    pub fn max(&self) -> u8 {
        self.heights.iter().copied().max().unwrap_or(0)
    }
}

/// Blocks that raise the ground surface. Plants and torches stand in the air
/// cell above that surface.
fn occupies_column(block: BlockId) -> bool {
    !matches!(block, BlockId::Air | BlockId::Water)
        && !block.is_ladder()
        && !is_torch(block)
        && !is_crossed_plant(block)
}

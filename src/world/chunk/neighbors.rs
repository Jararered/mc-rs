//! The eight chunks around one.

use super::CHUNK_HEIGHT;
use super::CHUNK_SIZE;
use super::Chunk;
use crate::block::blocks::Block;

/// Block data surrounding a chunk, used for lighting, face culling and
/// ambient occlusion at its edges. Streaming supplies all eight neighbors before
/// meshing; isolated mesh helpers may leave them absent.
#[derive(Clone, Copy, Default)]
pub struct ChunkNeighbors<'a> {
    pub w: Option<&'a Chunk>,
    pub e: Option<&'a Chunk>,
    pub n: Option<&'a Chunk>,
    pub s: Option<&'a Chunk>,
    pub nw: Option<&'a Chunk>,
    pub ne: Option<&'a Chunk>,
    pub sw: Option<&'a Chunk>,
    pub se: Option<&'a Chunk>,
}

impl ChunkNeighbors<'_> {
    /// The block at a chunk-local cell, reading into the neighbors.
    pub fn get(&self, center: &Chunk, x: i32, y: i32, z: i32) -> Option<Block> {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return None;
        }
        if (0..CHUNK_SIZE as i32).contains(&x) && (0..CHUNK_SIZE as i32).contains(&z) {
            return center.get(x as usize, y as usize, z as usize);
        }
        let region_x = if x < 0 {
            -1
        } else if x >= CHUNK_SIZE as i32 {
            1
        } else {
            0
        };
        let region_z = if z < 0 {
            -1
        } else if z >= CHUNK_SIZE as i32 {
            1
        } else {
            0
        };
        let chunk = match (region_x, region_z) {
            (0, 0) => Some(center),
            (-1, 0) => self.w,
            (1, 0) => self.e,
            (0, -1) => self.n,
            (0, 1) => self.s,
            (-1, -1) => self.nw,
            (1, -1) => self.ne,
            (-1, 1) => self.sw,
            (1, 1) => self.se,
            _ => None,
        }?;
        let local_x = (x - region_x * CHUNK_SIZE as i32) as usize;
        let local_z = (z - region_z * CHUNK_SIZE as i32) as usize;
        chunk.get(local_x, y as usize, local_z)
    }

    /// The block and metadata at a chunk-local cell, reading into the
    /// neighbors. Missing cells read as air.
    pub(crate) fn cell(&self, center: &Chunk, x: i32, y: i32, z: i32) -> (Block, u8) {
        let Some(block) = self.get(center, x, y, z) else {
            return (Block::Air, 0);
        };
        let region = |value: i32| value.div_euclid(CHUNK_SIZE as i32);
        let chunk = match (region(x), region(z)) {
            (0, 0) => Some(center),
            (-1, 0) => self.w,
            (1, 0) => self.e,
            (0, -1) => self.n,
            (0, 1) => self.s,
            (-1, -1) => self.nw,
            (1, -1) => self.ne,
            (-1, 1) => self.sw,
            (1, 1) => self.se,
            _ => None,
        };
        let metadata = chunk.map_or(0, |chunk| {
            chunk.metadata(
                x.rem_euclid(CHUNK_SIZE as i32) as usize,
                y as usize,
                z.rem_euclid(CHUNK_SIZE as i32) as usize,
            )
        });
        (block, metadata)
    }
}

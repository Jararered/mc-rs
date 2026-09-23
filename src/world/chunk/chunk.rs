use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::world::block::block::BlockId;
use crate::world::chest::Chest;
use crate::world::furnace::Furnace;
use crate::world::generation::Climate;
use crate::world::generation::GeneratedChunk;

use super::ChunkPos;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 128;

#[derive(Clone)]
pub struct Chunk {
    // Meshing snapshots the center chunk and eight neighbors. Sharing their
    // immutable blocks avoids copying nine full arrays for every mesh job;
    // edits detach only the modified chunk.
    blocks: Arc<[BlockId]>,
    /// Block-local inventories and simulation state. Keyed by flat block index.
    furnaces: HashMap<usize, Furnace>,
    chests: HashMap<usize, Chest>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: vec![BlockId::Air; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE].into(),
            furnaces: HashMap::new(),
            chests: HashMap::new(),
        }
    }

    /// Rebuild a chunk from a flat block array, as produced by [`Self::blocks`].
    pub fn from_blocks(blocks: Vec<BlockId>) -> Self {
        assert_eq!(
            blocks.len(),
            CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE,
            "a chunk holds exactly one block per position"
        );
        let furnaces = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| is_furnace(**block))
            .map(|(index, _)| (index, Furnace::default()))
            .collect();
        let chests = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.is_chest())
            .map(|(index, _)| (index, Chest::default()))
            .collect();
        Self {
            blocks: blocks.into(),
            furnaces,
            chests,
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
        let index = Self::index(x, y, z);
        let previous = self.blocks[index];
        if is_furnace(previous) && !is_furnace(block) {
            self.furnaces.remove(&index);
        } else if !is_furnace(previous) && is_furnace(block) {
            self.furnaces.entry(index).or_default();
        }
        if previous.is_chest() && !block.is_chest() {
            self.chests.remove(&index);
        } else if !previous.is_chest() && block.is_chest() {
            self.chests.entry(index).or_default();
        }
        Arc::make_mut(&mut self.blocks)[index] = block;
    }

    pub fn furnaces(&self) -> impl Iterator<Item = (usize, &Furnace)> {
        self.furnaces
            .iter()
            .map(|(index, furnace)| (*index, furnace))
    }

    pub fn furnace(&self, index: usize) -> Option<&Furnace> {
        self.furnaces.get(&index)
    }

    pub fn furnace_mut(&mut self, index: usize) -> Option<&mut Furnace> {
        self.furnaces.get_mut(&index)
    }

    pub fn insert_furnace(&mut self, index: usize, furnace: Furnace) {
        self.furnaces.insert(index, furnace);
    }

    pub fn chests(&self) -> impl Iterator<Item = (usize, &Chest)> {
        self.chests.iter().map(|(index, chest)| (*index, chest))
    }

    pub fn chest(&self, index: usize) -> Option<&Chest> {
        self.chests.get(&index)
    }

    pub fn chest_mut(&mut self, index: usize) -> Option<&mut Chest> {
        self.chests.get_mut(&index)
    }

    pub fn insert_chest(&mut self, index: usize, chest: Chest) {
        self.chests.insert(index, chest);
    }

    pub(crate) const fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }
}

fn is_furnace(block: BlockId) -> bool {
    block.is_furnace()
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
    /// Climate stored for the column, when that chunk is loaded.
    pub fn climate_at(&self, x: i32, z: i32) -> Option<Climate> {
        let generated = self.get(ChunkPos::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        Some(generated.biomes.get(local_x, local_z))
    }

    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<BlockId> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let chunk = self.get(ChunkPos::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        chunk.chunk.get(local_x, y as usize, local_z)
    }

    pub fn furnace_at(&self, x: i32, y: i32, z: i32) -> Option<&Furnace> {
        let chunk = self.get(ChunkPos::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.furnace(index)
    }

    pub fn furnace_at_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut Furnace> {
        let chunk = self.get_mut(ChunkPos::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.furnace_mut(index)
    }

    pub fn furnace_positions(&self) -> Vec<(i32, i32, i32)> {
        let mut positions = Vec::new();
        for (chunk_pos, generated) in &self.chunks {
            for (index, _) in generated.chunk.furnaces() {
                let x = index % CHUNK_SIZE;
                let z = index / CHUNK_SIZE % CHUNK_SIZE;
                let y = index / (CHUNK_SIZE * CHUNK_SIZE);
                positions.push((
                    chunk_pos.x * CHUNK_SIZE as i32 + x as i32,
                    y as i32,
                    chunk_pos.z * CHUNK_SIZE as i32 + z as i32,
                ));
            }
        }
        positions
    }

    pub fn chest_at(&self, x: i32, y: i32, z: i32) -> Option<&Chest> {
        let chunk = self.get(ChunkPos::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.chest(index)
    }

    pub fn chest_at_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut Chest> {
        let chunk = self.get_mut(ChunkPos::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.chest_mut(index)
    }

    pub fn chest_positions(&self) -> Vec<(i32, i32, i32)> {
        let mut positions = Vec::new();
        for (chunk_pos, generated) in &self.chunks {
            for (index, _) in generated.chunk.chests() {
                let x = index % CHUNK_SIZE;
                let z = index / CHUNK_SIZE % CHUNK_SIZE;
                let y = index / (CHUNK_SIZE * CHUNK_SIZE);
                positions.push((
                    chunk_pos.x * CHUNK_SIZE as i32 + x as i32,
                    y as i32,
                    chunk_pos.z * CHUNK_SIZE as i32 + z as i32,
                ));
            }
        }
        positions
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

pub(crate) const fn local_index(x: i32, y: i32, z: i32) -> Option<usize> {
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return None;
    }
    let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
    let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
    Some(Chunk::index(local_x, y as usize, local_z))
}

/// Chunks whose meshes can change when the block at `(x, z)` is edited.
///
/// The edited chunk is always included. A neighbour is included when the block
/// sits on that shared face. A corner edit also affects diagonal ambient
/// occlusion and lighting samples, so the diagonal chunk is included.
pub fn remesh_chunks_touching(x: i32, z: i32) -> Vec<ChunkPos> {
    let position = ChunkPos::from_block(x, z);
    let local_x = x.rem_euclid(CHUNK_SIZE as i32);
    let local_z = z.rem_euclid(CHUNK_SIZE as i32);
    let mut positions = vec![position];
    let mut x_neighbor = None;
    let mut z_neighbor = None;
    if local_x == 0 {
        x_neighbor = Some(position.x - 1);
    } else if local_x == CHUNK_SIZE as i32 - 1 {
        x_neighbor = Some(position.x + 1);
    }
    if local_z == 0 {
        z_neighbor = Some(position.z - 1);
    } else if local_z == CHUNK_SIZE as i32 - 1 {
        z_neighbor = Some(position.z + 1);
    }
    if let Some(x) = x_neighbor {
        positions.push(ChunkPos { x, z: position.z });
    }
    if let Some(z) = z_neighbor {
        positions.push(ChunkPos { x: position.x, z });
    }
    if let (Some(x), Some(z)) = (x_neighbor, z_neighbor) {
        positions.push(ChunkPos { x, z });
    }
    positions
}

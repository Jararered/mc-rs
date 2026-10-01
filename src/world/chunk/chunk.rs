use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::Resource;

use crate::block::id::Id;
use crate::world::biome::Climate;
use crate::world::chest::Chest;
use crate::world::chunk::GeneratedChunk;
use crate::world::furnace::Furnace;

use super::ChunkPosition;

pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_HEIGHT: usize = 128;
const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;
/// Beta's `NibbleArray`: two 4-bit metadata values per byte.
const METADATA_BYTES: usize = CHUNK_VOLUME / 2;
/// Chunks render as 16×16×16 sections, the granularity of Beta's
/// `WorldRenderer`, so an edit rebuilds and uploads one small mesh.
pub const SECTION_HEIGHT: usize = 16;
pub const SECTIONS_PER_CHUNK: usize = CHUNK_HEIGHT / SECTION_HEIGHT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChestGroup {
    /// First half in stable world order: west-to-east or north-to-south.
    pub first: (i32, i32, i32),
    pub second: Option<(i32, i32, i32)>,
}

impl ChestGroup {
    pub const fn is_double(self) -> bool {
        self.second.is_some()
    }

    pub const fn slot_count(self) -> usize {
        if self.is_double() { 54 } else { 27 }
    }
}

/// A scheduled block tick carried by a chunk that is not loaded into the live
/// world. While a chunk is loaded its ticks live in the block tick scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingTick {
    /// Flat block index in [`Chunk::index`] order.
    pub index: u16,
    /// The block the tick was scheduled for. It only runs if the cell still
    /// holds this block.
    pub block: Id,
    /// Ticks remaining when the chunk left the world.
    pub delay: u32,
}

#[derive(Clone)]
pub struct Chunk {
    // Meshing snapshots the center chunk and eight neighbors. Sharing their
    // immutable blocks avoids copying nine full arrays for every mesh job;
    // edits detach only the modified chunk.
    //
    // Raw `Id` bytes, not `Id` values: the `Unknown(u8)` catch-all makes `Id`
    // two bytes wide, which would double every chunk and generator cache.
    blocks: Arc<[u8]>,
    /// Beta's per-block 4-bit metadata (`Chunk.data`): fluid levels, crop and
    /// cactus ages, farmland moisture, and leaf decay flags. Packed two cells
    /// per byte in [`Self::index`] order, low nibble first. Most chunks never
    /// store a nonzero value, so the array is allocated on the first one.
    metadata: Option<Arc<[u8]>>,
    /// Block-local inventories and simulation state. Keyed by flat block index.
    furnaces: HashMap<usize, Furnace>,
    chests: HashMap<usize, Chest>,
    /// Scheduled ticks saved with the chunk. Empty while the chunk is live.
    pending_ticks: Vec<PendingTick>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            blocks: vec![Id::Air.as_u8(); CHUNK_VOLUME].into(),
            metadata: None,
            furnaces: HashMap::new(),
            chests: HashMap::new(),
            pending_ticks: Vec::new(),
        }
    }

    /// Rebuild a chunk from a flat block array in [`Self::index`] order.
    pub fn from_blocks(blocks: Vec<Id>) -> Self {
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
            blocks: blocks.into_iter().map(Id::as_u8).collect(),
            metadata: None,
            furnaces,
            chests,
            pending_ticks: Vec::new(),
        }
    }

    /// Rebuild a chunk from raw block bytes in [`Self::index`] order.
    pub fn from_raw(blocks: Vec<u8>) -> Self {
        assert_eq!(
            blocks.len(),
            CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE,
            "a chunk holds exactly one block per position"
        );
        let indices = |matches: fn(Id) -> bool| {
            blocks
                .iter()
                .enumerate()
                .filter(move |(_, raw)| matches(Id::from(**raw)))
                .map(|(index, _)| index)
        };
        let furnaces = indices(is_furnace)
            .map(|index| (index, Furnace::default()))
            .collect();
        let chests = indices(Id::is_chest)
            .map(|index| (index, Chest::default()))
            .collect();
        Self {
            blocks: blocks.into(),
            metadata: None,
            furnaces,
            chests,
            pending_ticks: Vec::new(),
        }
    }

    /// Scheduled ticks stored with this chunk while it is out of the world.
    pub fn pending_ticks(&self) -> &[PendingTick] {
        &self.pending_ticks
    }

    pub fn set_pending_ticks(&mut self, ticks: Vec<PendingTick>) {
        self.pending_ticks = ticks;
    }

    pub fn take_pending_ticks(&mut self) -> Vec<PendingTick> {
        std::mem::take(&mut self.pending_ticks)
    }

    /// The flat array of raw block bytes in [`Self::index`] order.
    pub fn raw_blocks(&self) -> &[u8] {
        &self.blocks
    }

    /// Packed metadata nibbles, or `None` while every value is zero.
    pub fn raw_metadata(&self) -> Option<&[u8]> {
        self.metadata.as_deref()
    }

    /// Replace every metadata value from packed nibbles in the
    /// [`Self::raw_metadata`] layout. A wrong length clears the metadata.
    pub fn set_raw_metadata(&mut self, packed: Vec<u8>) {
        self.metadata = (packed.len() == METADATA_BYTES && packed.iter().any(|&byte| byte != 0))
            .then(|| packed.into());
    }

    /// Beta `Chunk.getBlockMetadata`. Out-of-range cells read zero.
    pub fn metadata(&self, x: usize, y: usize, z: usize) -> u8 {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return 0;
        }
        self.metadata_at_index(Self::index(x, y, z))
    }

    pub(crate) fn metadata_at_index(&self, index: usize) -> u8 {
        self.metadata.as_ref().map_or(0, |packed| {
            let byte = packed[index / 2];
            if index % 2 == 0 {
                byte & 0x0f
            } else {
                byte >> 4
            }
        })
    }

    /// Beta `Chunk.setBlockMetadata`. Only the low four bits are stored.
    pub fn set_metadata(&mut self, x: usize, y: usize, z: usize, value: u8) {
        assert!(x < CHUNK_SIZE && y < CHUNK_HEIGHT && z < CHUNK_SIZE);
        self.set_metadata_at_index(Self::index(x, y, z), value);
    }

    fn set_metadata_at_index(&mut self, index: usize, value: u8) {
        let value = value & 0x0f;
        if self.metadata_at_index(index) == value {
            return;
        }
        let packed = Arc::make_mut(
            self.metadata
                .get_or_insert_with(|| vec![0; METADATA_BYTES].into()),
        );
        let byte = &mut packed[index / 2];
        *byte = if index % 2 == 0 {
            (*byte & 0xf0) | value
        } else {
            (*byte & 0x0f) | (value << 4)
        };
    }

    /// Every block decoded, in [`Self::index`] order. This copies the chunk;
    /// hot paths read [`Self::raw_blocks`].
    pub fn blocks(&self) -> Vec<Id> {
        self.blocks.iter().map(|&raw| Id::from(raw)).collect()
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<Id> {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return None;
        }
        Some(Id::from(self.blocks[Self::index(x, y, z)]))
    }

    /// Replace a block. Like Beta's `Chunk.setBlockID`, a different block
    /// starts with metadata zero; setting the same block keeps its metadata.
    pub fn set(&mut self, x: usize, y: usize, z: usize, block: Id) {
        assert!(x < CHUNK_SIZE && y < CHUNK_HEIGHT && z < CHUNK_SIZE);
        let index = Self::index(x, y, z);
        if Id::from(self.blocks[index]) != block {
            self.set_metadata_at_index(index, 0);
        }
        self.set_block_only(index, block);
    }

    /// Beta `Chunk.setBlockIDWithMetadata`.
    pub fn set_with_metadata(&mut self, x: usize, y: usize, z: usize, block: Id, metadata: u8) {
        assert!(x < CHUNK_SIZE && y < CHUNK_HEIGHT && z < CHUNK_SIZE);
        let index = Self::index(x, y, z);
        self.set_block_only(index, block);
        self.set_metadata_at_index(index, metadata);
    }

    fn set_block_only(&mut self, index: usize, block: Id) {
        let previous = Id::from(self.blocks[index]);
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
        Arc::make_mut(&mut self.blocks)[index] = block.as_u8();
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

    pub const fn index(x: usize, y: usize, z: usize) -> usize {
        (y * CHUNK_SIZE + z) * CHUNK_SIZE + x
    }
}

fn is_furnace(block: Id) -> bool {
    block.is_furnace()
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Resource, Default)]
pub struct WorldChunks {
    chunks: HashMap<ChunkPosition, GeneratedChunk>,
    membership_revision: u64,
}

impl WorldChunks {
    pub fn insert(&mut self, position: ChunkPosition, chunk: GeneratedChunk) {
        self.chunks.insert(position, chunk);
        self.membership_revision = self.membership_revision.wrapping_add(1);
    }

    pub fn get(&self, position: ChunkPosition) -> Option<&GeneratedChunk> {
        self.chunks.get(&position)
    }

    pub fn get_mut(&mut self, position: ChunkPosition) -> Option<&mut GeneratedChunk> {
        self.chunks.get_mut(&position)
    }

    pub fn contains(&self, position: ChunkPosition) -> bool {
        self.chunks.contains_key(&position)
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn positions(&self) -> impl Iterator<Item = ChunkPosition> + '_ {
        self.chunks.keys().copied()
    }

    /// Changes when chunks are inserted, replaced, or removed, rather than
    /// when simulation writes blocks inside an existing chunk.
    pub fn membership_revision(&self) -> u64 {
        self.membership_revision
    }

    pub fn clear(&mut self) {
        if !self.chunks.is_empty() {
            self.chunks.clear();
            self.membership_revision = self.membership_revision.wrapping_add(1);
        }
    }

    pub fn remove(&mut self, position: ChunkPosition) -> Option<GeneratedChunk> {
        let removed = self.chunks.remove(&position);
        if removed.is_some() {
            self.membership_revision = self.membership_revision.wrapping_add(1);
        }
        removed
    }

    /// Block at a world-space integer position, if that chunk is loaded and `y`
    /// is inside the world height.
    /// Climate stored for the column, when that chunk is loaded.
    pub fn climate_at(&self, x: i32, z: i32) -> Option<Climate> {
        let generated = self.get(ChunkPosition::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        Some(generated.biomes.get(local_x, local_z))
    }

    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<Id> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let chunk = self.get(ChunkPosition::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        chunk.chunk.get(local_x, y as usize, local_z)
    }

    /// Beta `World.getBlockMetadata`: zero outside the world or a loaded chunk.
    pub fn metadata_at(&self, x: i32, y: i32, z: i32) -> u8 {
        let Some(index) = local_index(x, y, z) else {
            return 0;
        };
        self.get(ChunkPosition::from_block(x, z))
            .map_or(0, |generated| generated.chunk.metadata_at_index(index))
    }

    /// Beta `World.setBlockMetadata`. Returns `false` when the cell is outside
    /// the world or its chunk is not loaded.
    pub fn set_metadata(&mut self, x: i32, y: i32, z: i32, metadata: u8) -> bool {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return false;
        }
        let Some(generated) = self.get_mut(ChunkPosition::from_block(x, z)) else {
            return false;
        };
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        generated
            .chunk
            .set_metadata(local_x, y as usize, local_z, metadata);
        true
    }

    /// Replace a loaded block and its metadata, refreshing the heightmap when
    /// the block changes. Returns the previous block and metadata, or `None`
    /// when the cell is outside the world or its chunk is not loaded.
    pub fn set_block_with_metadata(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block: Id,
        metadata: u8,
    ) -> Option<(Id, u8)> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let generated = self.get_mut(ChunkPosition::from_block(x, z))?;
        let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
        let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
        let y = y as usize;
        let previous = generated.chunk.get(local_x, y, local_z)?;
        let previous_metadata = generated.chunk.metadata(local_x, y, local_z);
        generated
            .chunk
            .set_with_metadata(local_x, y, local_z, block, metadata);
        if previous != block {
            generated
                .heightmap
                .recompute_column(&generated.chunk, local_x, local_z);
        }
        Some((previous, previous_metadata))
    }

    pub fn furnace_at(&self, x: i32, y: i32, z: i32) -> Option<&Furnace> {
        let chunk = self.get(ChunkPosition::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.furnace(index)
    }

    pub fn furnace_at_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut Furnace> {
        let chunk = self.get_mut(ChunkPosition::from_block(x, z))?;
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
        let chunk = self.get(ChunkPosition::from_block(x, z))?;
        let index = local_index(x, y, z)?;
        chunk.chunk.chest(index)
    }

    pub fn chest_at_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut Chest> {
        let chunk = self.get_mut(ChunkPosition::from_block(x, z))?;
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

    /// Resolve one chest or a valid adjacent pair in stable inventory order.
    /// Invalid clusters never become a multi-chest inventory view.
    pub fn chest_group_at(&self, x: i32, y: i32, z: i32) -> Option<ChestGroup> {
        if !self.block_at(x, y, z).is_some_and(Id::is_chest) {
            return None;
        }
        let adjacent = [(x - 1, y, z), (x + 1, y, z), (x, y, z - 1), (x, y, z + 1)]
            .into_iter()
            .filter(|&(nx, ny, nz)| self.block_at(nx, ny, nz).is_some_and(Id::is_chest))
            .collect::<Vec<_>>();
        let [neighbor] = adjacent.as_slice() else {
            return adjacent.is_empty().then_some(ChestGroup {
                first: (x, y, z),
                second: None,
            });
        };

        let pair = [(x, y, z), *neighbor];
        for position in pair {
            let (px, py, pz) = position;
            for other in [
                (px - 1, py, pz),
                (px + 1, py, pz),
                (px, py, pz - 1),
                (px, py, pz + 1),
            ] {
                if other != pair[0]
                    && other != pair[1]
                    && self
                        .block_at(other.0, other.1, other.2)
                        .is_some_and(Id::is_chest)
                {
                    return None;
                }
            }
        }
        let [mut first, mut second] = pair;
        if first.0 == second.0 {
            if first.2 > second.2 {
                std::mem::swap(&mut first, &mut second);
            }
        } else if first.0 > second.0 {
            std::mem::swap(&mut first, &mut second);
        }
        Some(ChestGroup {
            first,
            second: Some(second),
        })
    }

    /// Replace a loaded block and refresh that column's heightmap. A different
    /// block starts with metadata zero, as in Beta's `World.setBlock`.
    ///
    /// Returns the previous block, or `None` when the cell is outside the world
    /// or its chunk is not loaded.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block: Id) -> Option<Id> {
        if y < 0 || y >= CHUNK_HEIGHT as i32 {
            return None;
        }
        let generated = self.get_mut(ChunkPosition::from_block(x, z))?;
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
pub fn remesh_chunks_touching(x: i32, z: i32) -> Vec<ChunkPosition> {
    let position = ChunkPosition::from_block(x, z);
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
        positions.push(ChunkPosition { x, z: position.z });
    }
    if let Some(z) = z_neighbor {
        positions.push(ChunkPosition { x: position.x, z });
    }
    if let (Some(x), Some(z)) = (x_neighbor, z_neighbor) {
        positions.push(ChunkPosition { x, z });
    }
    positions
}

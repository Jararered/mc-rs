//! The world a chunk population pass reads and writes.
//!
//! Beta populates chunk `(x, z)` once `(x + 1, z)`, `(x, z + 1)`, and
//! `(x + 1, z + 1)` exist, and every feature it places lands in those four
//! chunks. [`PopulationWorld`] holds exactly those chunks, with Beta's
//! heightmap and the sky light it has at population time. Lighting updates are
//! queued rather than run while Beta populates, so light is the column model
//! from `Chunk.generateSkylightMap`, adjusted by `Chunk.relightBlock` as blocks
//! are placed. Block light is zero.

use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::is_solid_material;
use crate::world::chest::Chest;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;
use crate::world::chunk::ChunkPosition;

const HEIGHT: i32 = CHUNK_HEIGHT as i32;
const SIZE: i32 = CHUNK_SIZE as i32;
const CELLS: usize = CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE;

/// Beta `Block.lightOpacity`, in light levels.
pub(super) fn beta_opacity(block: Id) -> u8 {
    match block {
        Id::Leaves | Id::BirchLeaves | Id::SpruceLeaves => 1,
        Id::Water | Id::FlowingWater | Id::Ice => 3,
        Id::Lava | Id::FlowingLava | Id::Farmland => 15,
        _ if is_opaque_cube(block) => 15,
        _ => 0,
    }
}

/// Beta `Material.getIsLiquid`.
pub(super) fn is_liquid(block: Id) -> bool {
    matches!(
        block,
        Id::Water | Id::FlowingWater | Id::Lava | Id::FlowingLava
    )
}

pub(super) fn is_water(block: Id) -> bool {
    matches!(block, Id::Water | Id::FlowingWater)
}

/// Beta `Material.isSolid` and `getIsSolid`, which agree for every material.
pub(super) fn is_solid(block: Id) -> bool {
    is_solid_material(block)
}

pub(super) fn is_leaf(block: Id) -> bool {
    matches!(block, Id::Leaves | Id::BirchLeaves | Id::SpruceLeaves)
}

/// Beta stores every leaf species as block 18. Generators that look for "air
/// or leaves" treat all of them alike.
pub(super) fn is_air_or_leaves(block: Id) -> bool {
    block == Id::Air || is_leaf(block)
}

struct PopulatedChunk {
    chunk: Chunk,
    /// `Chunk.heightMap`: the lowest y whose column above passes light freely.
    heights: [u8; CHUNK_SIZE * CHUNK_SIZE],
    /// Saved sky light, one level per block in [`Chunk::index`] order.
    sky: Box<[u8]>,
}

impl PopulatedChunk {
    fn new(chunk: Chunk) -> Self {
        let mut populated = Self {
            chunk,
            heights: [0; CHUNK_SIZE * CHUNK_SIZE],
            sky: vec![0; CELLS].into_boxed_slice(),
        };
        populated.generate_skylight_map();
        populated
    }

    fn block(&self, x: usize, y: usize, z: usize) -> Id {
        Id::from(self.chunk.raw_blocks()[Chunk::index(x, y, z)])
    }

    /// `Chunk.generateSkylightMap`: heightmap, then full light down each
    /// column until opacity uses it up.
    fn generate_skylight_map(&mut self) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut height = CHUNK_HEIGHT - 1;
                while height > 0 && beta_opacity(self.block(x, height - 1, z)) == 0 {
                    height -= 1;
                }
                self.heights[z * CHUNK_SIZE + x] = height as u8;
                let mut light = 15i32;
                let mut y = CHUNK_HEIGHT - 1;
                loop {
                    light -= i32::from(beta_opacity(self.block(x, y, z)));
                    if light > 0 {
                        self.sky[Chunk::index(x, y, z)] = light as u8;
                    }
                    if y == 0 {
                        break;
                    }
                    y -= 1;
                    if y == 0 || light <= 0 {
                        break;
                    }
                }
            }
        }
    }

    /// `Chunk.relightBlock`: move the column's height and recompute the
    /// light below it, losing at least one level per block.
    fn relight(&mut self, x: usize, y: usize, z: usize) {
        let column = z * CHUNK_SIZE + x;
        let old = usize::from(self.heights[column]);
        let mut height = old.max(y);
        while height > 0 && beta_opacity(self.block(x, height - 1, z)) == 0 {
            height -= 1;
        }
        if height == old {
            return;
        }
        self.heights[column] = height as u8;
        if height < old {
            for level in height..old {
                self.sky[Chunk::index(x, level, z)] = 15;
            }
        } else {
            for level in old..height {
                self.sky[Chunk::index(x, level, z)] = 0;
            }
        }
        let mut light = 15i32;
        let mut level = height;
        while level > 0 && light > 0 {
            level -= 1;
            let opacity = i32::from(beta_opacity(self.block(x, level, z))).max(1);
            light = (light - opacity).max(0);
            self.sky[Chunk::index(x, level, z)] = light as u8;
        }
    }
}

pub struct PopulationWorld {
    /// The chunk being populated. Its three `+x`/`+z` neighbors follow.
    origin: ChunkPosition,
    chunks: [PopulatedChunk; 4],
}

impl PopulationWorld {
    /// `chunks` are `origin`, `+x`, `+z`, and `+x+z`, in that order.
    pub fn new(origin: ChunkPosition, chunks: [Chunk; 4]) -> Self {
        Self {
            origin,
            chunks: chunks.map(PopulatedChunk::new),
        }
    }

    pub fn origin(&self) -> ChunkPosition {
        self.origin
    }

    pub fn into_chunks(self) -> [Chunk; 4] {
        self.chunks.map(|chunk| chunk.chunk)
    }

    fn locate(&self, x: i32, z: i32) -> Option<(usize, usize, usize)> {
        let dx = x.div_euclid(SIZE) - self.origin.x;
        let dz = z.div_euclid(SIZE) - self.origin.z;
        if !(0..2).contains(&dx) || !(0..2).contains(&dz) {
            return None;
        }
        Some((
            (dz * 2 + dx) as usize,
            x.rem_euclid(SIZE) as usize,
            z.rem_euclid(SIZE) as usize,
        ))
    }

    /// `World.getBlockId`. Outside the four chunks is air, which no Beta
    /// overworld feature reaches.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Id {
        if !(0..HEIGHT).contains(&y) {
            return Id::Air;
        }
        self.locate(x, z).map_or(Id::Air, |(chunk, lx, lz)| {
            self.chunks[chunk].block(lx, y as usize, lz)
        })
    }

    pub fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == Id::Air
    }

    /// `Chunk.setBlockID` through `World.setBlock`, including the heightmap
    /// and relight updates and the only `onBlockAdded` side effect that
    /// matters during generation: lava hardening next to water.
    pub fn set(&mut self, x: i32, y: i32, z: i32, block: Id) -> bool {
        if !self.set_raw(x, y, z, block) {
            return false;
        }
        if block == Id::Lava && self.touches_water(x, y, z) {
            self.set_raw(x, y, z, Id::Obsidian);
        }
        true
    }

    fn set_raw(&mut self, x: i32, y: i32, z: i32, block: Id) -> bool {
        if !(0..HEIGHT).contains(&y) {
            return false;
        }
        let Some((index, lx, lz)) = self.locate(x, z) else {
            return false;
        };
        let chunk = &mut self.chunks[index];
        let y = y as usize;
        if chunk.block(lx, y, lz) == block {
            return false;
        }
        let height = usize::from(chunk.heights[lz * CHUNK_SIZE + lx]);
        chunk.chunk.set(lx, y, lz, block);
        if beta_opacity(block) != 0 {
            if y >= height {
                chunk.relight(lx, y + 1, lz);
            }
        } else if y + 1 == height {
            chunk.relight(lx, y, lz);
        }
        true
    }

    /// `BlockFluid.checkForHarden`'s neighbor test for lava.
    fn touches_water(&self, x: i32, y: i32, z: i32) -> bool {
        [
            (x, y, z - 1),
            (x, y, z + 1),
            (x - 1, y, z),
            (x + 1, y, z),
            (x, y + 1, z),
        ]
        .into_iter()
        .any(|(x, y, z)| is_water(self.get(x, y, z)))
    }

    pub fn set_chest(&mut self, x: i32, y: i32, z: i32, chest: Chest) {
        self.set(x, y, z, Id::Chest);
        if let Some((index, lx, lz)) = self.locate(x, z)
            && (0..HEIGHT).contains(&y)
        {
            self.chunks[index]
                .chunk
                .insert_chest(Chunk::index(lx, y as usize, lz), chest);
        }
    }

    /// `World.getHeightValue`.
    pub fn height(&self, x: i32, z: i32) -> i32 {
        self.locate(x, z).map_or(0, |(chunk, lx, lz)| {
            i32::from(self.chunks[chunk].heights[lz * CHUNK_SIZE + lx])
        })
    }

    /// `World.getFullBlockLightValue`: saved sky light, since no block light
    /// has propagated yet.
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8 {
        if y < 0 {
            return 0;
        }
        let y = y.min(HEIGHT - 1) as usize;
        self.locate(x, z).map_or(0, |(chunk, lx, lz)| {
            self.chunks[chunk].sky[Chunk::index(lx, y, lz)]
        })
    }

    /// `World.getSavedLightValue(EnumSkyBlock.Sky, ...)`.
    pub fn sky_light(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..HEIGHT).contains(&y) {
            return if y < 0 { 0 } else { 15 };
        }
        self.light(x, y, z)
    }

    /// `World.canBlockSeeTheSky`.
    pub fn sees_sky(&self, x: i32, y: i32, z: i32) -> bool {
        y >= self.height(x, z)
    }

    /// `World.findTopSolidBlock`: one above the highest solid or liquid block.
    pub fn top_solid_block(&self, x: i32, z: i32) -> i32 {
        let mut y = HEIGHT - 1;
        while y > 0 {
            let block = self.get(x, y, z);
            if is_solid(block) || is_liquid(block) {
                return y + 1;
            }
            y -= 1;
        }
        -1
    }
}

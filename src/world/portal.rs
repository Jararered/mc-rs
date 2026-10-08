//! Beta's `Teleporter`: where a player comes out after stepping through a
//! Nether portal.
//!
//! `Minecraft.usePortal` scales the player's position by eight, switches
//! dimension, and hands the player to the teleporter, which looks for the
//! closest portal within 128 blocks and stands the player in it. If there is
//! none it builds one within 16 blocks, on the closest ledge with room for
//! it, or on an obsidian platform when nothing fits, and then finds that.
//!
//! The functions here work on a [`PortalArea`], a set of chunks held outside
//! the live world, so the search can run on a background task while the
//! destination is still unloaded.

use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::Heightmap;

/// How far `placeInExistingPortal` looks, in blocks on each horizontal axis.
pub const SEARCH_RADIUS: i32 = 128;
/// How far `createPortal` looks for somewhere to build.
pub const CREATE_RADIUS: i32 = 16;

const HEIGHT: i32 = CHUNK_HEIGHT as i32;
const SIZE: i32 = CHUNK_SIZE as i32;

/// The chunks that hold every block within `radius` of `x`, `z`, plus
/// `margin` chunks around them.
pub fn chunks_within(x: f64, z: f64, radius: i32, margin: i32) -> Vec<ChunkPosition> {
    let (x, z) = (x.floor() as i32, z.floor() as i32);
    let low = ChunkPosition::from_block(x - radius, z - radius);
    let high = ChunkPosition::from_block(x + radius, z + radius);
    (low.x - margin..=high.x + margin)
        .flat_map(|x| (low.z - margin..=high.z + margin).map(move |z| ChunkPosition { x, z }))
        .collect()
}

/// A block the teleporter replaced, as `BlockTicks::block_changed` wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalChange {
    pub position: IVec3,
    pub previous: Block,
    pub previous_metadata: u8,
}

/// The blocks a teleporter reads and writes. Cells outside the held chunks
/// read as air and ignore writes.
#[derive(Default)]
pub struct PortalArea {
    chunks: HashMap<ChunkPosition, GeneratedChunk>,
    changes: Vec<PortalChange>,
}

impl PortalArea {
    pub fn new(chunks: HashMap<ChunkPosition, GeneratedChunk>) -> Self {
        Self {
            chunks,
            changes: Vec::new(),
        }
    }

    pub fn insert(&mut self, position: ChunkPosition, chunk: GeneratedChunk) {
        self.chunks.insert(position, chunk);
    }

    pub fn contains(&self, position: ChunkPosition) -> bool {
        self.chunks.contains_key(&position)
    }

    /// The chunks, and every block that was replaced, in order.
    pub fn into_parts(self) -> (HashMap<ChunkPosition, GeneratedChunk>, Vec<PortalChange>) {
        (self.chunks, self.changes)
    }

    fn local(x: i32, z: i32) -> (ChunkPosition, usize, usize) {
        (
            ChunkPosition::from_block(x, z),
            x.rem_euclid(SIZE) as usize,
            z.rem_euclid(SIZE) as usize,
        )
    }

    /// `World.getBlockId`.
    pub fn block(&self, x: i32, y: i32, z: i32) -> Block {
        if !(0..HEIGHT).contains(&y) {
            return Block::Air;
        }
        let (position, lx, lz) = Self::local(x, z);
        self.chunks
            .get(&position)
            .and_then(|generated| generated.chunk.get(lx, y as usize, lz))
            .unwrap_or(Block::Air)
    }

    fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.block(x, y, z) == Block::Air
    }

    fn is_portal(&self, x: i32, y: i32, z: i32) -> bool {
        self.block(x, y, z) == Block::NetherPortal
    }

    fn set(&mut self, x: i32, y: i32, z: i32, block: Block) {
        if !(0..HEIGHT).contains(&y) {
            return;
        }
        let (position, lx, lz) = Self::local(x, z);
        let Some(generated) = self.chunks.get_mut(&position) else {
            return;
        };
        let y = y as usize;
        let previous = generated.chunk.get(lx, y, lz).unwrap_or(Block::Air);
        if previous == block {
            return;
        }
        let previous_metadata = generated.chunk.metadata(lx, y, lz);
        generated.chunk.set(lx, y, lz, block);
        generated.heightmap = Heightmap::from_chunk(&generated.chunk);
        self.changes.push(PortalChange {
            position: IVec3::new(x, y as i32, z),
            previous,
            previous_metadata,
        });
    }

    /// Every portal block with no portal block under it, the cells
    /// `placeInExistingPortal` measures to, in its scan order: `x`, then `z`,
    /// then from the top down.
    fn portal_bases(&self) -> Vec<IVec3> {
        let portal = Block::NetherPortal.as_u8();
        let mut bases = Vec::new();
        for (position, generated) in &self.chunks {
            let raw = generated.chunk.raw_blocks();
            for (index, _) in raw.iter().enumerate().filter(|(_, raw)| **raw == portal) {
                let (lx, lz, y) = (
                    index % CHUNK_SIZE,
                    index / CHUNK_SIZE % CHUNK_SIZE,
                    index / 256,
                );
                if y > 0 && raw[index - 256] == portal {
                    continue;
                }
                bases.push(IVec3::new(
                    position.x * SIZE + lx as i32,
                    y as i32,
                    position.z * SIZE + lz as i32,
                ));
            }
        }
        bases.sort_by_key(|base| (base.x, base.z, -base.y));
        bases
    }
}

/// Whether a chunk holds any portal block, to decide which chunks a search
/// needs to keep.
pub fn has_portal(chunk: &GeneratedChunk) -> bool {
    chunk
        .chunk
        .raw_blocks()
        .contains(&Block::NetherPortal.as_u8())
}

/// `Teleporter.placeInExistingPortal` (`func_4106_b`): the closest portal
/// within [`SEARCH_RADIUS`] of `entity`, as the position the player's feet are
/// set to: centred between the portal's two columns, half a block above its
/// floor.
///
/// `entity` is Beta's `posX`/`posY`/`posZ`. The first portal found wins a tie.
pub fn find_exit(area: &PortalArea, entity: DVec3) -> Option<DVec3> {
    let (ex, ez) = (entity.x.floor() as i32, entity.z.floor() as i32);
    let mut best: Option<(f64, IVec3)> = None;
    for base in area.portal_bases() {
        if (base.x - ex).abs() > SEARCH_RADIUS || (base.z - ez).abs() > SEARCH_RADIUS {
            continue;
        }
        let distance = (base.as_dvec3() + DVec3::splat(0.5)).distance_squared(entity);
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, base));
        }
    }
    let (_, base) = best?;
    let mut exit = base.as_dvec3() + DVec3::splat(0.5);
    if area.is_portal(base.x - 1, base.y, base.z) {
        exit.x -= 0.5;
    }
    if area.is_portal(base.x + 1, base.y, base.z) {
        exit.x += 0.5;
    }
    if area.is_portal(base.x, base.y, base.z - 1) {
        exit.z -= 0.5;
    }
    if area.is_portal(base.x, base.y, base.z + 1) {
        exit.z += 0.5;
    }
    Some(exit)
}

/// The horizontal step along a portal's width and the step out of its plane,
/// for one of `createPortal`'s four rotations.
fn rotation_axes(rotation: i32) -> (i32, i32) {
    let mut along = rotation % 2;
    let mut across = 1 - along;
    if rotation % 4 >= 2 {
        along = -along;
        across = -across;
    }
    (along, across)
}

/// A floor cell must be solid and everything above it air.
fn fits(area: &PortalArea, x: i32, y: i32, z: i32, height: i32) -> bool {
    if height < 0 {
        area.block(x, y, z).is_solid_material()
    } else {
        area.is_air(x, y, z)
    }
}

/// Keep the candidate closest to the entity; the first one found wins a tie.
fn consider(
    best: &mut Option<(f64, IVec3, i32)>,
    entity: DVec3,
    x: i32,
    y: i32,
    z: i32,
    direction: i32,
) {
    let distance = DVec3::new(
        f64::from(x) + 0.5 - entity.x,
        f64::from(y) + 0.5 - entity.y,
        f64::from(z) + 0.5 - entity.z,
    )
    .length_squared();
    if best.is_none_or(|(nearest, ..)| distance < nearest) {
        *best = Some((distance, IVec3::new(x, y, z), direction));
    }
}

/// `Teleporter.createPortal` (`func_4108_c`): build a portal near `entity`.
///
/// The first pass wants a floor four wide and three deep with four blocks of
/// air over it, in any of four rotations starting from `rotation` (Beta draws
/// it from an unseeded random). The second settles for a floor four wide with
/// no depth. Each keeps the candidate closest to the entity. With neither, a
/// platform is built at the entity's own column, between y 70 and 118.
pub fn create_portal(area: &mut PortalArea, entity: DVec3, rotation: u32) {
    let rotation = (rotation % 4) as i32;
    let (ex, ey, ez) = (
        entity.x.floor() as i32,
        entity.y.floor() as i32,
        entity.z.floor() as i32,
    );
    let mut best: Option<(f64, IVec3, i32)> = None;
    for x in ex - CREATE_RADIUS..=ex + CREATE_RADIUS {
        for z in ez - CREATE_RADIUS..=ez + CREATE_RADIUS {
            let mut y = HEIGHT - 1;
            'column: while y >= 0 {
                if area.is_air(x, y, z) {
                    while y > 0 && area.is_air(x, y - 1, z) {
                        y -= 1;
                    }
                    for direction in rotation..rotation + 4 {
                        let (along, across) = rotation_axes(direction);
                        for depth in 0..3 {
                            for width in 0..4 {
                                for height in -1..4 {
                                    let bx = x + (width - 1) * along + depth * across;
                                    let bz = z + (width - 1) * across - depth * along;
                                    if !fits(area, bx, y + height, bz, height) {
                                        // A failure gives up on this ledge
                                        // altogether, not just the rotation.
                                        y -= 1;
                                        continue 'column;
                                    }
                                }
                            }
                        }
                        consider(&mut best, entity, x, y, z, direction % 4);
                    }
                }
                y -= 1;
            }
        }
    }

    if best.is_none() {
        for x in ex - CREATE_RADIUS..=ex + CREATE_RADIUS {
            for z in ez - CREATE_RADIUS..=ez + CREATE_RADIUS {
                let mut y = HEIGHT - 1;
                'column: while y >= 0 {
                    if area.is_air(x, y, z) {
                        // Beta has no floor guard here; every column it
                        // meets ends in bedrock.
                        while y > 0 && area.is_air(x, y - 1, z) {
                            y -= 1;
                        }
                        for direction in rotation..rotation + 2 {
                            let along = direction % 2;
                            let across = 1 - along;
                            for width in 0..4 {
                                for height in -1..4 {
                                    let bx = x + (width - 1) * along;
                                    let bz = z + (width - 1) * across;
                                    if !fits(area, bx, y + height, bz, height) {
                                        y -= 1;
                                        continue 'column;
                                    }
                                }
                            }
                            consider(&mut best, entity, x, y, z, direction % 2);
                        }
                    }
                    y -= 1;
                }
            }
        }
    }

    let found = best.is_some();
    let (_, mut origin, direction) = best.unwrap_or((0.0, IVec3::new(ex, ey, ez), 0));
    let (along, across) = rotation_axes(direction);
    if !found {
        origin.y = origin.y.clamp(70, 118);
        for depth in -1..=1 {
            for width in 1..3 {
                for height in -1..3 {
                    let block = if height < 0 {
                        Block::Obsidian
                    } else {
                        Block::Air
                    };
                    area.set(
                        origin.x + (width - 1) * along + depth * across,
                        origin.y + height,
                        origin.z + (width - 1) * across - depth * along,
                        block,
                    );
                }
            }
        }
    }
    // Beta writes the frame four times with notifications held back so the
    // portal blocks never see a half-built frame. One pass leaves the same
    // blocks.
    for width in 0..4 {
        for height in -1..4 {
            let frame = width == 0 || width == 3 || height == -1 || height == 3;
            area.set(
                origin.x + (width - 1) * along,
                origin.y + height,
                origin.z + (width - 1) * across,
                if frame {
                    Block::Obsidian
                } else {
                    Block::NetherPortal
                },
            );
        }
    }
}

/// `Teleporter.placeInPortal` (`func_4107_a`) over an area that already holds
/// every chunk it needs: use a portal that exists, or build one and use that.
pub fn place_in_portal(area: &mut PortalArea, entity: DVec3, rotation: u32) -> Option<DVec3> {
    if let Some(exit) = find_exit(area, entity) {
        return Some(exit);
    }
    create_portal(area, entity, rotation);
    find_exit(area, entity)
}

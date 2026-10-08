//! `BlockBed`'s metadata: the two low bits are the direction the bed points,
//! bit 4 marks an occupied bed, and bit 8 marks the half Beta calls the foot
//! (`isBlockFootOfBed`), which is the one with the pillow.

use bevy::math::IVec3;

use super::blocks::Block;
use crate::world::chunk::WorldChunks;

/// Metadata bit of a bed someone sleeps in.
pub const OCCUPIED: u8 = 4;
/// Metadata bit of the half `ItemBed` places second.
pub const FOOT: u8 = 8;

/// `BlockBed.headBlockToFootBlockMap`: the `(x, z)` step from the half
/// without [`FOOT`] to the half with it, by direction.
pub const HEAD_TO_FOOT: [(i32, i32); 4] = [(0, 1), (-1, 0), (0, -1), (1, 0)];

/// `BlockBed.getDirectionFromMetadata`.
pub const fn direction(metadata: u8) -> usize {
    (metadata & 3) as usize
}

/// `BlockBed.isBlockFootOfBed`.
pub const fn is_foot(metadata: u8) -> bool {
    metadata & FOOT != 0
}

/// `BlockBed.isBedOccupied`.
pub const fn is_occupied(metadata: u8) -> bool {
    metadata & OCCUPIED != 0
}

/// The step from the half without [`FOOT`] to the half with it.
pub const fn head_to_foot(metadata: u8) -> IVec3 {
    let (x, z) = HEAD_TO_FOOT[direction(metadata)];
    IVec3::new(x, 0, z)
}

/// `BlockBed.getNearestEmptyChunkCoordinates`: a cell beside the bed with a
/// normal cube under it and two cells of air, skipping the first `skip`.
pub fn nearest_empty(chunks: &WorldChunks, bed: IVec3, mut skip: u32) -> Option<IVec3> {
    let (step_x, step_z) = HEAD_TO_FOOT[direction(chunks.metadata_at(bed.x, bed.y, bed.z))];
    let air = |x, y, z| chunks.block_at(x, y, z) == Some(Block::Air);
    for part in 0..=1 {
        let min_x = bed.x - step_x * part - 1;
        let min_z = bed.z - step_z * part - 1;
        for x in min_x..=min_x + 2 {
            for z in min_z..=min_z + 2 {
                if chunks
                    .block_at(x, bed.y - 1, z)
                    .is_some_and(Block::is_normal_cube)
                    && air(x, bed.y, z)
                    && air(x, bed.y + 1, z)
                {
                    if skip == 0 {
                        return Some(IVec3::new(x, bed.y, z));
                    }
                    skip -= 1;
                }
            }
        }
    }
    None
}

//! Beta `TileEntityPiston`: a block on its way into a cell, stored on the
//! `MovingPiston` block that holds the cell while it slides.

use bevy::math::IVec3;
use bevy::math::Vec3;

use crate::block::blocks::Block;

/// `PistonBlockTextures.offsets{X,Y,Z}ForSide`.
pub const PISTON_OFFSETS: [IVec3; 6] = [
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
    IVec3::NEG_X,
    IVec3::X,
];

/// Each tick moves a block this far (`field_31022_k += 0.5F`).
pub const PROGRESS_PER_TICK: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovingPiston {
    /// `storedBlockID`: what the cell becomes when the move ends.
    pub block: Block,
    /// `storedMetadata`.
    pub metadata: u8,
    /// `field_31025_c`: the side the piston faces, 0..6.
    pub facing: u8,
    /// `field_31024_i`: moving away from the piston.
    pub extending: bool,
    /// `field_31023_j`: a retracting piston's own base. Beta does not save
    /// it.
    pub source: bool,
    /// `field_31022_k`.
    pub progress: f32,
    /// `field_31020_l`: the progress at the start of this tick.
    pub last_progress: f32,
    /// When it was created, for Beta's `loadedTileEntityList` order.
    pub sequence: u64,
}

impl MovingPiston {
    pub fn new(block: Block, metadata: u8, facing: u8, extending: bool, source: bool) -> Self {
        Self {
            block,
            metadata,
            facing,
            extending,
            source,
            progress: 0.0,
            last_progress: 0.0,
            sequence: 0,
        }
    }

    pub fn direction(&self) -> IVec3 {
        PISTON_OFFSETS[usize::from(self.facing.min(5))]
    }

    /// `getProgress`.
    pub fn progress_at(&self, partial: f32) -> f32 {
        let partial = partial.min(1.0);
        self.last_progress + (self.progress - self.last_progress) * partial
    }

    /// `func_31017_b`, `func_31014_c`, `func_31013_d`: where the block is
    /// drawn, relative to its cell.
    pub fn offset(&self, partial: f32) -> Vec3 {
        let progress = self.progress_at(partial);
        let along = if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        };
        self.direction().as_vec3() * along
    }

    /// `BlockPistonMoving.getCollisionBoundingBoxFromPool`: the stored
    /// block's box is moved back by this, along the facing. Beta moves a
    /// retracting block's box the wrong way, and so does this.
    pub fn collision_shift(&self) -> f32 {
        if self.extending {
            1.0 - self.last_progress
        } else {
            self.last_progress
        }
    }

    /// `BlockPistonMoving.getAxisAlignedBB`: the stored block's collision
    /// box in `cell`, moved back along the facing by `shift`.
    pub fn shifted_bounds(&self, cell: IVec3, shift: f32) -> Option<(Vec3, Vec3)> {
        if matches!(self.block, Block::Air | Block::MovingPiston) {
            return None;
        }
        let (min, max) = self.block.collision_bounds_for(self.metadata)?;
        let origin = cell.as_vec3() - self.direction().as_vec3() * shift;
        Some((
            origin + Vec3::from_array(min),
            origin + Vec3::from_array(max),
        ))
    }
}

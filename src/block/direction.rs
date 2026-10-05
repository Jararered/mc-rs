//! Horizontal orientation stored in block metadata, using Beta's encodings.

use super::blocks::Block;

/// A horizontal side of a block. Depending on the block it names the face
/// presented as the front (furnace, chest, pumpkin) or the side holding the
/// block up (torch, ladder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Direction {
    #[default]
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    /// The offset from a cell to its neighbor on this side.
    pub const fn offset(self) -> [i32; 3] {
        match self {
            Self::North => [0, 0, -1],
            Self::East => [1, 0, 0],
            Self::South => [0, 0, 1],
            Self::West => [-1, 0, 0],
        }
    }

    /// Mesh face index that points outwards on this side.
    pub const fn face_index(self) -> usize {
        match self {
            Self::East => 2,
            Self::West => 3,
            Self::South => 4,
            Self::North => 5,
        }
    }
}

impl Block {
    /// The orientation held in `metadata`, for blocks that have one. `None`
    /// for other blocks, a torch standing on the floor, and an unattached
    /// ladder. Furnaces, chests, and pumpkins always face somewhere.
    pub const fn facing(self, metadata: u8) -> Option<Direction> {
        use Direction::*;
        match (self, metadata & 15) {
            // Beta's front-face values: 2 north, 3 south, 4 west, 5 east.
            (Self::Furnace | Self::LitFurnace | Self::Chest, 3) => Some(South),
            (Self::Furnace | Self::LitFurnace | Self::Chest, 4) => Some(West),
            (Self::Furnace | Self::LitFurnace | Self::Chest, 5) => Some(East),
            (Self::Furnace | Self::LitFurnace | Self::Chest, _) => Some(North),
            // Beta's wall values name the side holding the torch up.
            (Self::Torch, 1) => Some(West),
            (Self::Torch, 2) => Some(East),
            (Self::Torch, 3) => Some(North),
            (Self::Torch, 4) => Some(South),
            // Beta's ladder values name the side of the supporting wall.
            (Self::Ladder, 2) => Some(South),
            (Self::Ladder, 3) => Some(North),
            (Self::Ladder, 4) => Some(East),
            (Self::Ladder, 5) => Some(West),
            // The pumpkin carving faces outward: 0 west, 1 south, 2 east, 3 north.
            (Self::Pumpkin, 1) => Some(South),
            (Self::Pumpkin, 2) => Some(East),
            (Self::Pumpkin, 3) => Some(North),
            (Self::Pumpkin, _) => Some(West),
            _ => None,
        }
    }

    /// The metadata that makes this block face `facing`; the inverse of
    /// [`Self::facing`]. Blocks without an orientation return 0.
    pub const fn facing_metadata(self, facing: Direction) -> u8 {
        use Direction::*;
        match (self, facing) {
            (Self::Furnace | Self::LitFurnace | Self::Chest, North) => 2,
            (Self::Furnace | Self::LitFurnace | Self::Chest, South) => 3,
            (Self::Furnace | Self::LitFurnace | Self::Chest, West) => 4,
            (Self::Furnace | Self::LitFurnace | Self::Chest, East) => 5,
            (Self::Torch, West) => 1,
            (Self::Torch, East) => 2,
            (Self::Torch, North) => 3,
            (Self::Torch, South) => 4,
            (Self::Ladder, South) => 2,
            (Self::Ladder, North) => 3,
            (Self::Ladder, East) => 4,
            (Self::Ladder, West) => 5,
            (Self::Pumpkin, West) => 0,
            (Self::Pumpkin, South) => 1,
            (Self::Pumpkin, East) => 2,
            (Self::Pumpkin, North) => 3,
            _ => 0,
        }
    }

    /// The offset from a torch or ladder to the block it hangs on.
    pub const fn support_offset(self, metadata: u8) -> Option<[i32; 3]> {
        match self {
            Self::Torch | Self::Ladder => match self.facing(metadata) {
                Some(facing) => Some(facing.offset()),
                None => None,
            },
            _ => None,
        }
    }
}

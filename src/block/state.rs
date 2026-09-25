//! Species, orientation, and inventory-state conversions for compact block values.

use crate::item::registry::ItemData;

use super::block::BlockId;

/// Horizontal face presented as the front of an oriented block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FurnaceFacing {
    #[default]
    North,
    East,
    South,
    West,
}

impl FurnaceFacing {
    /// Mesh face index that points outwards from a furnace front.
    pub const fn face_index(self) -> usize {
        match self {
            Self::East => 2,
            Self::West => 3,
            Self::South => 4,
            Self::North => 5,
        }
    }
}

impl BlockId {
    pub const fn is_furnace(self) -> bool {
        matches!(
            self,
            Self::Furnace
                | Self::LitFurnace
                | Self::FurnaceNorth
                | Self::FurnaceEast
                | Self::FurnaceSouth
                | Self::FurnaceWest
                | Self::LitFurnaceNorth
                | Self::LitFurnaceEast
                | Self::LitFurnaceSouth
                | Self::LitFurnaceWest
        )
    }

    pub const fn is_chest(self) -> bool {
        matches!(
            self,
            Self::Chest | Self::ChestNorth | Self::ChestEast | Self::ChestSouth | Self::ChestWest
        )
    }

    pub const fn chest_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::ChestNorth | Self::Chest => Some(FurnaceFacing::North),
            Self::ChestEast => Some(FurnaceFacing::East),
            Self::ChestSouth => Some(FurnaceFacing::South),
            Self::ChestWest => Some(FurnaceFacing::West),
            _ => None,
        }
    }

    pub const fn with_chest_facing(self, facing: FurnaceFacing) -> Self {
        match facing {
            FurnaceFacing::North => Self::ChestNorth,
            FurnaceFacing::East => Self::ChestEast,
            FurnaceFacing::South => Self::ChestSouth,
            FurnaceFacing::West => Self::ChestWest,
        }
    }

    /// Ladder orientation names the wall the ladder is attached to.
    pub const fn is_ladder(self) -> bool {
        matches!(
            self,
            Self::Ladder
                | Self::LadderNorth
                | Self::LadderEast
                | Self::LadderSouth
                | Self::LadderWest
        )
    }

    /// The offset from this ladder cell to its supporting wall.
    pub const fn ladder_support_offset(self) -> Option<[i32; 3]> {
        match self {
            Self::LadderNorth => Some([0, 0, -1]),
            Self::LadderEast => Some([1, 0, 0]),
            Self::LadderSouth => Some([0, 0, 1]),
            Self::LadderWest => Some([-1, 0, 0]),
            _ => None,
        }
    }

    pub const fn with_ladder_support(self, support: FurnaceFacing) -> Self {
        match support {
            FurnaceFacing::North => Self::LadderNorth,
            FurnaceFacing::East => Self::LadderEast,
            FurnaceFacing::South => Self::LadderSouth,
            FurnaceFacing::West => Self::LadderWest,
        }
    }

    pub const fn is_lit_furnace(self) -> bool {
        matches!(
            self,
            Self::LitFurnace
                | Self::LitFurnaceNorth
                | Self::LitFurnaceEast
                | Self::LitFurnaceSouth
                | Self::LitFurnaceWest
        )
    }

    pub const fn furnace_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::FurnaceEast | Self::LitFurnaceEast => Some(FurnaceFacing::East),
            Self::FurnaceSouth | Self::LitFurnaceSouth => Some(FurnaceFacing::South),
            Self::FurnaceWest | Self::LitFurnaceWest => Some(FurnaceFacing::West),
            Self::Furnace | Self::LitFurnace | Self::FurnaceNorth | Self::LitFurnaceNorth => {
                Some(FurnaceFacing::North)
            }
            _ => None,
        }
    }

    pub const fn with_furnace_state(self, facing: FurnaceFacing, lit: bool) -> Self {
        match (facing, lit) {
            (FurnaceFacing::North, false) => Self::FurnaceNorth,
            (FurnaceFacing::East, false) => Self::FurnaceEast,
            (FurnaceFacing::South, false) => Self::FurnaceSouth,
            (FurnaceFacing::West, false) => Self::FurnaceWest,
            (FurnaceFacing::North, true) => Self::LitFurnaceNorth,
            (FurnaceFacing::East, true) => Self::LitFurnaceEast,
            (FurnaceFacing::South, true) => Self::LitFurnaceSouth,
            (FurnaceFacing::West, true) => Self::LitFurnaceWest,
        }
    }

    pub const fn pumpkin_facing(self) -> Option<FurnaceFacing> {
        match self {
            Self::PumpkinNorth => Some(FurnaceFacing::North),
            Self::PumpkinEast => Some(FurnaceFacing::East),
            Self::PumpkinSouth => Some(FurnaceFacing::South),
            Self::PumpkinWest | Self::Pumpkin => Some(FurnaceFacing::West),
            _ => None,
        }
    }

    pub const fn with_pumpkin_facing(self, facing: FurnaceFacing) -> Self {
        match facing {
            FurnaceFacing::North => Self::PumpkinNorth,
            FurnaceFacing::East => Self::PumpkinEast,
            FurnaceFacing::South => Self::PumpkinSouth,
            FurnaceFacing::West => Self::PumpkinWest,
        }
    }

    /// Convert Beta pumpkin metadata into the outward-facing side.
    pub const fn pumpkin_from_metadata(metadata: u32) -> Self {
        match metadata & 3 {
            0 => Self::PumpkinWest,
            1 => Self::PumpkinSouth,
            2 => Self::PumpkinEast,
            _ => Self::PumpkinNorth,
        }
    }

    pub fn with_furnace_lit(self, lit: bool) -> Self {
        match self {
            Self::Furnace if lit => Self::LitFurnace,
            Self::LitFurnace if !lit => Self::Furnace,
            Self::FurnaceNorth | Self::LitFurnaceNorth => {
                Self::Furnace.with_furnace_state(FurnaceFacing::North, lit)
            }
            Self::FurnaceEast | Self::LitFurnaceEast => {
                Self::Furnace.with_furnace_state(FurnaceFacing::East, lit)
            }
            Self::FurnaceSouth | Self::LitFurnaceSouth => {
                Self::Furnace.with_furnace_state(FurnaceFacing::South, lit)
            }
            Self::FurnaceWest | Self::LitFurnaceWest => {
                Self::Furnace.with_furnace_state(FurnaceFacing::West, lit)
            }
            block => block,
        }
    }

    /// Stack identity for this chunk block. Species stays in the metadata.
    /// Torch facing is dropped.
    pub const fn item_form(self) -> (Self, u8) {
        match self {
            Self::SpruceWood => (Self::Wood, 1),
            Self::BirchWood => (Self::Wood, 2),
            Self::SprucePlanks => (Self::WoodenPlanks, 1),
            Self::BirchPlanks => (Self::WoodenPlanks, 2),
            Self::SpruceLeaves => (Self::Leaves, 1),
            Self::BirchLeaves => (Self::Leaves, 2),
            Self::Torch
            | Self::TorchWest
            | Self::TorchEast
            | Self::TorchNorth
            | Self::TorchSouth => (Self::Torch, 0),
            Self::Fern => (Self::TallGrass, 2),
            Self::FurnaceNorth
            | Self::FurnaceEast
            | Self::FurnaceSouth
            | Self::FurnaceWest
            | Self::LitFurnaceNorth
            | Self::LitFurnaceEast
            | Self::LitFurnaceSouth
            | Self::LitFurnaceWest
            | Self::LitFurnace => (Self::Furnace, 0),
            Self::PumpkinNorth | Self::PumpkinEast | Self::PumpkinSouth | Self::PumpkinWest => {
                (Self::Pumpkin, 0)
            }
            Self::ChestNorth | Self::ChestEast | Self::ChestSouth | Self::ChestWest => {
                (Self::Chest, 0)
            }
            Self::LadderNorth | Self::LadderEast | Self::LadderSouth | Self::LadderWest => {
                (Self::Ladder, 0)
            }
            block => (block, 0),
        }
    }

    /// Chunk block for a placed stack. `None` for blocks the world does not
    /// simulate, and for metadata it does not implement.
    pub fn placed(self, metadata: u8) -> Option<Self> {
        match (self, metadata) {
            (Self::Wood, 1) => Some(Self::SpruceWood),
            (Self::Wood, 2) => Some(Self::BirchWood),
            (Self::WoodenPlanks, 1) => Some(Self::SprucePlanks),
            (Self::WoodenPlanks, 2) => Some(Self::BirchPlanks),
            (Self::Leaves, 1) => Some(Self::SpruceLeaves),
            (Self::Leaves, 2) => Some(Self::BirchLeaves),
            (Self::Torch, 0 | 5) => Some(Self::Torch),
            (Self::Torch, 1) => Some(Self::TorchWest),
            (Self::Torch, 2) => Some(Self::TorchEast),
            (Self::Torch, 3) => Some(Self::TorchNorth),
            (Self::Torch, 4) => Some(Self::TorchSouth),
            (Self::TallGrass, 0 | 1) => Some(Self::TallGrass),
            (Self::TallGrass, 2) => Some(Self::Fern),
            (Self::Pumpkin, 0) => Some(Self::Pumpkin),
            (Self::Ladder, 0 | 2) => Some(Self::Ladder),
            (block, 0) if block.in_world() => Some(block),
            _ => None,
        }
    }

    /// Inventory subtype for the direct block item.
    pub const fn item_data(self) -> ItemData {
        match self {
            Self::Sapling | Self::Wood | Self::Leaves | Self::WoodenPlanks => ItemData::Subtype(2),
            Self::Wool => ItemData::Subtype(15),
            Self::StoneSlab => ItemData::Subtype(3),
            _ => ItemData::None,
        }
    }
}

//! Stored column climate and biome data, independent of generation.

use crate::world::chunk::CHUNK_SIZE;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, IntoPrimitive)]
pub enum Biome {
    Rainforest,
    Swampland,
    SeasonalForest,
    Forest,
    Savanna,
    Shrubland,
    Taiga,
    Desert,
    Plains,
    IceDesert,
    Tundra,
    #[num_enum(catch_all)]
    Unknown(u8),
}

impl Biome {
    /// Stable on-disk value. Persistence stores this rather than the enum's
    /// variant index so saves survive new biomes being added.
    pub fn as_u8(self) -> u8 {
        self.into()
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match Self::from(value) {
            Self::Unknown(_) => None,
            biome => Some(biome),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Climate {
    pub temperature: f64,
    pub humidity: f64,
    pub biome: Biome,
}

#[derive(Clone)]
pub struct BiomeMap {
    cells: [Climate; CHUNK_SIZE * CHUNK_SIZE],
}

impl BiomeMap {
    /// Rebuild a biome map from stored cells, as produced by [`Self::cells`].
    pub fn from_cells(cells: [Climate; CHUNK_SIZE * CHUNK_SIZE]) -> Self {
        Self { cells }
    }

    /// The per-column climate in `x * CHUNK_SIZE + z` order.
    pub fn cells(&self) -> &[Climate; CHUNK_SIZE * CHUNK_SIZE] {
        &self.cells
    }

    pub fn get(&self, x: usize, z: usize) -> Climate {
        self.cells[x * CHUNK_SIZE + z]
    }
}

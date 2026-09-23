use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPos;
use num_enum::FromPrimitive;
use num_enum::IntoPrimitive;

use super::noise::SimplexOctaves;

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

pub(super) struct BiomeGenerator {
    temperature: SimplexOctaves,
    humidity: SimplexOctaves,
    precipitation: SimplexOctaves,
}

impl BiomeGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            temperature: SimplexOctaves::new(seed.wrapping_mul(9871), 4),
            humidity: SimplexOctaves::new(seed.wrapping_mul(39811), 4),
            precipitation: SimplexOctaves::new(seed.wrapping_mul(543321), 2),
        }
    }

    pub fn generate(&self, position: ChunkPos) -> BiomeMap {
        BiomeMap {
            cells: std::array::from_fn(|index| {
                let x = index / CHUNK_SIZE;
                let z = index % CHUNK_SIZE;
                let wx = (position.x * CHUNK_SIZE as i32 + x as i32) as f64;
                let wz = (position.z * CHUNK_SIZE as i32 + z as i32) as f64;
                self.climate_at(wx, wz)
            }),
        }
    }

    /// Climate at an arbitrary world column, independent of chunk boundaries.
    ///
    /// This is the same computation [`Self::generate`] performs per cell, exposed
    /// so decoration can sample climate outside the chunk it is generating.
    pub fn climate_at(&self, wx: f64, wz: f64) -> Climate {
        let precipitation = self
            .precipitation
            .sample(wx, wz, 0.25 / 1.5, 0.5882352941176471)
            * 1.1
            + 0.5;
        let temperature = (self
            .temperature
            .sample(wx, wz, 0.02500000037252903 / 1.5, 0.25)
            * 0.15
            + 0.7)
            * 0.99
            + precipitation * 0.01;
        let temperature = (1.0 - (1.0 - temperature).powi(2)).clamp(0.0, 1.0);
        let humidity = ((self
            .humidity
            .sample(wx, wz, 0.05000000074505806 / 1.5, 1.0 / 3.0)
            * 0.15
            + 0.5)
            * 0.998
            + precipitation * 0.002)
            .clamp(0.0, 1.0);
        // The source quantizes temperature and humidity to a 64×64 lookup table.
        let quantized_temperature = (temperature * 63.0) as u8;
        let quantized_humidity = (humidity * 63.0) as u8;
        let biome = classify(
            quantized_temperature as f64 / 63.0,
            quantized_humidity as f64 / 63.0,
        );
        Climate {
            temperature,
            humidity,
            biome,
        }
    }
}

fn classify(temperature: f64, humidity: f64) -> Biome {
    let rainfall = humidity * temperature;
    if temperature < 0.1 {
        Biome::Tundra
    } else if rainfall < 0.2 {
        if temperature < 0.5 {
            Biome::Tundra
        } else if temperature < 0.95 {
            Biome::Savanna
        } else {
            Biome::Desert
        }
    } else if rainfall > 0.5 && temperature < 0.7 {
        Biome::Swampland
    } else if temperature < 0.5 {
        Biome::Taiga
    } else if temperature < 0.97 {
        if rainfall < 0.35 {
            Biome::Shrubland
        } else {
            Biome::Forest
        }
    } else if rainfall < 0.45 {
        Biome::Plains
    } else if rainfall < 0.9 {
        Biome::SeasonalForest
    } else {
        Biome::Rainforest
    }
}

//! Beta Nether terrain and decoration: `ChunkProviderHell`.

mod caves;
mod generator;
mod population;
mod terrain;

use super::math;
use super::noise;
use super::world;

pub use generator::NetherGenerator;

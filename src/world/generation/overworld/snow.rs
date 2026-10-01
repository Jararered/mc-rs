//! The snow pass at the end of Beta's `populate`.

use crate::block::id::Id;
use crate::world::chunk::CHUNK_HEIGHT;

use super::biome::BiomeGenerator;
use super::world::PopulationWorld;
use super::world::is_solid;

/// Snow on every column of the populated 16×16 area, offset by eight blocks,
/// whose temperature, lowered with the top solid block's height, is below 0.5.
/// Beta checks no biome: any cold enough column is covered.
pub(super) fn place_snow(world: &mut PopulationWorld, biomes: &BiomeGenerator) {
    let origin = world.origin();
    let x0 = origin.x * 16 + 8;
    let z0 = origin.z * 16 + 8;
    for x in x0..x0 + 16 {
        for z in z0..z0 + 16 {
            let y = world.top_solid_block(x, z);
            let temperature = biomes.climate_at(f64::from(x), f64::from(z)).temperature
                - f64::from(y - 64) / 64.0 * 0.3;
            if temperature < 0.5
                && y > 0
                && y < CHUNK_HEIGHT as i32
                && world.is_air(x, y, z)
                && is_solid(world.get(x, y - 1, z))
                && world.get(x, y - 1, z) != Id::Ice
            {
                world.set(x, y, z, Id::SnowLayer);
            }
        }
    }
}

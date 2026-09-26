//! Beta-style post-population snow cover.

use crate::block::id::Id;
use crate::block::properties::blocks_movement;
use crate::block::properties::is_crossed_plant;
use crate::block::properties::is_torch;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::Chunk;

use super::biome::Biome;
use super::biome::BiomeMap;

/// Add a single default-thickness snow layer where Beta's population pass
/// would place snow: above exposed solid ground in snow-enabled biomes, with
/// temperature reduced by elevation.
pub(super) fn place_snow(chunk: &mut Chunk, biomes: &BiomeMap) {
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            if !matches!(
                biomes.get(x, z).biome,
                Biome::Taiga | Biome::IceDesert | Biome::Tundra
            ) {
                continue;
            }

            let Some(surface_y) = (0..CHUNK_HEIGHT)
                .rev()
                .find(|&y| occupies_surface(chunk.get(x, y, z).unwrap()))
                .map(|y| y + 1)
            else {
                continue;
            };
            if surface_y >= CHUNK_HEIGHT || chunk.get(x, surface_y, z) != Some(Id::Air) {
                continue;
            }

            let support = chunk.get(x, surface_y - 1, z).unwrap();
            if support == Id::Ice || !blocks_movement(support) {
                continue;
            }

            let temperature = biomes.get(x, z).temperature - (surface_y as f64 - 64.0) / 64.0 * 0.3;
            if temperature < 0.5 {
                chunk.set(x, surface_y, z, Id::SnowLayer);
            }
        }
    }
}

/// The reference height query includes solid blocks and liquids, but excludes
/// plants and torches that sit in the air above the surface.
fn occupies_surface(block: Id) -> bool {
    block != Id::Air && !is_crossed_plant(block) && !is_torch(block)
}

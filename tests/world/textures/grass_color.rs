use bevy::prelude::Mesh;
use game::block::block::BlockId;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::lighting::Skylight;
use game::world::meshing::ChunkNeighbors;
use game::world::meshing::mesh_chunk_with_biomes;
use game::world::textures::FoliageColors;
use game::world::textures::GrassColors;
use game::world::textures::PALETTE_SIZE;
use game::world::textures::palette_index;

#[test]
fn grass_palette_uses_temperature_and_temperature_weighted_humidity() {
    assert_eq!(palette_index(1.0, 1.0), 0);
    assert_eq!(palette_index(0.0, 0.0), 255 * 256 + 255);
    assert_eq!(palette_index(1.0, 0.0), 255 * 256);
    assert_eq!(palette_index(0.5, 1.0), 127 * 256 + 127);
}

#[test]
fn grass_color_varies_with_climate_within_one_biome() {
    let mut rgba = vec![0; PALETTE_SIZE * PALETTE_SIZE * 4];
    rgba[0..4].copy_from_slice(&[0, 255, 0, 255]);
    let dry = palette_index(1.0, 0.0) * 4;
    rgba[dry..dry + 4].copy_from_slice(&[255, 0, 0, 255]);
    let colors = GrassColors::from_rgba(rgba);
    let wet = colors.sample(Climate {
        temperature: 1.0,
        humidity: 1.0,
        biome: Biome::Forest,
    });
    let dry = colors.sample(Climate {
        temperature: 1.0,
        humidity: 0.0,
        biome: Biome::Forest,
    });
    assert_eq!(wet, [0.0, 1.0, 0.0]);
    assert_eq!(dry, [1.0, 0.0, 0.0]);
}

#[test]
fn crossed_grass_mesh_uses_each_column_biome_grass_color() {
    let mut chunk = Chunk::new();
    chunk.set(2, 64, 3, BlockId::TallGrass);
    chunk.set(4, 64, 5, BlockId::Fern);
    let wet = Climate {
        temperature: 0.5,
        humidity: 0.5,
        biome: Biome::Forest,
    };
    let dry = Climate {
        temperature: 1.0,
        humidity: 0.0,
        biome: Biome::Forest,
    };
    let biomes = BiomeMap::from_cells(std::array::from_fn(|index| {
        match (index / 16, index % 16) {
            (2, 3) => wet,
            (4, 5) => dry,
            _ => wet,
        }
    }));
    let mut rgba = [0, 255, 0, 255].repeat(PALETTE_SIZE * PALETTE_SIZE);
    let red = palette_index(wet.temperature, wet.humidity) * 4;
    rgba[red..red + 4].copy_from_slice(&[255, 0, 0, 255]);
    let blue = palette_index(dry.temperature, dry.humidity) * 4;
    rgba[blue..blue + 4].copy_from_slice(&[0, 0, 255, 255]);
    let meshes = mesh_chunk_with_biomes(
        &chunk,
        &ChunkNeighbors::default(),
        &Skylight::from_chunk(&chunk),
        &biomes,
        &GrassColors::from_rgba(rgba),
        &FoliageColors::default(),
        true,
        true,
        false,
        0,
        ChunkPos::ZERO,
    );
    let bevy::mesh::VertexAttributeValues::Float32x4(colors) = meshes
        .masked
        .attribute(Mesh::ATTRIBUTE_COLOR)
        .expect("crossed plants should have vertex colors")
    else {
        panic!("plant colors should be float RGBA values");
    };
    assert_eq!(colors.len(), 16);
    assert!(
        colors[..8]
            .iter()
            .all(|color| *color == [1.0, 0.0, 0.0, 1.0])
    );
    assert!(
        colors[8..]
            .iter()
            .all(|color| *color == [0.0, 0.0, 1.0, 1.0])
    );
}

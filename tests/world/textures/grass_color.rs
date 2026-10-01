use game::block::id::Id;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::lighting::Skylight;
use game::world::meshing::BlockLighting;
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
    chunk.set(2, 64, 3, Id::TallGrass);
    chunk.set(4, 64, 5, Id::Fern);
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
        false,
        ChunkPosition::ZERO,
    );
    let colors = meshes.masked.colors(BlockLighting::default());
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

#[test]
fn greedy_grass_tops_merge_only_when_the_quantized_tint_matches() {
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
    let mut rgba = [0, 255, 0, 255].repeat(PALETTE_SIZE * PALETTE_SIZE);
    let red = palette_index(wet.temperature, wet.humidity) * 4;
    rgba[red..red + 4].copy_from_slice(&[255, 0, 0, 255]);
    let blue = palette_index(dry.temperature, dry.humidity) * 4;
    rgba[blue..blue + 4].copy_from_slice(&[0, 0, 255, 255]);
    let colors = GrassColors::from_rgba(rgba);

    let top_quads = |biomes: &BiomeMap| {
        let mut chunk = Chunk::new();
        chunk.set(2, 2, 4, Id::Grass);
        chunk.set(3, 2, 4, Id::Grass);
        let meshes = mesh_chunk_with_biomes(
            &chunk,
            &ChunkNeighbors::default(),
            &Skylight::from_chunk(&chunk),
            biomes,
            &colors,
            &FoliageColors::default(),
            false,
            ChunkPosition::ZERO,
        );
        meshes
            .opaque
            .vertices()
            .chunks_exact(4)
            .filter(|quad| quad[0].normal[1] > 0.5)
            .map(|quad| {
                (
                    quad[0].tint,
                    quad.iter()
                        .map(|vertex| vertex.position[0])
                        .fold(f32::INFINITY, f32::min),
                    quad.iter()
                        .map(|vertex| vertex.position[0])
                        .fold(f32::NEG_INFINITY, f32::max),
                )
            })
            .collect::<Vec<_>>()
    };

    let split = BiomeMap::from_cells(std::array::from_fn(|index| {
        let x = index / CHUNK_SIZE;
        let z = index % CHUNK_SIZE;
        if x == 3 && z == 4 { dry } else { wet }
    }));
    let split_tops = top_quads(&split);
    assert_eq!(split_tops.len(), 2, "different grass colors stay apart");
    assert!(split_tops.iter().any(|(tint, ..)| *tint == [1.0, 0.0, 0.0]));
    assert!(split_tops.iter().any(|(tint, ..)| *tint == [0.0, 0.0, 1.0]));

    let same = BiomeMap::from_cells([wet; CHUNK_SIZE * CHUNK_SIZE]);
    let merged = top_quads(&same);
    assert_eq!(merged.len(), 1, "one quantized tint shares a quad");
    assert_eq!(merged[0].0, [1.0, 0.0, 0.0]);
    assert!((merged[0].1 - 2.0).abs() < 1e-4);
    assert!((merged[0].2 - 4.0).abs() < 1e-4);
}

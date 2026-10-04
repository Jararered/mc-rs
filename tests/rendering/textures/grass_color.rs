use game::block::blocks::Block;
use game::rendering::meshing::BlockLighting;
use game::rendering::meshing::ChunkNeighbors;
use game::rendering::meshing::mesh_chunk_with_biomes;
use game::rendering::textures::FoliageColors;
use game::rendering::textures::GrassColors;
use game::rendering::textures::PALETTE_SIZE;
use game::rendering::textures::palette_index;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::lighting::Skylight;

/// Any climate will do where a test is about tiles rather than tints.
const TEST_CLIMATE: Climate = Climate {
    temperature: 0.5,
    humidity: 0.5,
    biome: Biome::Forest,
};

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
    chunk.set(2, 64, 3, Block::TallGrass);
    chunk.set_with_metadata(4, 64, 5, Block::TallGrass, 2);
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
        chunk.set(2, 2, 4, Block::Grass);
        chunk.set(3, 2, 4, Block::Grass);
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

#[test]
fn grass_side_under_snow_draws_the_snowcapped_tile_and_drops_the_overlay() {
    let sides_and_overlays = |cover: Option<Block>| {
        let mut chunk = Chunk::new();
        chunk.set(2, 2, 4, Block::Grass);
        if let Some(cover) = cover {
            chunk.set(2, 3, 4, cover);
        }
        let meshes = mesh_chunk_with_biomes(
            &chunk,
            &ChunkNeighbors::default(),
            &Skylight::from_chunk(&chunk),
            &BiomeMap::from_cells([TEST_CLIMATE; CHUNK_SIZE * CHUNK_SIZE]),
            &GrassColors::default(),
            &FoliageColors::default(),
            true,
            ChunkPosition::ZERO,
        );
        let sides = meshes
            .opaque
            .vertices()
            .chunks_exact(4)
            // The cover sits at y = 3, so only the grass row's sides qualify.
            .filter(|quad| quad[0].normal[1].abs() < 0.5 && quad[0].position[1] < 2.5)
            .map(|quad| quad[0].texel.tile)
            .collect::<Vec<_>>();
        (
            sides,
            meshes.grass_overlay.vertices().chunks_exact(4).count(),
        )
    };

    let (bare, bare_overlays) = sides_and_overlays(None);
    assert_eq!(bare.len(), 4, "a lone grass block shows four sides");
    assert!(bare.iter().all(|tile| *tile == [3, 0]));
    assert_eq!(bare_overlays, 4, "fancy grass still draws its side overlay");

    // `BlockGrass.getBlockTexture` swaps the side tile for 68 under either
    // `Material.snow` or `Material.builtSnow`, and `RenderBlocks` then skips
    // the overlay because the tile is no longer 3.
    for cover in [Block::SnowLayer, Block::Snow] {
        let (covered, overlays) = sides_and_overlays(Some(cover));
        assert_eq!(covered.len(), 4);
        assert!(
            covered.iter().all(|tile| *tile == [4, 4]),
            "snow cover {cover:?} redraws the sides"
        );
        assert_eq!(overlays, 0, "snow cover {cover:?} drops the overlay");
    }
}

#[test]
fn greedy_grass_sides_do_not_merge_across_a_snow_cover_boundary() {
    let mut chunk = Chunk::new();
    chunk.set(2, 2, 4, Block::Grass);
    chunk.set(3, 2, 4, Block::Grass);
    chunk.set(3, 3, 4, Block::Snow);
    let meshes = mesh_chunk_with_biomes(
        &chunk,
        &ChunkNeighbors::default(),
        &Skylight::from_chunk(&chunk),
        &BiomeMap::from_cells([TEST_CLIMATE; CHUNK_SIZE * CHUNK_SIZE]),
        &GrassColors::default(),
        &FoliageColors::default(),
        true,
        ChunkPosition::ZERO,
    );
    let north_sides = meshes
        .opaque
        .vertices()
        .chunks_exact(4)
        .filter(|quad| quad[0].normal[2] > 0.5 && quad[0].position[1] < 2.5)
        .map(|quad| {
            (
                quad[0].texel.tile,
                quad.iter()
                    .map(|vertex| vertex.position[0])
                    .fold(f32::INFINITY, f32::min),
                quad.iter()
                    .map(|vertex| vertex.position[0])
                    .fold(f32::NEG_INFINITY, f32::max),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        north_sides.len(),
        2,
        "a covered side must not merge into a bare neighbour's rectangle"
    );
    assert!(north_sides.iter().any(|(tile, low, high)| {
        *tile == [4, 4] && (*low - 3.0).abs() < 1e-4 && (*high - 4.0).abs() < 1e-4
    }));
    assert!(north_sides.iter().any(|(tile, low, high)| {
        *tile == [3, 0] && (*low - 2.0).abs() < 1e-4 && (*high - 3.0).abs() < 1e-4
    }));
}

#[test]
fn fancy_grass_overlays_share_the_base_faces_packed_triangles() {
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
    let blue = palette_index(dry.temperature, dry.humidity) * 4;
    rgba[blue..blue + 4].copy_from_slice(&[0, 0, 255, 255]);
    let colors = GrassColors::from_rgba(rgba);

    for split_colors in [false, true] {
        let biomes = BiomeMap::from_cells(std::array::from_fn(|index| {
            let x = index / CHUNK_SIZE;
            let z = index % CHUNK_SIZE;
            if split_colors && (x >= 4 || z >= 4) {
                dry
            } else {
                wet
            }
        }));
        for torch in [false, true] {
            let mut chunk = Chunk::new();
            for x in 1..7 {
                for z in 1..7 {
                    chunk.set(x, 2, z, Block::Grass);
                }
            }
            if torch {
                chunk.set(3, 2, 0, Block::Torch);
            }
            let meshes = mesh_chunk_with_biomes(
                &chunk,
                &ChunkNeighbors::default(),
                &Skylight::from_chunk(&chunk),
                &biomes,
                &colors,
                &FoliageColors::default(),
                true,
                ChunkPosition::ZERO,
            );
            let base = meshes
                .opaque
                .vertices()
                .chunks_exact(4)
                .filter(|quad| quad[0].texel.tile == [3, 0])
                .collect::<Vec<_>>();
            let overlays = meshes
                .grass_overlay
                .vertices()
                .chunks_exact(4)
                .filter(|quad| quad[0].texel.tile == [6, 2])
                .collect::<Vec<_>>();
            assert_eq!(base.len(), overlays.len());
            if !torch {
                assert!(overlays.iter().any(|quad| quad[0].repeat_uv));
            }
            for overlay in overlays {
                let paired = base.iter().find(|quad| {
                    quad.iter()
                        .zip(overlay)
                        .all(|(a, b)| a.position == b.position && a.normal == b.normal)
                });
                let paired = paired.expect("overlay must use the base face's ordered corners");
                for (a, b) in paired.iter().zip(overlay) {
                    let a_packed = game::rendering::meshing::unpack_vertex(a.pack());
                    let b_packed = game::rendering::meshing::unpack_vertex(b.pack());
                    assert_eq!(a_packed.position, b_packed.position);
                    assert_eq!(a_packed.normal, b_packed.normal);
                    assert_eq!(a_packed.repeat_uv, b_packed.repeat_uv);
                    assert_eq!(a.light, b.light);
                    assert_eq!(a.ao, b.ao);
                }
            }
        }
    }
}

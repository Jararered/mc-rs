use bevy::mesh::Mesh;
use bevy::mesh::VertexAttributeValues;

use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::chunk::remesh_chunks_touching;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::generation::WorldGenerator;
use game::world::generation::generate_chunk;
use game::world::lighting::Skylight;
use game::world::lighting::beta_brightness;
use game::world::meshing::mesh_chunk;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::meshing::mesh_chunk_with_settings_and_smooth_lighting;

#[test]
fn generated_chunk_has_solid_ground_and_sunlit_air() {
    let generated = WorldGenerator::new(0).generate(ChunkPos::ZERO);
    let chunk = &generated.chunk;
    let light = Skylight::from_chunk(chunk);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let height = generated.heightmap.get(x, z) as usize;
            assert!(height > 0 && height < CHUNK_HEIGHT);
            assert!(!matches!(
                chunk.get(x, height - 1, z),
                Some(BlockId::Air | BlockId::Water)
            ));
            assert_eq!(chunk.get(x, 0, z), Some(BlockId::Bedrock));
            assert_eq!(light.get(x, height - 1, z), Some(0));
            // Trees can shadow the air directly above the terrain, so only
            // assert sunlight where the column is clear above the surface.
            let clear_above =
                (height..CHUNK_HEIGHT).all(|y| chunk.get(x, y, z) == Some(BlockId::Air));
            if clear_above {
                assert_eq!(light.get(x, height, z), Some(15));
            }
        }
    }

    assert_eq!(chunk.get(CHUNK_SIZE, 0, 0), None);
    assert_eq!(chunk.get(0, CHUNK_HEIGHT, 0), None);
}

#[test]
fn ambient_occlusion_darkens_enclosed_face_corners() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Stone);
    chunk.set(2, 2, 1, BlockId::Stone);
    chunk.set(1, 2, 2, BlockId::Stone);
    chunk.set(2, 2, 2, BlockId::Stone);

    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, false);
    let Some(VertexAttributeValues::Float32x4(colors)) =
        meshes.opaque.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("terrain mesh should have vertex colors");
    };
    let top_colors = &colors[0..4];
    assert!(
        top_colors
            .iter()
            .map(|color| color[0])
            .min_by(f32::total_cmp)
            .zip(
                top_colors
                    .iter()
                    .map(|color| color[0])
                    .max_by(f32::total_cmp)
            )
            .is_some_and(|(min, max)| min < max),
        "occluded top-face corners should not all have the same brightness"
    );
}

#[test]
fn smooth_lighting_toggle_controls_corner_interpolation() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Stone);
    chunk.set(2, 2, 1, BlockId::Stone);
    chunk.set(1, 2, 2, BlockId::Stone);
    chunk.set(2, 2, 2, BlockId::Stone);
    let skylight = Skylight::from_chunk(&chunk);

    let smooth = mesh_chunk_with_settings_and_smooth_lighting(&chunk, &skylight, true, true, false);
    let flat = mesh_chunk_with_settings_and_smooth_lighting(&chunk, &skylight, true, false, false);
    let colors = |mesh: &Mesh| {
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain mesh should have vertex colors");
        };
        colors[0..4].to_vec()
    };
    let smooth_colors = colors(&smooth.opaque);
    let flat_colors = colors(&flat.opaque);
    assert!(smooth_colors.windows(2).any(|pair| pair[0] != pair[1]));
    assert!(flat_colors.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn terrain_generation_is_deterministic_at_a_chunk_position() {
    let position = ChunkPos { x: -2, z: 3 };
    let first = generate_chunk(position);
    let second = generate_chunk(position);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                assert_eq!(first.get(x, y, z), second.get(x, y, z));
            }
        }
    }
}

#[test]
fn mesher_culls_faces_between_adjacent_blocks() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.count_vertices(), 24);
    assert_eq!(mesh.indices().unwrap().len(), 36);

    chunk.set(2, 1, 1, BlockId::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.count_vertices(), 40);
    assert_eq!(mesh.indices().unwrap().len(), 60);
}

#[test]
fn fancy_leaves_keep_internal_faces_and_use_the_cutout_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Leaves);
    chunk.set(2, 1, 1, BlockId::Leaves);
    let skylight = Skylight::from_chunk(&chunk);

    let fast = mesh_chunk_with_settings(&chunk, &skylight, true, false);
    assert_eq!(fast.opaque.count_vertices(), 40);

    let fancy = mesh_chunk_with_settings(&chunk, &skylight, true, true);
    assert_eq!(fancy.opaque.count_vertices(), 0);
    assert_eq!(fancy.cutout.count_vertices(), 48);

    let Some(VertexAttributeValues::Float32x2(uvs)) = fancy.cutout.attribute(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!("chunk mesh should have atlas UVs");
    };
    assert!(
        uvs.iter()
            .all(|uv| (4.0 / 16.0..5.0 / 16.0).contains(&uv[0])),
        "fancy oak leaves should sample terrain.png tile (4, 3)"
    );
}

#[test]
fn grass_mesh_uses_separate_atlas_tiles_for_top_bottom_and_sides() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Grass);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("chunk mesh should have atlas UVs");
    };
    assert_eq!(uvs.len(), 24);
    assert!(uvs[0..4].iter().all(|uv| uv[0] < 1.0 / 16.0));
    assert!(
        uvs[4..8]
            .iter()
            .all(|uv| (2.0 / 16.0..3.0 / 16.0).contains(&uv[0]))
    );
    assert!(
        uvs[8..24]
            .iter()
            .all(|uv| (3.0 / 16.0..4.0 / 16.0).contains(&uv[0]))
    );
}

#[test]
fn fancy_grass_adds_the_transparent_biome_overlay_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Grass);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    let Some(VertexAttributeValues::Float32x2(uvs)) =
        meshes.grass_overlay.attribute(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!("chunk mesh should have atlas UVs");
    };
    assert_eq!(uvs.len(), 16);
    assert!(
        uvs.iter()
            .all(|uv| (6.0 / 16.0..7.0 / 16.0).contains(&uv[0]))
    );
}

#[test]
fn block_face_uvs_stay_inside_the_padded_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("chunk mesh should have atlas UVs");
    };
    // Stone is terrain.png tile (1, 0). Padding keeps UVs off the 1/16 grid
    // lines so neighbouring tiles cannot bleed across a block edge.
    let gutter = 2.0 / 320.0;
    for uv in uvs {
        assert!(
            uv[0] >= 1.0 / 16.0 + gutter - 1e-5 && uv[0] <= 2.0 / 16.0 - gutter + 1e-5,
            "stone U={:?} should stay in the padded tile",
            uv[0]
        );
        assert!(
            uv[1] >= gutter - 1e-5 && uv[1] <= 1.0 / 16.0 - gutter + 1e-5,
            "stone V={:?} should stay in the padded tile",
            uv[1]
        );
    }
}

#[test]
fn water_renders_as_a_transparent_top_face() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Water);
    let skylight = Skylight::from_chunk(&chunk);
    let meshes = mesh_chunk_with_settings(&chunk, &skylight, true, true);
    assert_eq!(meshes.opaque.count_vertices(), 0);
    assert_eq!(meshes.water.count_vertices(), 4);
    assert_eq!(meshes.water.indices().unwrap().len(), 6);

    let Some(VertexAttributeValues::Float32x4(colors)) =
        meshes.water.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("water mesh should have vertex colors");
    };
    assert!(
        colors.iter().all(|color| (color[3] - 0.8).abs() < 0.001),
        "water vertices should be translucent"
    );

    let Some(VertexAttributeValues::Float32x3(positions)) =
        meshes.water.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("water mesh should have positions");
    };
    let surface_y = 1.0 + 1.0 - 2.0 / 16.0;
    assert!(
        positions
            .iter()
            .all(|pos| (pos[1] - surface_y).abs() < 0.001),
        "water surface should sit two texels below the block top"
    );

    chunk.set(1, 2, 1, BlockId::Water);
    let stacked = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    assert_eq!(
        stacked.water.count_vertices(),
        4,
        "only the surface of a water column should be meshed"
    );

    chunk.set(1, 0, 1, BlockId::Stone);
    let with_bed = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    assert_eq!(
        with_bed.opaque.count_vertices(),
        24,
        "water must not hide the lake bed"
    );
}

#[test]
fn skylight_passes_through_water_and_stops_at_stone() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Water);
    chunk.set(1, 0, 1, BlockId::Stone);
    let light = Skylight::from_chunk(&chunk);
    assert_eq!(light.get(1, 2, 1), Some(15));
    assert_eq!(light.get(1, 1, 1), Some(12));
    assert_eq!(light.get(1, 0, 1), Some(0));
}

#[test]
fn skylight_propagates_sideways_under_an_overhang() {
    let mut chunk = Chunk::new();
    for x in 1..=3 {
        chunk.set(x, 3, 1, BlockId::Stone);
    }
    let light = Skylight::from_chunk(&chunk);

    assert!(light.get(2, 2, 1).unwrap() > 0);
}

#[test]
fn chunk_border_light_uses_loaded_neighbor_values() {
    let center = Chunk::new();
    let mut west = Chunk::new();
    west.set(CHUNK_SIZE - 1, 2, 1, BlockId::Stone);

    let isolated = Skylight::from_chunk(&center);
    let connected = Skylight::from_chunk_with_neighbors(&center, Some(&west), None, None, None);
    assert_eq!(isolated.get_extended(-1, 2, 1), 15);
    assert_eq!(connected.get_extended(-1, 2, 1), 0);
}

#[test]
fn chunk_corner_light_uses_loaded_diagonal_neighbor_values() {
    let center = Chunk::new();
    let mut northwest = Chunk::new();
    northwest.set(CHUNK_SIZE - 1, 2, CHUNK_SIZE - 1, BlockId::Stone);

    let isolated = Skylight::from_chunk(&center);
    let connected = Skylight::from_chunk_with_neighbors_and_corners(
        &center,
        None,
        None,
        None,
        None,
        Some(&northwest),
        None,
        None,
        None,
    );

    assert_eq!(isolated.get_extended(-1, 2, -1), 15);
    assert_eq!(connected.get_extended(-1, 2, -1), 0);
}

#[test]
fn beta_brightness_curve_keeps_caves_dark() {
    assert!((beta_brightness(15) - 1.0).abs() < f32::EPSILON);
    assert!((beta_brightness(0) - 0.05).abs() < f32::EPSILON);
    assert!(beta_brightness(4) < 0.3);
}

#[test]
fn block_light_propagates_from_beta_emitters() {
    let mut chunk = Chunk::new();
    chunk.set(2, 2, 2, BlockId::Glowstone);
    let light = Skylight::from_chunk(&chunk);

    assert_eq!(light.block(2, 2, 2), Some(15));
    assert_eq!(light.block(3, 2, 2), Some(14));
    assert_eq!(light.block(4, 2, 2), Some(13));
}

#[test]
fn adjacent_chunk_edges_have_continuous_height() {
    let generator = WorldGenerator::new(0);
    let left = generator.generate(ChunkPos::ZERO);
    let right = generator.generate(ChunkPos { x: 1, z: 0 });
    for z in 0..CHUNK_SIZE {
        let a = left.heightmap.get(CHUNK_SIZE - 1, z);
        let b = right.heightmap.get(0, z);
        assert!(a.abs_diff(b) <= 8, "height seam at z={z}: {a} vs {b}");
    }
}

#[test]
fn climate_matches_the_local_cpp_reference_at_seed_zero() {
    let generated = WorldGenerator::new(0).generate(ChunkPos::ZERO);
    for (x, z, temperature, humidity) in [
        (0, 0, 0.918687, 0.516277),
        (8, 8, 0.930815, 0.604113),
        (15, 15, 0.95448, 0.626576),
    ] {
        let climate = generated.biomes.get(x, z);
        assert!((climate.temperature - temperature).abs() < 0.00001);
        assert!((climate.humidity - humidity).abs() < 0.00001);
        assert_eq!(climate.biome, Biome::Forest);
    }

    let desert = WorldGenerator::new(12345)
        .generate(ChunkPos::ZERO)
        .biomes
        .get(0, 0);
    assert!((desert.temperature - 0.971755).abs() < 0.00001);
    assert_eq!(desert.humidity, 0.0);
    assert_eq!(desert.biome, Biome::Desert);
}

#[test]
fn set_block_updates_the_column_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(3, 10, 4, BlockId::Stone);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPos::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Plains,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            chunk,
        },
    );
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(3, 4), 11);

    assert_eq!(
        chunks.set_block(3, 20, 4, BlockId::Dirt),
        Some(BlockId::Air)
    );
    assert_eq!(chunks.block_at(3, 20, 4), Some(BlockId::Dirt));
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(3, 4), 21);

    assert_eq!(
        chunks.set_block(3, 20, 4, BlockId::Air),
        Some(BlockId::Dirt)
    );
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(3, 4), 11);
}

#[test]
fn remesh_includes_the_neighbour_when_an_edge_block_changes() {
    assert_eq!(remesh_chunks_touching(8, 8), vec![ChunkPos::ZERO]);
    assert_eq!(
        remesh_chunks_touching(0, 8),
        vec![ChunkPos::ZERO, ChunkPos { x: -1, z: 0 }]
    );
    assert_eq!(
        remesh_chunks_touching(15, 0),
        vec![
            ChunkPos::ZERO,
            ChunkPos { x: 1, z: 0 },
            ChunkPos { x: 0, z: -1 }
        ]
    );
}

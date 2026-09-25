use bevy::mesh::Mesh;
use bevy::mesh::VertexAttributeValues;

use game::block::block::BlockId;
use game::block::properties::selection_bounds;
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
use game::world::lighting::combined_light;
use game::world::lighting::light_emission;
use game::world::lighting::light_opacity;
use game::world::meshing::ChunkNeighbors;
use game::world::meshing::mesh_chunk;
use game::world::meshing::mesh_chunk_with_neighbors;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::meshing::mesh_chunk_with_settings_and_smooth_lighting;
use game::world::textures::atlas_tile_uvs;
use game::world::textures::block_tile;

#[test]
fn mesh_snapshot_keeps_old_blocks_after_world_edit() {
    let mut world_chunk = Chunk::new();
    world_chunk.set(1, 2, 3, BlockId::Stone);
    let snapshot = world_chunk.clone();

    world_chunk.set(1, 2, 3, BlockId::Dirt);

    assert_eq!(snapshot.get(1, 2, 3), Some(BlockId::Stone));
    assert_eq!(world_chunk.get(1, 2, 3), Some(BlockId::Dirt));
}

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
fn species_plank_meshes_apply_distinct_vertex_tints() {
    let first_vertex_color = |block| {
        let mut chunk = Chunk::new();
        chunk.set(1, 1, 1, block);
        let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain mesh should have vertex colors");
        };
        colors[0]
    };

    let oak = first_vertex_color(BlockId::WoodenPlanks);
    let spruce = first_vertex_color(BlockId::SprucePlanks);
    let birch = first_vertex_color(BlockId::BirchPlanks);

    assert_ne!(oak, spruce);
    assert_ne!(oak, birch);
    assert_ne!(spruce, birch);
}

#[test]
fn neighboring_block_data_culls_shared_faces_and_darkens_border_corners() {
    let mut center = Chunk::new();
    center.set(CHUNK_SIZE - 1, 1, 1, BlockId::Stone);
    let mut east = Chunk::new();
    east.set(0, 1, 1, BlockId::Stone);
    let light = Skylight::from_chunk(&center);
    let isolated = mesh_chunk(&center, &light);
    let connected = mesh_chunk_with_neighbors(
        &center,
        &ChunkNeighbors {
            east: Some(&east),
            ..Default::default()
        },
        &light,
    );
    assert_eq!(isolated.count_vertices(), 24);
    assert_eq!(
        connected.count_vertices(),
        20,
        "shared face should be culled"
    );

    east.set(0, 1, 1, BlockId::Air);
    east.set(0, 2, 1, BlockId::Stone);
    let connected = mesh_chunk_with_neighbors(
        &center,
        &ChunkNeighbors {
            east: Some(&east),
            ..Default::default()
        },
        &light,
    );
    let colors = |mesh: &Mesh| {
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("terrain mesh should have vertex colors");
        };
        colors[0..4]
            .iter()
            .map(|color| color[0])
            .collect::<Vec<_>>()
    };
    let isolated = colors(&isolated);
    let connected = colors(&connected);
    assert!(
        isolated
            .iter()
            .zip(&connected)
            .any(|(before, after)| after < before),
        "neighboring stone should darken the border AO corner"
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

    let smooth =
        mesh_chunk_with_settings_and_smooth_lighting(&chunk, &skylight, true, true, false, 0);
    let flat =
        mesh_chunk_with_settings_and_smooth_lighting(&chunk, &skylight, true, false, false, 0);
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
fn complete_neighborhood_does_not_invent_light_at_cave_edges() {
    let stone = || Chunk::from_blocks(vec![BlockId::Stone; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE]);
    let northwest = stone();
    let mut north = stone();
    let northeast = stone();
    let west = stone();
    let mut center = stone();
    let mut east = stone();
    let southwest = stone();
    let south = stone();
    let southeast = stone();
    for x in 0..CHUNK_SIZE {
        north.set(x, 64, CHUNK_SIZE - 1, BlockId::Air);
        center.set(x, 64, 8, BlockId::Air);
        east.set(x, 64, 8, BlockId::Air);
    }
    let light = Skylight::from_chunk_with_neighbors_and_corners(
        &center,
        Some(&west),
        Some(&east),
        Some(&north),
        Some(&south),
        Some(&northwest),
        Some(&northeast),
        Some(&southwest),
        Some(&southeast),
    );
    assert_eq!(light.sky(CHUNK_SIZE - 1, 64, 8), Some(0));
    assert_eq!(light.light_at(CHUNK_SIZE as i32, 64, 8, 0), 0);
    for y in 65..CHUNK_HEIGHT {
        center.set(CHUNK_SIZE - 1, y, 8, BlockId::Air);
    }
    let lit = Skylight::from_chunk_with_neighbors_and_corners(
        &center,
        Some(&west),
        Some(&east),
        Some(&north),
        Some(&south),
        Some(&northwest),
        Some(&northeast),
        Some(&southwest),
        Some(&southeast),
    );
    assert_eq!(lit.sky(CHUNK_SIZE - 1, 64, 8), Some(15));
    assert_eq!(lit.light_at(CHUNK_SIZE as i32, 64, 8, 0), 14);
}

#[test]
fn beta_brightness_curve_keeps_caves_dark() {
    assert!((beta_brightness(15) - 1.0).abs() < f32::EPSILON);
    assert!((beta_brightness(0) - 0.05).abs() < f32::EPSILON);
    assert!(beta_brightness(4) < 0.3);
}

#[test]
fn night_dims_sunlight_and_leaves_torches() {
    assert_eq!(combined_light(15, 0, 0), 15);
    assert_eq!(combined_light(15, 0, 11), 4);
    assert_eq!(combined_light(0, 14, 11), 14);
    assert_eq!(combined_light(15, 14, 11), 14);

    let mut chunk = Chunk::new();
    chunk.set(1, 64, 1, BlockId::Stone);
    let light = Skylight::from_chunk(&chunk);
    let day = mesh_chunk_with_settings_and_smooth_lighting(&chunk, &light, true, false, false, 0);
    let night =
        mesh_chunk_with_settings_and_smooth_lighting(&chunk, &light, true, false, false, 11);
    let day_top = top_vertex_brightness(&day.opaque);
    let night_top = top_vertex_brightness(&night.opaque);
    assert!(
        night_top < day_top,
        "open sunlight should darken after dusk, day {day_top} night {night_top}"
    );
    assert!((day_top - beta_brightness(15)).abs() < 1e-4);
    assert!((night_top - beta_brightness(4)).abs() < 1e-4);
}

fn top_vertex_brightness(mesh: &Mesh) -> f32 {
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("terrain mesh should have vertex colors");
    };
    colors[0][0]
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
            items: Vec::new(),
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
fn ladders_transmit_light_and_do_not_raise_the_surface_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(3, 20, 4, BlockId::LadderWest);
    assert_eq!(light_opacity(BlockId::LadderWest), 0);
    assert_eq!(Heightmap::from_chunk(&chunk).get(3, 4), 0);
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
            ChunkPos { x: 0, z: -1 },
            ChunkPos { x: 1, z: -1 },
        ]
    );
}

#[test]
fn torch_emits_level_fifteen_and_lights_neighboring_chunk() {
    let mut west = Chunk::new();
    west.set(CHUNK_SIZE - 1, 40, 8, BlockId::Torch);
    let east = Chunk::new();
    let west_light = Skylight::from_chunk(&west);
    let east_light = Skylight::from_chunk_with_neighbors(&east, Some(&west), None, None, None);

    assert_eq!(light_emission(BlockId::Torch), 15);
    assert_eq!(light_opacity(BlockId::Torch), 0);
    assert_eq!(west_light.block(CHUNK_SIZE - 1, 40, 8), Some(15));
    assert_eq!(west_light.block(CHUNK_SIZE - 2, 40, 8), Some(14));
    assert_eq!(east_light.block(0, 40, 8), Some(14));
    assert_eq!(east_light.block(1, 40, 8), Some(13));
}

#[test]
fn torch_mesh_uses_a_narrow_shape_instead_of_a_cube() {
    let mut chunk = Chunk::new();
    chunk.set(8, 40, 8, BlockId::Torch);
    let light = Skylight::from_chunk(&chunk);
    let meshes = mesh_chunk_with_settings(&chunk, &light, true, false);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        meshes.grass_overlay.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("torch mesh should have vertices");
    };
    assert!(positions.iter().all(|p| p[0] > 8.4 && p[0] < 8.6));
    assert!(positions.iter().all(|p| p[2] > 8.4 && p[2] < 8.6));
    assert!(positions.iter().all(|p| p[1] >= 40.0 && p[1] <= 40.625));

    let Some(VertexAttributeValues::Float32x2(uvs)) =
        meshes.grass_overlay.attribute(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!("torch mesh should have atlas UVs");
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(0, 5);
    for side in uvs[4..].chunks_exact(4) {
        let min_u = side.iter().map(|uv| uv[0]).fold(f32::INFINITY, f32::min);
        let max_u = side
            .iter()
            .map(|uv| uv[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_v = side.iter().map(|uv| uv[1]).fold(f32::INFINITY, f32::min);
        let max_v = side
            .iter()
            .map(|uv| uv[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_u - (u0 + 7.0 * (u1 - u0) / 16.0)).abs() < 1e-6);
        assert!((max_u - (u0 + 9.0 * (u1 - u0) / 16.0)).abs() < 1e-6);
        assert!((min_v - (v0 + 6.0 * (v1 - v0) / 16.0)).abs() < 1e-6);
        assert!((max_v - v1).abs() < 1e-6);
    }
}

#[test]
fn ladder_mesh_uses_beta_tile_and_a_wall_plane() {
    let mut chunk = Chunk::new();
    chunk.set(3, 5, 7, BlockId::LadderWest);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, false);
    let positions = match meshes.masked.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
        VertexAttributeValues::Float32x3(values) => values,
        _ => panic!("ladder mesh should have 3D positions"),
    };
    assert_eq!(positions.len(), 4);
    assert!(
        positions
            .iter()
            .all(|position| (position[0] - 3.125).abs() < 1e-6)
    );
    assert_eq!(block_tile(BlockId::LadderWest, 0, false), (3, 5));

    let uvs = match meshes.masked.attribute(Mesh::ATTRIBUTE_UV_0).unwrap() {
        VertexAttributeValues::Float32x2(values) => values,
        _ => panic!("ladder mesh should have UV coordinates"),
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(3, 5);
    assert!(
        uvs.iter()
            .all(|uv| uv[0] >= u0 && uv[0] <= u1 && uv[1] >= v0 && uv[1] <= v1)
    );
}

#[test]
fn wall_torch_rotates_the_floor_post_without_tapering_or_flattening_its_cap() {
    let distance = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    for (block, tilted_axis, sign) in [
        (BlockId::TorchWest, 0, 1.0),
        (BlockId::TorchEast, 0, -1.0),
        (BlockId::TorchNorth, 2, 1.0),
        (BlockId::TorchSouth, 2, -1.0),
    ] {
        let mut chunk = Chunk::new();
        chunk.set(8, 40, 8, block);
        let light = Skylight::from_chunk(&chunk);
        let meshes = mesh_chunk_with_settings(&chunk, &light, true, false);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            meshes.grass_overlay.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("wall torch mesh should have vertices");
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            meshes.grass_overlay.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("wall torch mesh should have normals");
        };
        assert!((distance(positions[0], positions[3]) - 0.125).abs() < 1e-5);
        assert!((distance(positions[4], positions[5]) - 0.625).abs() < 1e-5);
        assert!(normals[0][tilted_axis] * sign > 0.5);
        assert!(normals[0][1] < 0.9);
        assert!(
            positions[0][1] != positions[3][1] || positions[0][1] != positions[1][1],
            "cap must tilt with shaft"
        );
        assert!(positions.iter().all(|point| point[1] > 40.25));
        let (bounds_min, bounds_max) = selection_bounds(block);
        if sign > 0.0 {
            assert!(positions.iter().any(|point| point[tilted_axis] < 8.0));
            assert_eq!(bounds_min[tilted_axis], 0.0);
        } else {
            assert!(positions.iter().any(|point| point[tilted_axis] > 9.0));
            assert_eq!(bounds_max[tilted_axis], 1.0);
        }
    }
}

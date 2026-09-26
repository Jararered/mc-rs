use game::block::id::Id;
use game::block::properties::selection_bounds;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
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
use game::world::meshing::BlockGeometry;
use game::world::meshing::BlockLighting;
use game::world::meshing::ChunkNeighbors;
use game::world::meshing::WATER_ALPHA;
use game::world::meshing::mesh_chunk;
use game::world::meshing::mesh_chunk_with_neighbors;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::textures::atlas_tile_uvs;
use game::world::textures::block_tile;

#[test]
fn mesh_snapshot_keeps_old_blocks_after_world_edit() {
    let mut world_chunk = Chunk::new();
    world_chunk.set(1, 2, 3, Id::Stone);
    let snapshot = world_chunk.clone();

    world_chunk.set(1, 2, 3, Id::Dirt);

    assert_eq!(snapshot.get(1, 2, 3), Some(Id::Stone));
    assert_eq!(world_chunk.get(1, 2, 3), Some(Id::Dirt));
}

#[test]
fn generated_chunk_has_solid_ground_and_sunlit_air() {
    let generated = WorldGenerator::new(0).generate(ChunkPosition::ZERO);
    let chunk = &generated.chunk;
    let light = Skylight::from_chunk(chunk);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let height = generated.heightmap.get(x, z) as usize;
            assert!(height > 0 && height < CHUNK_HEIGHT);
            assert!(!matches!(
                chunk.get(x, height - 1, z),
                Some(Id::Air | Id::Water)
            ));
            assert_eq!(chunk.get(x, 0, z), Some(Id::Bedrock));
            assert_eq!(light.get(x, height - 1, z), Some(0));
            // Trees can shadow the air directly above the terrain, so only
            // assert sunlight where the column is clear above the surface.
            let clear_above = (height..CHUNK_HEIGHT).all(|y| chunk.get(x, y, z) == Some(Id::Air));
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
    chunk.set(1, 1, 1, Id::Stone);
    chunk.set(2, 2, 1, Id::Stone);
    chunk.set(1, 2, 2, Id::Stone);
    chunk.set(2, 2, 2, Id::Stone);

    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), false);
    let colors = meshes.opaque.colors(BlockLighting::default());
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
        mesh.colors(BlockLighting::default())[0]
    };

    let oak = first_vertex_color(Id::WoodenPlanks);
    let spruce = first_vertex_color(Id::SprucePlanks);
    let birch = first_vertex_color(Id::BirchPlanks);

    assert_ne!(oak, spruce);
    assert_ne!(oak, birch);
    assert_ne!(spruce, birch);
}

#[test]
fn neighboring_block_data_culls_shared_faces_and_darkens_border_corners() {
    let mut center = Chunk::new();
    center.set(CHUNK_SIZE - 1, 1, 1, Id::Stone);
    let mut east = Chunk::new();
    east.set(0, 1, 1, Id::Stone);
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
    assert_eq!(isolated.vertex_count(), 24);
    assert_eq!(connected.vertex_count(), 20, "shared face should be culled");

    east.set(0, 1, 1, Id::Air);
    east.set(0, 2, 1, Id::Stone);
    let connected = mesh_chunk_with_neighbors(
        &center,
        &ChunkNeighbors {
            east: Some(&east),
            ..Default::default()
        },
        &light,
    );
    let colors = |mesh: &BlockGeometry| {
        mesh.colors(BlockLighting::default())[0..4]
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
    chunk.set(1, 1, 1, Id::Stone);
    chunk.set(2, 2, 1, Id::Stone);
    chunk.set(1, 2, 2, Id::Stone);
    chunk.set(2, 2, 2, Id::Stone);
    let skylight = Skylight::from_chunk(&chunk);

    // Smooth lighting is a shader setting, so one mesh serves both modes.
    let meshes = mesh_chunk_with_settings(&chunk, &skylight, false);
    let colors = |smooth_lighting| {
        meshes.opaque.colors(BlockLighting {
            smooth_lighting,
            ..BlockLighting::default()
        })[0..4]
            .to_vec()
    };
    let smooth_colors = colors(true);
    let flat_colors = colors(false);
    assert!(smooth_colors.windows(2).any(|pair| pair[0] != pair[1]));
    assert!(flat_colors.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn terrain_generation_is_deterministic_at_a_chunk_position() {
    let position = ChunkPosition { x: -2, z: 3 };
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
fn world_floor_undersides_are_not_meshed() {
    let mut chunk = Chunk::new();
    chunk.set(1, 0, 1, Id::Bedrock);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.vertex_count(), 20);
    assert!(
        mesh.normals().iter().all(|normal| normal[1] > -0.5),
        "no face may point below the world"
    );

    chunk.set(1, 0, 1, Id::Air);
    chunk.set(1, 1, 1, Id::Bedrock);
    let raised = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(
        raised.vertex_count(),
        24,
        "a floating block keeps its bottom"
    );
}

#[test]
fn mesher_culls_faces_between_adjacent_blocks() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Id::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.index_count(), 36);

    chunk.set(2, 1, 1, Id::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.vertex_count(), 40);
    assert_eq!(mesh.index_count(), 60);
}

#[test]
fn fancy_leaves_keep_internal_faces_and_use_the_cutout_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Id::Leaves);
    chunk.set(2, 1, 1, Id::Leaves);
    let skylight = Skylight::from_chunk(&chunk);

    let fast = mesh_chunk_with_settings(&chunk, &skylight, false);
    assert_eq!(fast.opaque.vertex_count(), 40);

    let fancy = mesh_chunk_with_settings(&chunk, &skylight, true);
    assert_eq!(fancy.opaque.vertex_count(), 0);
    assert_eq!(fancy.cutout.vertex_count(), 48);

    let uvs = fancy.cutout.uvs();
    assert!(
        uvs.iter()
            .all(|uv| (4.0 / 16.0..5.0 / 16.0).contains(&uv[0])),
        "fancy oak leaves should sample terrain.png tile (4, 3)"
    );
}

#[test]
fn grass_mesh_uses_separate_atlas_tiles_for_top_bottom_and_sides() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Id::Grass);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let uvs = mesh.uvs();
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
    chunk.set(1, 1, 1, Id::Grass);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let uvs = meshes.grass_overlay.uvs();
    assert_eq!(uvs.len(), 16);
    assert!(
        uvs.iter()
            .all(|uv| (6.0 / 16.0..7.0 / 16.0).contains(&uv[0]))
    );
}

#[test]
fn block_face_uvs_stay_inside_the_padded_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Id::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let uvs = mesh.uvs();
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
    chunk.set(1, 1, 1, Id::Water);
    let skylight = Skylight::from_chunk(&chunk);
    let meshes = mesh_chunk_with_settings(&chunk, &skylight, true);
    assert_eq!(meshes.opaque.vertex_count(), 0);
    assert_eq!(meshes.water.vertex_count(), 4);
    assert_eq!(meshes.water.index_count(), 6);

    // Translucency is the water material's base alpha, not a vertex value.
    assert!((WATER_ALPHA - 0.8).abs() < 0.001);
    assert!(
        meshes
            .water
            .vertices()
            .iter()
            .all(|vertex| vertex.tint == [0.4, 0.6, 0.95]),
        "water keeps Beta's blue tint"
    );

    let positions = meshes.water.positions();
    let surface_y = 1.0 + 1.0 - 2.0 / 16.0;
    assert!(
        positions
            .iter()
            .all(|pos| (pos[1] - surface_y).abs() < 0.001),
        "water surface should sit two texels below the block top"
    );

    chunk.set(1, 2, 1, Id::Water);
    let stacked = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    assert_eq!(
        stacked.water.vertex_count(),
        4,
        "only the surface of a water column should be meshed"
    );

    chunk.set(1, 0, 1, Id::Stone);
    let with_bed = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    // Five faces: water must not hide the bed's top, and the bed sits on the
    // world floor, whose underside is never drawn.
    assert_eq!(
        with_bed.opaque.vertex_count(),
        20,
        "water must not hide the lake bed"
    );
}

#[test]
fn skylight_passes_through_water_and_stops_at_stone() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Id::Water);
    chunk.set(1, 0, 1, Id::Stone);
    let light = Skylight::from_chunk(&chunk);
    assert_eq!(light.get(1, 2, 1), Some(15));
    assert_eq!(light.get(1, 1, 1), Some(12));
    assert_eq!(light.get(1, 0, 1), Some(0));
}

#[test]
fn skylight_propagates_sideways_under_an_overhang() {
    let mut chunk = Chunk::new();
    for x in 1..=3 {
        chunk.set(x, 3, 1, Id::Stone);
    }
    let light = Skylight::from_chunk(&chunk);

    assert!(light.get(2, 2, 1).unwrap() > 0);
}

#[test]
fn chunk_border_light_uses_loaded_neighbor_values() {
    let center = Chunk::new();
    let mut west = Chunk::new();
    west.set(CHUNK_SIZE - 1, 2, 1, Id::Stone);

    let isolated = Skylight::from_chunk(&center);
    let connected = Skylight::from_chunk_with_neighbors(&center, Some(&west), None, None, None);
    assert_eq!(isolated.get_extended(-1, 2, 1), 15);
    assert_eq!(connected.get_extended(-1, 2, 1), 0);
}

#[test]
fn chunk_corner_light_uses_loaded_diagonal_neighbor_values() {
    let center = Chunk::new();
    let mut northwest = Chunk::new();
    northwest.set(CHUNK_SIZE - 1, 2, CHUNK_SIZE - 1, Id::Stone);

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
    let stone = || Chunk::from_blocks(vec![Id::Stone; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE]);
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
        north.set(x, 64, CHUNK_SIZE - 1, Id::Air);
        center.set(x, 64, 8, Id::Air);
        east.set(x, 64, 8, Id::Air);
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
        center.set(CHUNK_SIZE - 1, y, 8, Id::Air);
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
    chunk.set(1, 64, 1, Id::Stone);
    let light = Skylight::from_chunk(&chunk);
    // Dusk changes a material uniform; the same mesh serves day and night.
    let meshes = mesh_chunk_with_settings(&chunk, &light, false);
    let flat_at = |skylight_subtracted| BlockLighting {
        old_lighting: true,
        smooth_lighting: false,
        skylight_subtracted,
    };
    let day_top = top_vertex_brightness(&meshes.opaque, flat_at(0));
    let night_top = top_vertex_brightness(&meshes.opaque, flat_at(11));
    assert!(
        night_top < day_top,
        "open sunlight should darken after dusk, day {day_top} night {night_top}"
    );
    assert!((day_top - beta_brightness(15)).abs() < 1e-4);
    assert!((night_top - beta_brightness(4)).abs() < 1e-4);
}

fn top_vertex_brightness(mesh: &BlockGeometry, lighting: BlockLighting) -> f32 {
    mesh.colors(lighting)[0][0]
}

#[test]
fn block_light_propagates_from_beta_emitters() {
    let mut chunk = Chunk::new();
    chunk.set(2, 2, 2, Id::Glowstone);
    let light = Skylight::from_chunk(&chunk);

    assert_eq!(light.block(2, 2, 2), Some(15));
    assert_eq!(light.block(3, 2, 2), Some(14));
    assert_eq!(light.block(4, 2, 2), Some(13));
}

#[test]
fn adjacent_chunk_edges_have_continuous_height() {
    let generator = WorldGenerator::new(0);
    let left = generator.generate(ChunkPosition::ZERO);
    let right = generator.generate(ChunkPosition { x: 1, z: 0 });
    for z in 0..CHUNK_SIZE {
        let a = left.heightmap.get(CHUNK_SIZE - 1, z);
        let b = right.heightmap.get(0, z);
        assert!(a.abs_diff(b) <= 8, "height seam at z={z}: {a} vs {b}");
    }
}

#[test]
fn climate_matches_the_local_cpp_reference_at_seed_zero() {
    let generated = WorldGenerator::new(0).generate(ChunkPosition::ZERO);
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
        .generate(ChunkPosition::ZERO)
        .biomes
        .get(0, 0);
    assert!((desert.temperature - 0.971755).abs() < 0.00001);
    assert_eq!(desert.humidity, 0.0);
    assert_eq!(desert.biome, Biome::Desert);
}

#[test]
fn set_block_updates_the_column_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(3, 10, 4, Id::Stone);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
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
            populated: true,
        },
    );
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(3, 4),
        11
    );

    assert_eq!(chunks.set_block(3, 20, 4, Id::Dirt), Some(Id::Air));
    assert_eq!(chunks.block_at(3, 20, 4), Some(Id::Dirt));
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(3, 4),
        21
    );

    assert_eq!(chunks.set_block(3, 20, 4, Id::Air), Some(Id::Dirt));
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(3, 4),
        11
    );
}

#[test]
fn ladders_transmit_light_and_do_not_raise_the_surface_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(3, 20, 4, Id::LadderWest);
    assert_eq!(light_opacity(Id::LadderWest), 0);
    assert_eq!(Heightmap::from_chunk(&chunk).get(3, 4), 0);
}

#[test]
fn remesh_includes_the_neighbour_when_an_edge_block_changes() {
    assert_eq!(remesh_chunks_touching(8, 8), vec![ChunkPosition::ZERO]);
    assert_eq!(
        remesh_chunks_touching(0, 8),
        vec![ChunkPosition::ZERO, ChunkPosition { x: -1, z: 0 }]
    );
    assert_eq!(
        remesh_chunks_touching(15, 0),
        vec![
            ChunkPosition::ZERO,
            ChunkPosition { x: 1, z: 0 },
            ChunkPosition { x: 0, z: -1 },
            ChunkPosition { x: 1, z: -1 },
        ]
    );
}

#[test]
fn torch_emits_level_fifteen_and_lights_neighboring_chunk() {
    let mut west = Chunk::new();
    west.set(CHUNK_SIZE - 1, 40, 8, Id::Torch);
    let east = Chunk::new();
    let west_light = Skylight::from_chunk(&west);
    let east_light = Skylight::from_chunk_with_neighbors(&east, Some(&west), None, None, None);

    assert_eq!(light_emission(Id::Torch), 15);
    assert_eq!(light_opacity(Id::Torch), 0);
    assert_eq!(west_light.block(CHUNK_SIZE - 1, 40, 8), Some(15));
    assert_eq!(west_light.block(CHUNK_SIZE - 2, 40, 8), Some(14));
    assert_eq!(east_light.block(0, 40, 8), Some(14));
    assert_eq!(east_light.block(1, 40, 8), Some(13));
}

#[test]
fn torch_mesh_uses_a_narrow_shape_instead_of_a_cube() {
    let mut chunk = Chunk::new();
    chunk.set(8, 40, 8, Id::Torch);
    let light = Skylight::from_chunk(&chunk);
    let meshes = mesh_chunk_with_settings(&chunk, &light, false);
    let positions = meshes.grass_overlay.positions();
    assert!(positions.iter().all(|p| p[0] > 8.4 && p[0] < 8.6));
    assert!(positions.iter().all(|p| p[2] > 8.4 && p[2] < 8.6));
    assert!(positions.iter().all(|p| p[1] >= 40.0 && p[1] <= 40.625));

    let uvs = meshes.grass_overlay.uvs();
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
    chunk.set(3, 5, 7, Id::LadderWest);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), false);
    let positions = meshes.masked.positions();
    assert_eq!(positions.len(), 4);
    assert!(
        positions
            .iter()
            .all(|position| (position[0] - 3.125).abs() < 1e-6)
    );
    assert_eq!(block_tile(Id::LadderWest, 0, false), (3, 5));

    let uvs = meshes.masked.uvs();
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
        (Id::TorchWest, 0, 1.0),
        (Id::TorchEast, 0, -1.0),
        (Id::TorchNorth, 2, 1.0),
        (Id::TorchSouth, 2, -1.0),
    ] {
        let mut chunk = Chunk::new();
        chunk.set(8, 40, 8, block);
        let light = Skylight::from_chunk(&chunk);
        let meshes = mesh_chunk_with_settings(&chunk, &light, false);
        let positions = meshes.grass_overlay.positions();
        let normals = meshes.grass_overlay.normals();
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

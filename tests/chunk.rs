use game::block::blocks::Block;
use game::block::properties::selection_bounds;
use game::rendering::meshing::BlockGeometry;
use game::rendering::meshing::BlockLighting;
use game::rendering::meshing::BlockVertex;
use game::rendering::meshing::ChunkNeighbors;
use game::rendering::meshing::WATER_ALPHA;
use game::rendering::meshing::mesh_chunk;
use game::rendering::meshing::mesh_chunk_filtered;
use game::rendering::meshing::mesh_chunk_with_neighbors;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::meshing::unpack_vertex;
use game::rendering::textures::atlas_tile_uvs;
use game::rendering::textures::block_tile;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::chunk::remesh_chunks_touching;
use game::world::generation::generate_chunk;
use game::world::generation::overworld::OverworldGenerator;
use game::world::lighting::Skylight;
use game::world::lighting::beta_brightness;
use game::world::lighting::combined_light;
use game::world::lighting::light_emission;
use game::world::lighting::light_opacity;

#[test]
fn mesh_snapshot_keeps_old_blocks_after_world_edit() {
    let mut world_chunk = Chunk::new();
    world_chunk.set(1, 2, 3, Block::Stone);
    let snapshot = world_chunk.clone();

    world_chunk.set(1, 2, 3, Block::Dirt);

    assert_eq!(snapshot.get(1, 2, 3), Some(Block::Stone));
    assert_eq!(world_chunk.get(1, 2, 3), Some(Block::Dirt));
}

#[test]
fn generated_chunk_has_solid_ground_and_sunlit_air() {
    let generated = OverworldGenerator::new(0).generate(ChunkPosition::ZERO);
    let chunk = &generated.chunk;
    let light = Skylight::from_chunk(chunk);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let height = generated.heightmap.get(x, z) as usize;
            assert!(height > 0 && height < CHUNK_HEIGHT);
            assert!(!matches!(
                chunk.get(x, height - 1, z),
                Some(Block::Air | Block::Water)
            ));
            assert_eq!(chunk.get(x, 0, z), Some(Block::Bedrock));
            assert_eq!(light.get(x, height - 1, z), Some(0));
            // Trees can shadow the air directly above the terrain, so only
            // assert sunlight where the column is clear above the surface.
            let clear_above =
                (height..CHUNK_HEIGHT).all(|y| chunk.get(x, y, z) == Some(Block::Air));
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
    chunk.set(1, 1, 1, Block::Stone);
    chunk.set(2, 2, 1, Block::Stone);
    chunk.set(1, 2, 2, Block::Stone);
    chunk.set(2, 2, 2, Block::Stone);

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

    let oak = first_vertex_color(Block::WoodenPlanks);
    let spruce = first_vertex_color(Block::SprucePlanks);
    let birch = first_vertex_color(Block::BirchPlanks);

    assert_ne!(oak, spruce);
    assert_ne!(oak, birch);
    assert_ne!(spruce, birch);
}

#[test]
fn neighboring_block_data_culls_shared_faces_and_darkens_border_corners() {
    let mut center = Chunk::new();
    center.set(CHUNK_SIZE - 1, 1, 1, Block::Stone);
    let mut east = Chunk::new();
    east.set(0, 1, 1, Block::Stone);
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

    east.set(0, 1, 1, Block::Air);
    east.set(0, 2, 1, Block::Stone);
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
    chunk.set(1, 1, 1, Block::Stone);
    chunk.set(2, 2, 1, Block::Stone);
    chunk.set(1, 2, 2, Block::Stone);
    chunk.set(2, 2, 2, Block::Stone);
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
    chunk.set(1, 0, 1, Block::Bedrock);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.vertex_count(), 20);
    assert!(
        mesh.normals().iter().all(|normal| normal[1] > -0.5),
        "no face may point below the world"
    );

    chunk.set(1, 0, 1, Block::Air);
    chunk.set(1, 1, 1, Block::Bedrock);
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
    chunk.set(1, 1, 1, Block::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.index_count(), 36);

    chunk.set(2, 1, 1, Block::Stone);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    // Tops and the open north/south walls merge. The shared face is culled.
    // Each underside stays its own quad: the cell under a block is darker than
    // the open cells beside it, so the four corners do not match.
    assert_eq!(mesh.vertex_count(), 28);
    assert_eq!(mesh.index_count(), 42);
    let tops = quads_facing(&mesh, |normal| normal[1] > 0.5);
    assert_eq!(tops.len(), 1);
    assert!(tops[0].iter().all(|vertex| vertex.repeat_uv));
    assert!(spans(tops[0], 0, 1.0, 3.0));
}

#[test]
fn fancy_leaves_keep_internal_faces_and_use_the_cutout_tile() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Block::Leaves);
    chunk.set(2, 1, 1, Block::Leaves);
    let skylight = Skylight::from_chunk(&chunk);

    let fast = mesh_chunk_with_settings(&chunk, &skylight, false);
    assert_eq!(fast.opaque.vertex_count(), 28);

    let fancy = mesh_chunk_with_settings(&chunk, &skylight, true);
    assert_eq!(fancy.opaque.vertex_count(), 0);
    // The pair's outside faces merge like stone, and the two leaf-to-leaf
    // faces stay, so this is eight vertices more than the fast mesh.
    assert_eq!(fancy.cutout.vertex_count(), 36);

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
    chunk.set(1, 1, 1, Block::Grass);
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
    chunk.set(1, 1, 1, Block::Grass);
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
    chunk.set(1, 1, 1, Block::Stone);
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
fn water_in_a_lake_renders_only_its_surface_at_beta_height() {
    let mut chunk = Chunk::new();
    for x in 0..5 {
        for z in 0..5 {
            chunk.set(x, 0, z, Block::Stone);
            chunk.set(x, 1, z, Block::Water);
        }
    }
    let skylight = Skylight::from_chunk(&chunk);
    let meshes = mesh_chunk_with_settings(&chunk, &skylight, true);

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

    // Interior cells face only other water and the lake bed, so the middle
    // column contributes nothing but its top.
    let positions = meshes.water.positions();
    let center_top: Vec<_> = positions
        .chunks_exact(4)
        .filter(|quad| {
            quad.iter().all(|position| {
                (2.0..=3.0).contains(&position[0]) && (2.0..=3.0).contains(&position[2])
            }) && quad.iter().all(|position| position[1] > 1.5)
        })
        .collect();
    assert_eq!(
        center_top.len(),
        1,
        "one surface quad above the lake's middle"
    );
    // `RenderBlocks` averages the four sources around each corner, leaving
    // the surface a ninth of a block below the top.
    let surface_y = 1.0 + 8.0 / 9.0;
    assert!(
        center_top[0]
            .iter()
            .all(|position| (position[1] - surface_y).abs() < 0.01),
        "surrounded source water sits at Beta's 8/9 height: {center_top:?}"
    );

    // The lake bed stays visible under the water.
    assert!(
        meshes.opaque.vertex_count() > 0,
        "water must not hide the lake bed"
    );
}

#[test]
fn isolated_water_renders_every_open_face_below_its_neighbors() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Block::Water);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    assert_eq!(meshes.opaque.vertex_count(), 0);
    // Top, bottom, and four sides: nothing covers a lone source.
    assert_eq!(meshes.water.vertex_count(), 24);

    // Each corner averages the source (weight 11) with three open cells
    // (weight 1 each).
    let corner = 1.0 - (11.0 / 9.0 + 3.0) / 14.0;
    let top = meshes
        .water
        .positions()
        .iter()
        .map(|position| position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((top - (1.0 + corner)).abs() < 0.01, "top {top}");

    // With water above, the lower block fills its cell and draws no top.
    chunk.set(1, 2, 1, Block::Water);
    let stacked = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let heights: Vec<_> = stacked
        .water
        .positions()
        .iter()
        .map(|position| position[1])
        .collect();
    assert!(
        heights.iter().all(|&y| y <= 2.0 + corner + 0.01),
        "only the upper block's surface sits below its top"
    );
    assert!(
        heights.iter().any(|&y| (y - 2.0).abs() < 0.01),
        "the lower block's sides reach its full height"
    );
    assert_eq!(
        stacked.water.vertex_count(),
        40,
        "two blocks minus the shared faces"
    );
}

#[test]
fn flowing_water_slopes_toward_its_lower_levels_and_uses_the_flow_tile() {
    let mut chunk = Chunk::new();
    for x in 0..4 {
        chunk.set(x, 0, 1, Block::Stone);
    }
    chunk.set_with_metadata(0, 1, 1, Block::Water, 0);
    chunk.set_with_metadata(1, 1, 1, Block::FlowingWater, 1);
    chunk.set_with_metadata(2, 1, 1, Block::FlowingWater, 2);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let positions = meshes.water.positions();
    let uvs = meshes.water.uvs();
    let top_of = |x: f32| {
        positions
            .iter()
            .filter(|position| (position[0] - x).abs() < 0.01 && position[1] > 1.0)
            .map(|position| position[1])
            .fold(f32::NEG_INFINITY, f32::max)
    };
    assert!(
        top_of(1.0) > top_of(2.0),
        "the surface falls with the level"
    );
    assert!(top_of(2.0) > top_of(3.0));

    let (u0, v0, u1, v1) = atlas_tile_uvs(14, 12);
    assert!(
        uvs.iter()
            .any(|uv| (u0..=u1).contains(&uv[0]) && (v0..=v1).contains(&uv[1])),
        "moving water draws the flowing tile"
    );
}

#[test]
fn diagonal_water_uses_the_full_flow_angle_inside_repeated_atlas_tiles() {
    let mut corners = Vec::new();
    for directions in [[(1i32, 0i32), (0, 1)], [(-1, 0), (0, -1)]] {
        let mut chunk = Chunk::new();
        chunk.set_with_metadata(8, 1, 8, Block::FlowingWater, 0);
        for (dx, dz) in directions {
            chunk.set_with_metadata(
                (8 + dx) as usize,
                1,
                (8 + dz) as usize,
                Block::FlowingWater,
                1,
            );
        }
        let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
        let quad = meshes
            .water
            .vertices()
            .chunks_exact(4)
            .find(|quad| {
                quad.iter().all(|vertex| {
                    vertex.normal[1] > 0.99
                        && (8.0..=9.0).contains(&vertex.position[0])
                        && (8.0..=9.0).contains(&vertex.position[2])
                })
            })
            .expect("diagonal current has a top face");
        let (u0, v0, _, _) = atlas_tile_uvs(14, 12);
        let (_, _, u1, v1) = atlas_tile_uvs(15, 13);
        for vertex in quad {
            let texel = vertex.texel;
            assert_eq!(texel, unpack_vertex(vertex.pack()).texel);
            assert_eq!(texel.tile, [14, 12]);
            assert!((5..=27).contains(&texel.texel[0]));
            assert!((5..=27).contains(&texel.texel[1]));
            let [u, v] = texel.uv();
            assert!((u0..=u1).contains(&u) && (v0..=v1).contains(&v));
        }
        corners.push(
            quad.iter()
                .map(|vertex| vertex.texel.texel)
                .collect::<Vec<_>>(),
        );
    }
    // With diagonal flow, opposite corners land near 5 and 27 texels,
    // not at the 8/24 corners produced by a quarter-turn approximation.
    assert!(corners[0].iter().flatten().any(|&texel| texel == 5));
    assert!(corners[0].iter().flatten().any(|&texel| texel == 27));
    assert_ne!(
        corners[0], corners[1],
        "opposite currents rotate differently"
    );
}

#[test]
fn skylight_passes_through_water_and_stops_at_stone() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Block::Water);
    chunk.set(1, 0, 1, Block::Stone);
    let light = Skylight::from_chunk(&chunk);
    assert_eq!(light.get(1, 2, 1), Some(15));
    assert_eq!(light.get(1, 1, 1), Some(12));
    assert_eq!(light.get(1, 0, 1), Some(0));
}

#[test]
fn skylight_propagates_sideways_under_an_overhang() {
    let mut chunk = Chunk::new();
    for x in 1..=3 {
        chunk.set(x, 3, 1, Block::Stone);
    }
    let light = Skylight::from_chunk(&chunk);

    assert!(light.get(2, 2, 1).unwrap() > 0);
}

#[test]
fn chunk_border_light_uses_loaded_neighbor_values() {
    let center = Chunk::new();
    let mut west = Chunk::new();
    west.set(CHUNK_SIZE - 1, 2, 1, Block::Stone);

    let isolated = Skylight::from_chunk(&center);
    let connected = Skylight::from_chunk_with_neighbors(&center, Some(&west), None, None, None);
    assert_eq!(isolated.get_extended(-1, 2, 1), 15);
    assert_eq!(connected.get_extended(-1, 2, 1), 0);
}

#[test]
fn chunk_corner_light_uses_loaded_diagonal_neighbor_values() {
    let center = Chunk::new();
    let mut northwest = Chunk::new();
    northwest.set(CHUNK_SIZE - 1, 2, CHUNK_SIZE - 1, Block::Stone);

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
    let stone = || Chunk::from_blocks(vec![Block::Stone; CHUNK_SIZE * CHUNK_HEIGHT * CHUNK_SIZE]);
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
        north.set(x, 64, CHUNK_SIZE - 1, Block::Air);
        center.set(x, 64, 8, Block::Air);
        east.set(x, 64, 8, Block::Air);
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
        center.set(CHUNK_SIZE - 1, y, 8, Block::Air);
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
    chunk.set(1, 64, 1, Block::Stone);
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
    chunk.set(2, 2, 2, Block::Glowstone);
    let light = Skylight::from_chunk(&chunk);

    assert_eq!(light.block(2, 2, 2), Some(15));
    assert_eq!(light.block(3, 2, 2), Some(14));
    assert_eq!(light.block(4, 2, 2), Some(13));
}

#[test]
fn adjacent_chunk_edges_have_continuous_height() {
    let generator = OverworldGenerator::new(0);
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
    let generated = OverworldGenerator::new(0).generate(ChunkPosition::ZERO);
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

    let desert = OverworldGenerator::new(12345)
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
    chunk.set(3, 10, 4, Block::Stone);
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

    assert_eq!(chunks.set_block(3, 20, 4, Block::Dirt), Some(Block::Air));
    assert_eq!(chunks.block_at(3, 20, 4), Some(Block::Dirt));
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(3, 4),
        21
    );

    assert_eq!(chunks.set_block(3, 20, 4, Block::Air), Some(Block::Dirt));
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(3, 4),
        11
    );
}

#[test]
fn ladders_transmit_light_and_do_not_raise_the_surface_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(3, 20, 4, Block::LadderWest);
    assert_eq!(light_opacity(Block::LadderWest), 0);
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
    west.set(CHUNK_SIZE - 1, 40, 8, Block::Torch);
    let east = Chunk::new();
    let west_light = Skylight::from_chunk(&west);
    let east_light = Skylight::from_chunk_with_neighbors(&east, Some(&west), None, None, None);

    assert_eq!(light_emission(Block::Torch), 15);
    assert_eq!(light_opacity(Block::Torch), 0);
    assert_eq!(west_light.block(CHUNK_SIZE - 1, 40, 8), Some(15));
    assert_eq!(west_light.block(CHUNK_SIZE - 2, 40, 8), Some(14));
    assert_eq!(east_light.block(0, 40, 8), Some(14));
    assert_eq!(east_light.block(1, 40, 8), Some(13));
}

#[test]
fn torch_mesh_uses_a_narrow_shape_instead_of_a_cube() {
    let mut chunk = Chunk::new();
    chunk.set(8, 40, 8, Block::Torch);
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
    chunk.set(3, 5, 7, Block::LadderWest);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), false);
    let positions = meshes.masked.positions();
    assert_eq!(positions.len(), 4);
    assert!(
        positions
            .iter()
            .all(|position| (position[0] - 3.125).abs() < 1e-6)
    );
    assert_eq!(block_tile(Block::LadderWest, 0, false), (3, 5));

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
        (Block::TorchWest, 0, 1.0),
        (Block::TorchEast, 0, -1.0),
        (Block::TorchNorth, 2, 1.0),
        (Block::TorchSouth, 2, -1.0),
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

#[test]
fn greedy_mesh_merges_a_uniform_stone_slab_into_one_top_quad() {
    let mut chunk = Chunk::new();
    for x in 0..4 {
        for z in 0..4 {
            chunk.set(x, 1, z, Block::Stone);
        }
    }
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let tops = quads_facing(&mesh, |normal| normal[1] > 0.5);
    assert_eq!(tops.len(), 1, "a flat sunlit roof is one quad");
    let quad = tops[0];
    assert!(
        quad.iter().all(|vertex| vertex.repeat_uv),
        "a wide quad tiles its atlas tile from the block position"
    );
    let (tile_x, tile_y) = block_tile(Block::Stone, 0, false);
    assert!(
        quad.iter()
            .all(|vertex| vertex.texel.tile == [tile_x, tile_y] && vertex.texel.texel == [0, 0])
    );
    assert!(quad.iter().all(|vertex| {
        vertex.ao == quad[0].ao && vertex.light == quad[0].light && vertex.tint == quad[0].tint
    }));
    assert_eq!(quad[0].ao, 0);
    assert_eq!(quad[0].light[0] >> 4, 15);
    let packed = unpack_vertex(quad[0].pack());
    assert!(packed.repeat_uv);
    assert!(packed.normal[1] > 0.5);
    assert_eq!(packed.texel.tile, [tile_x, tile_y]);
    assert!(spans(quad, 0, 0.0, 4.0));
    assert!(spans(quad, 2, 0.0, 4.0));
    assert!(
        quad.iter()
            .all(|vertex| (vertex.position[1] - 2.0).abs() < 1e-4)
    );
}

#[test]
fn greedy_mesh_splits_a_row_where_torch_light_changes() {
    let fill_row = |chunk: &mut Chunk| {
        for x in 1..5 {
            chunk.set(x, 2, 3, Block::Stone);
        }
    };
    let mut plain = Chunk::new();
    fill_row(&mut plain);
    let plain_mesh = mesh_chunk(&plain, &Skylight::from_chunk(&plain));
    let plain_north = quads_facing(&plain_mesh, |normal| normal[2] < -0.5);
    assert_eq!(plain_north.len(), 1);
    assert!(plain_north[0].iter().all(|vertex| vertex.repeat_uv));
    assert!(spans(plain_north[0], 0, 1.0, 5.0));
    assert!(
        plain_north[0]
            .iter()
            .all(|vertex| (vertex.position[2] - 3.0).abs() < 1e-4)
    );

    let mut lit = Chunk::new();
    fill_row(&mut lit);
    lit.set(1, 2, 2, Block::Torch);
    let lit_mesh = mesh_chunk(&lit, &Skylight::from_chunk(&lit));
    assert!(
        quads_facing(&lit_mesh, |normal| normal[2] < -0.5).len() > 1,
        "a torch's block light changes along the row, so the wall cannot be one quad"
    );
}

#[test]
fn filtered_wireframe_merges_a_lit_wall_into_one_side() {
    let mut chunk = Chunk::new();
    for x in 1..7 {
        for y in 2..6 {
            chunk.set(x, y, 3, Block::Stone);
        }
    }
    chunk.set(3, 2, 2, Block::Torch);
    let light = Skylight::from_chunk(&chunk);
    assert!(
        quads_facing(&mesh_chunk(&chunk, &light), |normal| normal[2] < -0.5).len() > 1,
        "the torch and the height gradient keep the gameplay wall split"
    );

    let filtered = mesh_chunk_filtered(&chunk, &light, false, Some(Block::Stone));
    let north = quads_facing(&filtered.opaque, |normal| normal[2] < -0.5);
    assert_eq!(north.len(), 1, "the filtered wall is one rectangle");
    assert!(spans(north[0], 0, 1.0, 7.0));
    assert!(spans(north[0], 1, 2.0, 6.0));
    assert!(
        north[0]
            .iter()
            .all(|vertex| (vertex.position[2] - 3.0).abs() < 1e-4)
    );
}

#[test]
fn greedy_mesh_merges_a_fancy_grass_side_overlay() {
    let mut chunk = Chunk::new();
    for x in 1..5 {
        chunk.set(x, 2, 3, Block::Grass);
    }
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let north = quads_facing(&meshes.grass_overlay, |normal| normal[2] < -0.5);
    assert_eq!(north.len(), 1);
    assert!(north[0].iter().all(|vertex| vertex.repeat_uv));
    assert!(north[0].iter().all(|vertex| vertex.texel.tile == [6, 2]));
    assert!(
        north[0]
            .iter()
            .all(|vertex| vertex.tint == north[0][0].tint)
    );
    assert!(spans(north[0], 0, 1.0, 5.0));
}

#[test]
fn greedy_mesh_merges_a_chunk_edge_and_culls_it_against_the_neighbor() {
    let mut chunk = Chunk::new();
    for z in 0..4 {
        chunk.set(CHUNK_SIZE - 1, 2, z, Block::Stone);
    }
    let light = Skylight::from_chunk(&chunk);
    let mesh = mesh_chunk(&chunk, &light);
    let east = quads_facing(&mesh, |normal| normal[0] > 0.5);
    assert_eq!(east.len(), 1);
    assert!(east[0].iter().all(|vertex| vertex.repeat_uv));
    assert!(
        east[0]
            .iter()
            .all(|vertex| (vertex.position[0] - CHUNK_SIZE as f32).abs() < 1e-4)
    );
    assert!(spans(east[0], 2, 0.0, 4.0));

    let mut neighbor = Chunk::new();
    for z in 0..4 {
        neighbor.set(0, 2, z, Block::Stone);
    }
    let culled = mesh_chunk_with_neighbors(
        &chunk,
        &ChunkNeighbors {
            east: Some(&neighbor),
            ..Default::default()
        },
        &light,
    );
    assert!(
        quads_facing(&culled, |normal| normal[0] > 0.5).is_empty(),
        "the neighbor occludes the chunk-edge face"
    );
    let tops = quads_facing(&culled, |normal| normal[1] > 0.5);
    assert_eq!(tops.len(), 1, "the row still merges inside its own section");
    assert!(spans(tops[0], 2, 0.0, 4.0));
    assert!(tops[0].iter().all(|vertex| {
        (15.0..=16.0).contains(&vertex.position[0]) && (vertex.position[1] - 3.0).abs() < 1e-4
    }));
}

#[test]
fn wireframe_block_filter_keeps_only_that_blocks_shell() {
    let mut chunk = Chunk::new();
    chunk.set(1, 4, 1, Block::Stone);
    chunk.set(3, 4, 1, Block::Grass);
    chunk.set(5, 4, 1, Block::Water);
    let light = Skylight::from_chunk(&chunk);

    let all = mesh_chunk_filtered(&chunk, &light, true, None);
    assert!(!all.opaque.is_empty());
    assert!(!all.grass_overlay.is_empty());
    assert!(!all.water.is_empty());

    let grass = mesh_chunk_filtered(&chunk, &light, true, Some(Block::Grass));
    assert!(!grass.opaque.is_empty());
    assert!(!grass.grass_overlay.is_empty());
    assert!(grass.water.is_empty());
    assert!(grass.cutout.is_empty());
    assert!(grass.masked.is_empty());

    let water = mesh_chunk_filtered(&chunk, &light, true, Some(Block::Water));
    assert!(water.opaque.is_empty());
    assert!(water.grass_overlay.is_empty());
    assert!(!water.water.is_empty());
    assert!(water.cutout.is_empty());
    assert!(water.masked.is_empty());
}

#[test]
fn filtered_water_merges_a_pool_into_one_top_face() {
    let mut chunk = Chunk::new();
    for x in 2..6 {
        for z in 2..6 {
            chunk.set(x, 2, z, Block::Water);
        }
    }
    let light = Skylight::from_chunk(&chunk);

    let filtered = mesh_chunk_filtered(&chunk, &light, true, Some(Block::Water));
    assert!(filtered.opaque.is_empty());
    assert!(filtered.grass_overlay.is_empty());
    assert!(filtered.cutout.is_empty());
    assert!(filtered.masked.is_empty());
    let tops = quads_facing(&filtered.water, |normal| normal[1] > 0.5);
    assert_eq!(tops.len(), 1, "the pool top is one rectangle");
    assert!(spans(tops[0], 0, 2.0, 6.0));
    assert!(spans(tops[0], 2, 2.0, 6.0));
    assert!(
        tops[0]
            .iter()
            .all(|vertex| (vertex.position[1] - 3.0).abs() < 1e-4)
    );
    let interior_wall = quads_facing(&filtered.water, |normal| {
        normal[0].abs() > 0.5 || normal[2].abs() > 0.5
    })
    .into_iter()
    .any(|quad| {
        let inside = |axis: usize| {
            quad.iter()
                .all(|vertex| (2.01..5.99).contains(&vertex.position[axis]))
        };
        inside(0) || inside(2)
    });
    assert!(
        !interior_wall,
        "faces shared by two water cells are omitted"
    );

    let open = mesh_chunk_filtered(&chunk, &light, true, None);
    assert_eq!(
        quads_facing(&open.water, |normal| normal[1] > 0.5).len(),
        16,
        "unfiltered water stays one quad per cell"
    );
}

#[test]
fn filtered_water_leaves_the_shared_face_on_the_dirt() {
    let mut chunk = Chunk::new();
    for x in 2..6 {
        for z in 2..6 {
            chunk.set(x, 1, z, Block::Dirt);
            chunk.set(x, 2, z, Block::Water);
        }
    }
    let light = Skylight::from_chunk(&chunk);

    let water = mesh_chunk_filtered(&chunk, &light, true, Some(Block::Water));
    let tops = quads_facing(&water.water, |normal| normal[1] > 0.5);
    assert_eq!(tops.len(), 1, "the open water surface is one rectangle");
    assert!(spans(tops[0], 0, 2.0, 6.0));
    assert!(spans(tops[0], 2, 2.0, 6.0));
    assert!(
        water.water.vertices().chunks_exact(4).all(|quad| {
            !quad
                .iter()
                .all(|vertex| (vertex.position[1] - 2.0).abs() < 1e-4)
        }),
        "water does not draw the face it shares with the dirt"
    );

    let dirt = mesh_chunk_filtered(&chunk, &light, true, Some(Block::Dirt));
    let bed_area: f32 = quads_facing(&dirt.opaque, |normal| normal[1] > 0.5)
        .into_iter()
        .filter(|quad| {
            quad.iter()
                .all(|vertex| (vertex.position[1] - 2.0).abs() < 1e-4)
        })
        .map(horizontal_area)
        .sum();
    assert!(
        (bed_area - 16.0).abs() < 0.01,
        "dirt under the water keeps its top, area {bed_area}"
    );
}

fn horizontal_area(quad: &[BlockVertex]) -> f32 {
    let width = extent(quad, 0);
    let depth = extent(quad, 2);
    width * depth
}

fn extent(quad: &[BlockVertex], axis: usize) -> f32 {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for vertex in quad {
        lo = lo.min(vertex.position[axis]);
        hi = hi.max(vertex.position[axis]);
    }
    hi - lo
}

fn quads_facing<'a>(
    mesh: &'a BlockGeometry,
    facing: impl Fn([f32; 3]) -> bool,
) -> Vec<&'a [BlockVertex]> {
    mesh.vertices()
        .chunks_exact(4)
        .filter(|quad| facing(quad[0].normal))
        .collect()
}

fn spans(quad: &[BlockVertex], axis: usize, min: f32, max: f32) -> bool {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for vertex in quad {
        lo = lo.min(vertex.position[axis]);
        hi = hi.max(vertex.position[axis]);
    }
    (lo - min).abs() < 1e-4 && (hi - max).abs() < 1e-4
}

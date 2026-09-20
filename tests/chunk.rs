use bevy::mesh::{Mesh, VertexAttributeValues};

use game::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk, ChunkPos},
    generation::{Biome, WorldGenerator, generate_chunk},
    lighting::Skylight,
    meshing::mesh_chunk,
};

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
            if chunk.get(x, height, z) == Some(BlockId::Air) {
                assert_eq!(light.get(x, height, z), Some(15));
            }
        }
    }

    assert_eq!(chunk.get(CHUNK_SIZE, 0, 0), None);
    assert_eq!(chunk.get(0, CHUNK_HEIGHT, 0), None);
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
fn skylight_passes_through_water_and_stops_at_stone() {
    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Water);
    chunk.set(1, 0, 1, BlockId::Stone);
    let light = Skylight::from_chunk(&chunk);
    assert_eq!(light.get(1, 2, 1), Some(15));
    assert_eq!(light.get(1, 1, 1), Some(14));
    assert_eq!(light.get(1, 0, 1), Some(0));
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

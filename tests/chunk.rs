use game::world::{
    block::block::BlockId,
    chunk::{CHUNK_HEIGHT, CHUNK_SIZE, Chunk, ChunkPos},
    generation::generate_chunk,
    lighting::Skylight,
    meshing::mesh_chunk,
};

#[test]
fn generated_chunk_has_solid_ground_and_sunlit_air() {
    let chunk = generate_chunk(ChunkPos::ZERO);
    let light = Skylight::from_chunk(&chunk);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let surface = (0..CHUNK_HEIGHT)
                .rev()
                .find(|&y| chunk.get(x, y, z) != Some(BlockId::Air))
                .expect("every column should contain ground");

            assert_eq!(chunk.get(x, surface, z), Some(BlockId::Grass));
            assert_eq!(chunk.get(x, surface + 1, z), Some(BlockId::Air));
            assert_eq!(light.get(x, surface + 1, z), Some(15));
            assert_eq!(light.get(x, surface, z), Some(0));
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

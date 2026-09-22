use bevy::mesh::Mesh;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Vec3;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::place_selected_block;
use game::world::block::block::BlockId;
use game::world::block::properties::blocks_movement;
use game::world::block::properties::collision_bounds;
use game::world::block::properties::hardness;
use game::world::block::properties::is_opaque_cube;
use game::world::block::properties::selection_bounds;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::generation::WorldGenerator;
use game::world::lighting::Skylight;
use game::world::lighting::light_opacity;
use game::world::meshing::mesh_chunk;
use game::world::textures::block_tile;

fn world_with(chunk: Chunk) -> WorldChunks {
    let heightmap = Heightmap::from_chunk(&chunk);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPos::ZERO,
        GeneratedChunk {
            chunk,
            heightmap,
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Desert,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            items: Vec::new(),
        },
    );
    chunks
}

#[test]
fn cactus_uses_species_faces_and_an_inset_world_mesh() {
    assert_eq!(block_tile(BlockId::Cactus, 0, false), (5, 4));
    assert_eq!(block_tile(BlockId::Cactus, 1, false), (7, 4));
    assert_eq!(block_tile(BlockId::Cactus, 2, false), (6, 4));

    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, BlockId::Cactus);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("terrain mesh should have vertex positions");
    };
    let minimum = std::array::from_fn::<_, 3, _>(|axis| {
        positions
            .iter()
            .map(|point| point[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let maximum = std::array::from_fn::<_, 3, _>(|axis| {
        positions
            .iter()
            .map(|point| point[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    assert_eq!(minimum, [1.0625, 1.0, 1.0625]);
    assert_eq!(maximum, [1.9375, 2.0, 1.9375]);
}

#[test]
fn cactus_is_nonopaque_but_collides_and_uses_beta_bounds_and_hardness() {
    assert!(!is_opaque_cube(BlockId::Cactus));
    assert!(blocks_movement(BlockId::Cactus));
    assert_eq!(light_opacity(BlockId::Cactus), 0);
    assert_eq!(hardness(BlockId::Cactus), 0.4);
    assert_eq!(
        collision_bounds(BlockId::Cactus),
        Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375]))
    );
    assert_eq!(
        selection_bounds(BlockId::Cactus),
        ([0.0625, 0.0, 0.0625], [0.9375, 1.0, 0.9375])
    );
}

#[test]
fn cactus_placement_requires_sand_support_and_clear_sides() {
    let hit = BlockHit {
        x: 8,
        y: 64,
        z: 8,
        face: BlockFace::Up,
        block: BlockId::Sand,
    };
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));

    let mut supported = Chunk::new();
    supported.set(8, 64, 8, BlockId::Sand);
    let mut chunks = world_with(supported);
    assert!(place_selected_block(
        &mut chunks,
        hit,
        player,
        BlockId::Cactus,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Cactus));

    let mut obstructed = Chunk::new();
    obstructed.set(8, 64, 8, BlockId::Sand);
    obstructed.set(7, 65, 8, BlockId::Stone);
    let mut chunks = world_with(obstructed);
    assert!(!place_selected_block(
        &mut chunks,
        hit,
        player,
        BlockId::Cactus,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Air));
}

#[test]
fn desert_chunks_generate_repeatable_cactus_columns() {
    for seed in 0..128 {
        let generator = WorldGenerator::new(seed);
        let generated = generator.generate(ChunkPos::ZERO);
        let cactus_count = generated
            .chunk
            .blocks()
            .iter()
            .filter(|&&block| block == BlockId::Cactus)
            .count();
        if cactus_count > 0 {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_HEIGHT {
                        if generated.chunk.get(x, y, z) != Some(BlockId::Cactus) {
                            continue;
                        }
                        assert!(matches!(
                            generated.chunk.get(x, y.saturating_sub(1), z),
                            Some(BlockId::Sand | BlockId::Cactus)
                        ));
                        if x > 0 && x < CHUNK_SIZE - 1 && z > 0 && z < CHUNK_SIZE - 1 {
                            assert!(
                                [
                                    generated.chunk.get(x - 1, y, z),
                                    generated.chunk.get(x + 1, y, z),
                                    generated.chunk.get(x, y, z - 1),
                                    generated.chunk.get(x, y, z + 1),
                                ]
                                .into_iter()
                                .flatten()
                                .all(|block| !is_opaque_cube(block))
                            );
                        }
                    }
                }
            }
            let again = WorldGenerator::new(seed).generate(ChunkPos::ZERO);
            assert_eq!(generated.chunk.blocks(), again.chunk.blocks());
            assert!(
                generated
                    .biomes
                    .cells()
                    .iter()
                    .any(|cell| cell.biome == Biome::Desert)
            );
            return;
        }
    }
    panic!("seeded desert chunks should eventually generate cactus");
}

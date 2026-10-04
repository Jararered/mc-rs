use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::item::ItemStack;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::place_selected_block;
use game::rendering::meshing::dropped_block_meshes;
use game::rendering::meshing::mesh_chunk_with_settings;
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
use game::world::generation::overworld::OverworldGenerator;
use game::world::lighting::Skylight;
use game::world::lighting::light_opacity;

fn world_with(chunk: Chunk) -> WorldChunks {
    let heightmap = Heightmap::from_chunk(&chunk);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
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
            populated: true,
        },
    );
    chunks
}

#[test]
fn cactus_uses_species_faces_and_an_inset_world_mesh() {
    assert_eq!(block_tile(Block::Cactus, 0, 0, false), (5, 4));
    assert_eq!(block_tile(Block::Cactus, 0, 1, false), (7, 4));
    assert_eq!(block_tile(Block::Cactus, 0, 2, false), (6, 4));

    let mut chunk = Chunk::new();
    chunk.set(1, 1, 1, Block::Cactus);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), false);
    assert_eq!(meshes.opaque.vertex_count(), 0);
    assert_eq!(meshes.masked.vertex_count(), 24);
    let mesh = &meshes.masked;
    let positions = mesh.positions();
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
    assert_eq!(minimum, [1.0, 1.0, 1.0]);
    assert_eq!(maximum, [2.0, 2.0, 2.0]);
    assert!(positions[..4].iter().all(|point| point[1] == 2.0));
    assert!(positions[..4].iter().any(|point| point[0] == 1.0));
    assert!(positions[..4].iter().any(|point| point[2] == 2.0));
    for point in &positions[8..12] {
        assert_eq!(point[0], 1.9375);
        assert!((1.0..=2.0).contains(&point[1]));
        assert!((1.0..=2.0).contains(&point[2]));
    }
    for point in &positions[12..16] {
        assert_eq!(point[0], 1.0625);
        assert!((1.0..=2.0).contains(&point[1]));
        assert!((1.0..=2.0).contains(&point[2]));
    }
    for point in &positions[16..20] {
        assert_eq!(point[2], 1.9375);
        assert!((1.0..=2.0).contains(&point[0]));
        assert!((1.0..=2.0).contains(&point[1]));
    }
    for point in &positions[20..24] {
        assert_eq!(point[2], 1.0625);
        assert!((1.0..=2.0).contains(&point[0]));
        assert!((1.0..=2.0).contains(&point[1]));
    }
    let uvs = mesh.uvs();
    let (top_u0, _, top_u1, _) = atlas_tile_uvs(5, 4);
    let top_min = uvs[..4]
        .iter()
        .map(|uv| uv[0])
        .fold(f32::INFINITY, f32::min);
    let top_max = uvs[..4]
        .iter()
        .map(|uv| uv[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert_eq!((top_min, top_max), (top_u0, top_u1));

    for face in 0..6 {
        let tile = block_tile(Block::Cactus, 0, face, false);
        let (u0, v0, u1, v1) = atlas_tile_uvs(tile.0, tile.1);
        let face_uvs = &uvs[face * 4..face * 4 + 4];
        assert_eq!(
            face_uvs
                .iter()
                .map(|uv| uv[0])
                .fold(f32::INFINITY, f32::min),
            u0
        );
        assert_eq!(
            face_uvs
                .iter()
                .map(|uv| uv[0])
                .fold(f32::NEG_INFINITY, f32::max),
            u1
        );
        assert_eq!(
            face_uvs
                .iter()
                .map(|uv| uv[1])
                .fold(f32::INFINITY, f32::min),
            v0
        );
        assert_eq!(
            face_uvs
                .iter()
                .map(|uv| uv[1])
                .fold(f32::NEG_INFINITY, f32::max),
            v1
        );
    }

    let dropped = dropped_block_meshes(Block::Cactus, 0, false, [1.0; 3], [1.0; 3]);
    assert!(dropped.alpha_masked);
    assert_eq!(dropped.body.vertex_count(), 24);
    let dropped_positions = dropped.body.positions();
    assert!(
        dropped_positions[8..12]
            .iter()
            .all(|point| point[0] == 0.4375)
    );
    assert!(
        dropped_positions[8..12]
            .iter()
            .any(|point| point[2] == -0.5)
    );
    assert!(dropped_positions[8..12].iter().any(|point| point[2] == 0.5));
}

#[test]
fn cactus_is_nonopaque_but_collides_and_uses_beta_bounds_and_hardness() {
    assert!(!Block::Cactus.is_opaque_cube());
    assert!(Block::Cactus.blocks_movement());
    assert_eq!(light_opacity(Block::Cactus), 0);
    assert_eq!(Block::Cactus.hardness(), 0.4);
    assert_eq!(
        Block::Cactus.collision_bounds(),
        Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375]))
    );
    assert_eq!(
        Block::Cactus.selection_bounds(),
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
        block: Block::Sand,
    };
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));
    let cactus_stack = ItemStack::from_block(Block::Cactus, 1).unwrap();
    assert_eq!(cactus_stack.runtime_block(), Some((Block::Cactus, 0)));

    let mut supported = Chunk::new();
    supported.set(8, 64, 8, Block::Sand);
    let mut chunks = world_with(supported);
    assert!(place_selected_block(
        &mut chunks,
        hit,
        player,
        cactus_stack.runtime_block().unwrap().0,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Cactus));

    let mut obstructed = Chunk::new();
    obstructed.set(8, 64, 8, Block::Sand);
    obstructed.set(7, 65, 8, Block::Stone);
    let mut chunks = world_with(obstructed);
    assert!(!place_selected_block(
        &mut chunks,
        hit,
        player,
        Block::Cactus,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Air));

    let mut cactus_support = Chunk::new();
    cactus_support.set(8, 64, 8, Block::Cactus);
    let mut chunks = world_with(cactus_support);
    let cactus_hit = BlockHit {
        block: Block::Cactus,
        ..hit
    };
    assert!(place_selected_block(
        &mut chunks,
        cactus_hit,
        player,
        Block::Cactus,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Cactus));
}

#[test]
fn desert_chunks_generate_repeatable_cactus_columns() {
    for seed in 0..128 {
        let generator = OverworldGenerator::new(seed);
        let generated = generator.generate(ChunkPosition::ZERO);
        let cactus_count = generated
            .chunk
            .blocks()
            .iter()
            .filter(|&&block| block == Block::Cactus)
            .count();
        if cactus_count > 0 {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_HEIGHT {
                        if generated.chunk.get(x, y, z) != Some(Block::Cactus) {
                            continue;
                        }
                        assert!(matches!(
                            generated.chunk.get(x, y.saturating_sub(1), z),
                            Some(Block::Sand | Block::Cactus)
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
                                .all(|block| !block.is_opaque_cube())
                            );
                        }
                    }
                }
            }
            let again = OverworldGenerator::new(seed).generate(ChunkPosition::ZERO);
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

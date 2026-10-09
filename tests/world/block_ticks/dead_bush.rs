use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::place_selected_block;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::textures::atlas_tile_uvs;
use game::rendering::textures::block_tile;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::lighting::Skylight;

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
                    biome: Biome::Plains,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            items: Vec::new(),
            populated: true,
        },
    );
    chunks
}

/// `BlockDeadBush.canThisPlantGrowOnThisBlockID`: sand only, not soil.
#[test]
fn dead_bush_only_places_on_sand() {
    let player = Aabb::new(Vec3::splat(100.0), Vec3::splat(101.0));
    for ground in [Block::Grass, Block::Dirt, Block::Farmland] {
        let mut chunk = Chunk::new();
        chunk.set(8, 64, 8, ground);
        let mut chunks = world_with(chunk);
        assert!(!place_selected_block(
            &mut chunks,
            BlockHit {
                x: 8,
                y: 64,
                z: 8,
                face: BlockFace::Up,
                block: ground,
            },
            player,
            Block::DeadBush,
        ));
        assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Air));
    }

    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Sand);
    let mut chunks = world_with(chunk);
    assert!(place_selected_block(
        &mut chunks,
        BlockHit {
            x: 8,
            y: 64,
            z: 8,
            face: BlockFace::Up,
            block: Block::Sand,
        },
        player,
        Block::DeadBush,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::DeadBush));
}

#[test]
fn dead_bush_uses_its_beta_sprite_tile_and_crossed_mesh() {
    assert_eq!(block_tile(Block::DeadBush, 0, 0, false), (7, 3));
    assert!(Block::DeadBush.is_crossed_plant());
    assert!(!Block::DeadBush.is_opaque_cube());
    assert_eq!(Block::DeadBush.collision_bounds(), None);
    assert_eq!(
        Block::DeadBush.selection_bounds(),
        ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9])
    );

    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Block::DeadBush);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    assert_eq!(meshes.opaque.vertex_count(), 0);
    let positions = meshes.masked.positions();
    // Two crossed planes, one quad per side.
    assert_eq!(positions.len(), 16);

    let uvs = meshes.masked.uvs();
    let (u0, v0, u1, v1) = atlas_tile_uvs(7, 3);
    assert_eq!(
        (
            uvs.iter().map(|uv| uv[0]).fold(f32::INFINITY, f32::min),
            uvs.iter().map(|uv| uv[1]).fold(f32::INFINITY, f32::min),
            uvs.iter().map(|uv| uv[0]).fold(f32::NEG_INFINITY, f32::max),
            uvs.iter().map(|uv| uv[1]).fold(f32::NEG_INFINITY, f32::max),
        ),
        (u0, v0, u1, v1)
    );
}

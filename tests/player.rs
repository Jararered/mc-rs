use bevy::prelude::Vec3;
use game::entity::EntitySize;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::HeartFill;
use game::player::MAX_PLAYER_HEALTH;
use game::player::PLACED_BLOCK;
use game::player::PlayerHealth;
use game::player::break_block;
use game::player::place_block;
use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;

#[test]
fn default_health_is_ten_full_hearts() {
    let health = PlayerHealth::default();
    assert_eq!(health.current, MAX_PLAYER_HEALTH);
    for index in 0..10 {
        assert_eq!(health.heart_fill(index), HeartFill::Full);
    }
}

#[test]
fn hearts_show_half_and_empty_from_remaining_health() {
    let health = PlayerHealth { current: 13 };
    assert_eq!(health.heart_fill(0), HeartFill::Full);
    assert_eq!(health.heart_fill(5), HeartFill::Full);
    assert_eq!(health.heart_fill(6), HeartFill::Half);
    assert_eq!(health.heart_fill(7), HeartFill::Empty);
    assert_eq!(health.heart_fill(9), HeartFill::Empty);
}

#[test]
fn zero_health_is_all_empty_hearts() {
    let health = PlayerHealth { current: 0 };
    for index in 0..10 {
        assert_eq!(health.heart_fill(index), HeartFill::Empty);
    }
}

fn plains() -> BiomeMap {
    BiomeMap::from_cells(
        [Climate {
            temperature: 0.5,
            humidity: 0.5,
            biome: Biome::Plains,
        }; CHUNK_SIZE * CHUNK_SIZE],
    )
}

fn world_with(chunk: Chunk) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPos::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            biomes: plains(),
            chunk,
        },
    );
    chunks
}

fn hit(x: i32, y: i32, z: i32, face: BlockFace, block: BlockId) -> BlockHit {
    BlockHit {
        x,
        y,
        z,
        face,
        block,
    }
}

#[test]
fn breaking_replaces_a_solid_block_with_air() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Dirt);
    let mut chunks = world_with(chunk);

    assert!(break_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Dirt)
    ));
    assert_eq!(chunks.block_at(8, 64, 8), Some(BlockId::Air));
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(8, 8), 0);
}

#[test]
fn bedrock_cannot_be_broken() {
    let mut chunk = Chunk::new();
    chunk.set(8, 0, 8, BlockId::Bedrock);
    let mut chunks = world_with(chunk);

    assert!(!break_block(
        &mut chunks,
        hit(8, 0, 8, BlockFace::Up, BlockId::Bedrock)
    ));
    assert_eq!(chunks.block_at(8, 0, 8), Some(BlockId::Bedrock));
}

#[test]
fn placing_puts_stone_against_the_hit_face() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Dirt);
    let mut chunks = world_with(chunk);
    let player = EntitySize::PLAYER.aabb(Vec3::new(8.5, 70.0, 8.5));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Dirt),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(PLACED_BLOCK));
    assert_eq!(PLACED_BLOCK, BlockId::Stone);
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(8, 8), 66);
}

#[test]
fn placing_cannot_overlap_the_player() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);
    let player = EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));

    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Stone),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Air));
}

#[test]
fn placing_replaces_water_and_not_solid_blocks() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    chunk.set(8, 65, 8, BlockId::Water);
    chunk.set(9, 64, 8, BlockId::Dirt);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Stone),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Stone));

    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, BlockId::Stone),
        player
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::Dirt));
}

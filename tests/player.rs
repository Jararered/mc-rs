use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use bevy::prelude::Vec3;
use game::entity::EntitySize;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::BlockFocus;
use game::player::HeartFill;
use game::player::MAX_PLAYER_HEALTH;
use game::player::MiningState;
use game::player::OUTLINE_THICKNESS;
use game::player::PLACED_BLOCK;
use game::player::PlayerHealth;
use game::player::break_block;
use game::player::destroy_overlay_mesh;
use game::player::destroy_stage;
use game::player::hand_ticks_to_break;
use game::player::place_block;
use game::player::punch_nearly_transparent_texels;
use game::player::selection_outline_mesh;
use game::world::block::block::BlockId;
use game::world::block::properties::hand_mine_progress_per_tick;
use game::world::block::properties::hardness;
use game::world::block::properties::harvestable_by_hand;
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
fn placing_puts_torch_against_the_hit_face() {
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
    assert_eq!(PLACED_BLOCK, BlockId::Torch);
    assert_eq!(chunks.get(ChunkPos::ZERO).unwrap().heightmap.get(8, 8), 65);
}

#[test]
fn placing_torch_can_overlap_the_player() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);
    let player = EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Stone),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Torch));
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
    assert_eq!(chunks.block_at(8, 65, 8), Some(BlockId::Torch));

    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, BlockId::Stone),
        player
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::Dirt));
}

#[test]
fn torch_attaches_to_walls_and_drops_when_support_breaks() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::ZERO, Vec3::ZERO);

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, BlockId::Stone),
        player,
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::TorchWest));
    assert!(break_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, BlockId::Stone),
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::Air));
}

#[test]
fn torch_requires_a_support_face() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::ZERO, Vec3::ZERO);
    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Down, BlockId::Stone),
        player,
    ));
    assert_eq!(chunks.block_at(8, 63, 8), Some(BlockId::Air));
}

#[test]
fn usual_blocks_use_beta_hand_break_times() {
    let grounded = |block| hand_ticks_to_break(block, true, false);
    assert_eq!(grounded(BlockId::Dirt), Some(15));
    assert_eq!(grounded(BlockId::Grass), Some(18));
    assert_eq!(grounded(BlockId::Sand), Some(15));
    assert_eq!(grounded(BlockId::Gravel), Some(18));
    assert_eq!(grounded(BlockId::Leaves), Some(6));
    assert_eq!(grounded(BlockId::Wood), Some(60));
    assert_eq!(grounded(BlockId::WoodenPlanks), Some(60));
    assert_eq!(grounded(BlockId::Stone), Some(150));
    assert_eq!(grounded(BlockId::Cobblestone), Some(200));
    assert_eq!(grounded(BlockId::CoalOre), Some(300));
    assert_eq!(grounded(BlockId::Netherrack), Some(40));
    assert_eq!(grounded(BlockId::Obsidian), Some(1000));
    assert_eq!(grounded(BlockId::Tnt), Some(1));
    assert_eq!(grounded(BlockId::Bedrock), None);
}

#[test]
fn airborne_and_water_only_slow_blocks_harvestable_by_hand() {
    assert!(harvestable_by_hand(BlockId::Dirt));
    assert!(!harvestable_by_hand(BlockId::Stone));
    assert_eq!(hand_ticks_to_break(BlockId::Dirt, false, false), Some(75));
    assert_eq!(hand_ticks_to_break(BlockId::Dirt, true, true), Some(75));
    assert_eq!(hand_ticks_to_break(BlockId::Stone, false, true), Some(150));
}

#[test]
fn punching_accumulates_until_the_block_breaks() {
    let dirt = hit(8, 64, 8, BlockFace::Up, BlockId::Dirt);
    let mut mining = MiningState::default();
    assert!(mining.tick(Some(dirt), true, false).is_none());
    let mut ticks = 0;
    let broken = loop {
        ticks += 1;
        assert!(ticks <= 20, "dirt should break within 15 damaging ticks");
        if mining.tick(Some(dirt), true, false).is_some() {
            break ticks;
        }
    };
    assert_eq!(broken, 15);
    assert_eq!(mining.damage(), 0.0);
}

#[test]
fn hardness_zero_breaks_on_the_click() {
    let tnt = hit(4, 10, 4, BlockFace::North, BlockId::Tnt);
    let mut mining = MiningState::default();
    assert!(mining.try_instant(tnt, true, false).is_some());
    assert!(
        mining
            .try_instant(hit(4, 10, 4, BlockFace::North, BlockId::Dirt), true, false)
            .is_none()
    );
}

#[test]
fn looking_at_a_new_block_resets_mining_progress() {
    let dirt = hit(8, 64, 8, BlockFace::Up, BlockId::Dirt);
    let grass = hit(8, 65, 8, BlockFace::Up, BlockId::Grass);
    let mut mining = MiningState::default();
    mining.tick(Some(dirt), true, false);
    for _ in 0..10 {
        mining.tick(Some(dirt), true, false);
    }
    assert!(mining.damage() > 0.0);
    mining.tick(Some(grass), true, false);
    assert_eq!(mining.damage(), 0.0);
    assert!((hardness(BlockId::Grass) - 0.6).abs() < f32::EPSILON);
    assert!(hand_mine_progress_per_tick(BlockId::Dirt, true, false) > 0.0);
}

#[test]
fn destroy_stage_follows_beta_damage_partial_time() {
    assert_eq!(destroy_stage(0.0), None);
    assert_eq!(destroy_stage(0.05), Some(0));
    assert_eq!(destroy_stage(0.15), Some(1));
    assert_eq!(destroy_stage(0.95), Some(9));
    assert_eq!(destroy_stage(1.0), Some(9));
}

#[test]
fn mining_reset_clears_the_destroy_overlay_stage() {
    let dirt = hit(8, 64, 8, BlockFace::Up, BlockId::Dirt);
    let mut mining = MiningState::default();
    mining.tick(Some(dirt), true, false);
    for _ in 0..10 {
        mining.tick(Some(dirt), true, false);
    }
    assert!(mining.destroy_stage().is_some());
    mining.reset();
    assert_eq!(mining.destroy_stage(), None);
    assert_eq!(mining.damage(), 0.0);

    let mut focus = BlockFocus {
        hit: Some(dirt),
        mining_damage: 0.4,
    };
    assert_eq!(focus.destroy_stage(), Some(4));
    focus.mining_damage = 0.0;
    assert_eq!(focus.destroy_stage(), None);
}

#[test]
fn selection_outline_is_a_twelve_edge_wire_cube() {
    let mesh = selection_outline_mesh();
    assert_eq!(
        mesh.primitive_topology(),
        bevy::render::render_resource::PrimitiveTopology::TriangleList
    );
    // 12 edges × 6 faces × 4 corners of a thickness box.
    assert_eq!(mesh.count_vertices(), 12 * 24);
    assert_eq!(mesh.indices().unwrap().len(), 12 * 36);
    assert!(OUTLINE_THICKNESS >= 1.0 / 32.0);
}

#[test]
fn destroy_stage_empty_texels_become_fully_transparent() {
    let mut pixels = [
        255, 255, 255, 1, // empty destroy-stage background
        61, 61, 61, 255, // crack
        255, 255, 255, 15, 120, 120, 120, 200, 210, 210, 210,
        255, // opaque pale-grey background
    ];
    punch_nearly_transparent_texels(&mut pixels);
    assert_eq!(&pixels[0..4], &[0, 0, 0, 0]);
    assert_eq!(&pixels[4..8], &[61, 61, 61, 255]);
    assert_eq!(&pixels[8..12], &[0, 0, 0, 0]);
    assert_eq!(&pixels[12..16], &[120, 120, 120, 200]);
    assert_eq!(&pixels[16..20], &[0, 0, 0, 0]);
}

#[test]
fn destroy_overlay_samples_the_terrain_atlas_crack_tiles() {
    let mesh = destroy_overlay_mesh(3);
    assert_eq!(mesh.count_vertices(), 24);
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        panic!("destroy overlay should have atlas UVs");
    };
    // Stage 3 is terrain.png tile (3, 15). Padding keeps UVs inside the tile.
    let gutter = 2.0 / 320.0;
    for uv in uvs {
        assert!(
            uv[0] >= 3.0 / 16.0 + gutter - 1e-5 && uv[0] <= 4.0 / 16.0 - gutter + 1e-5,
            "destroy stage 3 U={:?} should stay in tile (3, 15)",
            uv[0]
        );
        assert!(
            uv[1] >= 15.0 / 16.0 + gutter - 1e-5 && uv[1] <= 1.0 - gutter + 1e-5,
            "destroy stage 3 V={:?} should stay on the last atlas row",
            uv[1]
        );
    }
}

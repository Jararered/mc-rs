use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::block::direction::Direction;
use game::block::fluids::Fluid;
use game::block::properties::hand_mine_progress_per_tick;
use game::entity::EntitySize;
use game::item::Item;
use game::item::ItemStack;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::BlockFocus;
use game::player::HeartFill;
use game::player::MAX_PLAYER_HEALTH;
use game::player::MiningState;
use game::player::PLACED_BLOCK;
use game::player::PlayerHealth;
use game::player::break_block;
use game::player::destroy_overlay_mesh;
use game::player::destroy_stage;
use game::player::double_crack_intensity;
use game::player::hand_ticks_to_break;
use game::player::pick_up_fluid;
use game::player::place_block;
use game::player::place_fluid;
use game::player::place_selected_block_facing;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;

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
        ChunkPosition::ZERO,
        GeneratedChunk {
            heightmap: Heightmap::from_chunk(&chunk),
            biomes: plains(),
            chunk,
            items: Vec::new(),
            populated: true,
        },
    );
    chunks
}

fn hit(x: i32, y: i32, z: i32, face: BlockFace, block: Block) -> BlockHit {
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
    chunk.set(8, 64, 8, Block::Dirt);
    let mut chunks = world_with(chunk);

    assert!(break_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Dirt)
    ));
    assert_eq!(chunks.block_at(8, 64, 8), Some(Block::Air));
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(8, 8),
        0
    );
}

#[test]
fn bedrock_cannot_be_broken() {
    let mut chunk = Chunk::new();
    chunk.set(8, 0, 8, Block::Bedrock);
    let mut chunks = world_with(chunk);

    assert!(!break_block(
        &mut chunks,
        hit(8, 0, 8, BlockFace::Up, Block::Bedrock)
    ));
    assert_eq!(chunks.block_at(8, 0, 8), Some(Block::Bedrock));
}

#[test]
fn placing_puts_torch_against_the_hit_face() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Dirt);
    let mut chunks = world_with(chunk);
    let player = EntitySize::PLAYER.aabb(Vec3::new(8.5, 70.0, 8.5));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Dirt),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(PLACED_BLOCK));
    assert_eq!(PLACED_BLOCK, Block::Torch);
    assert_eq!(
        chunks.get(ChunkPosition::ZERO).unwrap().heightmap.get(8, 8),
        65
    );
}

#[test]
fn placed_furnace_front_faces_the_player_and_survives_lit_transitions() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Dirt);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));

    assert!(place_selected_block_facing(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Dirt),
        player,
        Block::Furnace,
        0,
        Direction::East,
    ));
    let furnace = chunks.block_at(8, 65, 8).unwrap();
    let metadata = chunks.metadata_at(8, 65, 8);
    assert_eq!(furnace, Block::Furnace);
    assert_eq!(furnace.facing(metadata), Some(Direction::East));
    // Lighting the furnace swaps the block and keeps the facing.
    chunks.set_block_with_metadata(8, 65, 8, Block::LitFurnace, metadata);
    assert_eq!(
        Block::LitFurnace.facing(chunks.metadata_at(8, 65, 8)),
        Some(Direction::East)
    );
}

#[test]
fn placed_ladder_attaches_to_the_clicked_wall_and_drops_as_a_ladder_item() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(2.0, 70.0, 2.0), Vec3::new(2.6, 71.8, 2.6));

    assert!(place_selected_block_facing(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, Block::Stone),
        player,
        Block::Ladder,
        0,
        Direction::South,
    ));
    let ladder = chunks.block_at(9, 64, 8).unwrap();
    let metadata = chunks.metadata_at(9, 64, 8);
    assert_eq!(ladder, Block::Ladder);
    assert_eq!(ladder.facing(metadata), Some(Direction::West));
    assert_eq!(ladder.item_form(metadata), (Block::Ladder, 0));
    assert_eq!(ladder.placed(0), Some((Block::Ladder, 0)));
    assert!(break_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone)
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(Block::Air));
}

#[test]
fn placed_pumpkin_front_faces_the_player() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Grass);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(2.0, 70.0, 2.0), Vec3::new(2.6, 71.8, 2.6));

    assert!(place_selected_block_facing(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Grass),
        player,
        Block::Pumpkin,
        0,
        Direction::East,
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Pumpkin));
    assert_eq!(
        Block::Pumpkin.facing(chunks.metadata_at(8, 65, 8)),
        Some(Direction::East)
    );
}

#[test]
fn placing_torch_can_overlap_the_player() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);
    let player = EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Torch));
}

#[test]
fn placing_replaces_water_and_not_solid_blocks() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(8, 65, 8, Block::Water);
    chunk.set(9, 64, 8, Block::Dirt);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone),
        player
    ));
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Torch));

    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, Block::Stone),
        player
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(Block::Dirt));
}

#[test]
fn torch_attaches_to_walls_and_drops_when_support_breaks() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::ZERO, Vec3::ZERO);

    assert!(place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::East, Block::Stone),
        player,
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(Block::Torch));
    assert_eq!(
        Block::Torch.facing(chunks.metadata_at(9, 64, 8)),
        Some(Direction::West)
    );
    assert!(break_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone),
    ));
    assert_eq!(chunks.block_at(9, 64, 8), Some(Block::Air));
}

#[test]
fn empty_bucket_picks_up_a_water_source_and_leaves_air() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Water);
    let mut chunks = world_with(chunk);

    let (x, y, z, previous, fluid) =
        pick_up_fluid(&mut chunks, Vec3::new(8.5, 66.0, 8.5), Vec3::NEG_Y)
            .expect("a water source should be picked up");
    assert_eq!((x, y, z), (8, 64, 8));
    assert_eq!(previous, Block::Water);
    assert_eq!(fluid, Fluid::Water);
    assert_eq!(chunks.block_at(8, 64, 8), Some(Block::Air));
}

#[test]
fn empty_bucket_picks_up_a_lava_source() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Lava);
    let mut chunks = world_with(chunk);

    let (.., fluid) = pick_up_fluid(&mut chunks, Vec3::new(8.5, 66.0, 8.5), Vec3::NEG_Y)
        .expect("a lava source should be picked up");
    assert_eq!(fluid, Fluid::Lava);
    assert_eq!(chunks.block_at(8, 64, 8), Some(Block::Air));
}

#[test]
fn empty_bucket_leaves_flowing_water_in_place() {
    let mut chunk = Chunk::new();
    chunk.set_with_metadata(8, 64, 8, Block::FlowingWater, 3);
    let mut chunks = world_with(chunk);

    assert!(pick_up_fluid(&mut chunks, Vec3::new(8.5, 66.0, 8.5), Vec3::NEG_Y).is_none());
    assert_eq!(chunks.block_at(8, 64, 8), Some(Block::FlowingWater));
    assert_eq!(chunks.metadata_at(8, 64, 8), 3);
}

#[test]
fn empty_bucket_stays_empty_when_aimed_at_a_solid_block() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);

    assert!(pick_up_fluid(&mut chunks, Vec3::new(8.5, 66.0, 8.5), Vec3::NEG_Y).is_none());
    assert_eq!(chunks.block_at(8, 64, 8), Some(Block::Stone));
}

#[test]
fn water_bucket_fills_the_non_solid_cell_beside_the_hit_face() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);

    let (x, y, z, previous, metadata) = place_fluid(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone),
        Fluid::Water,
    )
    .expect("air above a solid block accepts the fluid");
    assert_eq!((x, y, z), (8, 65, 8));
    assert_eq!(previous, Block::Air);
    assert_eq!(metadata, 0);
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::FlowingWater));
    assert_eq!(chunks.metadata_at(8, 65, 8), 0);
}

#[test]
fn lava_bucket_places_flowing_lava() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);

    place_fluid(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Stone),
        Fluid::Lava,
    )
    .expect("air above a solid block accepts the fluid");
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::FlowingLava));
    assert_eq!(chunks.metadata_at(8, 65, 8), 0);
}

#[test]
fn water_bucket_overwrites_non_solid_blocks_without_dropping_them() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Grass);
    chunk.set(8, 65, 8, Block::TallGrass);
    let mut chunks = world_with(chunk);

    let (x, y, z, previous, _) = place_fluid(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Up, Block::Grass),
        Fluid::Water,
    )
    .expect("tall grass is not a solid material");
    assert_eq!((x, y, z), (8, 65, 8));
    assert_eq!(previous, Block::TallGrass);
    assert_eq!(chunks.block_at(8, 65, 8), Some(Block::FlowingWater));
}

#[test]
fn lava_bucket_cannot_fill_a_solid_block() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(9, 64, 8, Block::Dirt);
    let mut chunks = world_with(chunk);

    assert!(
        place_fluid(
            &mut chunks,
            hit(8, 64, 8, BlockFace::East, Block::Stone),
            Fluid::Lava,
        )
        .is_none()
    );
    assert_eq!(chunks.block_at(9, 64, 8), Some(Block::Dirt));
}

#[test]
fn torch_requires_a_support_face() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::ZERO, Vec3::ZERO);
    assert!(!place_block(
        &mut chunks,
        hit(8, 64, 8, BlockFace::Down, Block::Stone),
        player,
    ));
    assert_eq!(chunks.block_at(8, 63, 8), Some(Block::Air));
}

#[test]
fn usual_blocks_use_beta_hand_break_times() {
    let grounded = |block| hand_ticks_to_break(block, true, false);
    assert_eq!(grounded(Block::Dirt), Some(15));
    assert_eq!(grounded(Block::Grass), Some(18));
    assert_eq!(grounded(Block::Sand), Some(15));
    assert_eq!(grounded(Block::Gravel), Some(18));
    assert_eq!(grounded(Block::Leaves), Some(6));
    assert_eq!(grounded(Block::Wood), Some(60));
    assert_eq!(grounded(Block::WoodenPlanks), Some(60));
    assert_eq!(grounded(Block::Stone), Some(150));
    assert_eq!(grounded(Block::Cobblestone), Some(200));
    assert_eq!(grounded(Block::CoalOre), Some(300));
    assert_eq!(grounded(Block::Netherrack), Some(40));
    assert_eq!(grounded(Block::Obsidian), Some(1000));
    assert_eq!(grounded(Block::Tnt), Some(1));
    assert_eq!(grounded(Block::Bedrock), None);
}

#[test]
fn airborne_and_water_only_slow_blocks_harvestable_by_hand() {
    assert!(Block::Dirt.harvestable_by_hand());
    assert!(!Block::Stone.harvestable_by_hand());
    assert_eq!(hand_ticks_to_break(Block::Dirt, false, false), Some(75));
    assert_eq!(hand_ticks_to_break(Block::Dirt, true, true), Some(75));
    assert_eq!(hand_ticks_to_break(Block::Stone, false, true), Some(150));
}

#[test]
fn punching_accumulates_until_the_block_breaks() {
    let dirt = hit(8, 64, 8, BlockFace::Up, Block::Dirt);
    let mut mining = MiningState::default();
    assert!(mining.tick(Some(dirt), None, true, false).is_none());
    let mut ticks = 0;
    let broken = loop {
        ticks += 1;
        assert!(ticks <= 20, "dirt should break within 15 damaging ticks");
        if mining.tick(Some(dirt), None, true, false).is_some() {
            break ticks;
        }
    };
    assert_eq!(broken, 15);
    assert_eq!(mining.damage(), 0.0);
}

#[test]
fn hardness_zero_breaks_on_the_click() {
    let tnt = hit(4, 10, 4, BlockFace::North, Block::Tnt);
    let mut mining = MiningState::default();
    assert!(mining.try_instant(tnt, None, true, false).is_some());
    assert!(
        mining
            .try_instant(
                hit(4, 10, 4, BlockFace::North, Block::Dirt),
                None,
                true,
                false
            )
            .is_none()
    );
}

#[test]
fn looking_at_a_new_block_resets_mining_progress() {
    let dirt = hit(8, 64, 8, BlockFace::Up, Block::Dirt);
    let grass = hit(8, 65, 8, BlockFace::Up, Block::Grass);
    let mut mining = MiningState::default();
    mining.tick(Some(dirt), None, true, false);
    for _ in 0..10 {
        mining.tick(Some(dirt), None, true, false);
    }
    assert!(mining.damage() > 0.0);
    mining.tick(Some(grass), None, true, false);
    assert_eq!(mining.damage(), 0.0);
    assert!((Block::Grass.hardness() - 0.6).abs() < f32::EPSILON);
    assert!(hand_mine_progress_per_tick(Block::Dirt, true, false) > 0.0);
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
    let dirt = hit(8, 64, 8, BlockFace::Up, Block::Dirt);
    let mut mining = MiningState::default();
    mining.tick(Some(dirt), None, true, false);
    for _ in 0..10 {
        mining.tick(Some(dirt), None, true, false);
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
fn diamond_pick_breaks_stone_in_six_damaging_ticks() {
    let stone = hit(8, 64, 8, BlockFace::Up, Block::Stone);
    let pick = Some(ItemStack::new(Item::DiamondPickaxe, 1).unwrap());
    let mut mining = MiningState::default();
    assert!(mining.tick(Some(stone), pick, true, false).is_none());
    let mut ticks = 0;
    let broken = loop {
        ticks += 1;
        assert!(ticks <= 6, "diamond pick should break stone in 6 ticks");
        if mining.tick(Some(stone), pick, true, false).is_some() {
            break ticks;
        }
    };
    assert_eq!(broken, 6);
    assert_eq!(mining.damage(), 0.0);
}

#[test]
fn shears_break_leaves_on_the_click() {
    let leaves = hit(3, 70, 3, BlockFace::Up, Block::Leaves);
    let shears = Some(ItemStack::new(Item::Shears, 1).unwrap());
    let mut mining = MiningState::default();
    assert!(mining.try_instant(leaves, shears, true, false).is_some());
    assert!(mining.tick(Some(leaves), shears, true, false).is_none());
    assert_eq!(mining.damage(), 0.0);
}

#[test]
fn switching_tools_keeps_mining_progress() {
    let stone = hit(8, 64, 8, BlockFace::Up, Block::Stone);
    let wood = Some(ItemStack::new(Item::WoodenPickaxe, 1).unwrap());
    let diamond = Some(ItemStack::new(Item::DiamondPickaxe, 1).unwrap());
    let mut mining = MiningState::default();
    mining.tick(Some(stone), wood, true, false);
    mining.tick(Some(stone), wood, true, false);
    let damage = mining.damage();
    assert!(damage > 0.0);
    mining.tick(Some(stone), diamond, true, false);
    assert!(mining.damage() > damage);
    assert_eq!(mining.target(), Some((8, 64, 8)));
}

#[test]
fn crack_texel_intensity_is_doubled_for_multiply_blending() {
    let mut pixels = [
        255, 255, 255, 1, // empty destroy-stage background
        61, 61, 61, 255, // crack
        255, 255, 255, 15, 120, 120, 120, 200, 210, 210, 210,
        255, // opaque pale-grey background
    ];
    double_crack_intensity(&mut pixels);
    assert_eq!(&pixels[0..4], &[255, 255, 255, 1]);
    assert_eq!(&pixels[4..8], &[122, 122, 122, 255]);
    assert_eq!(&pixels[8..12], &[255, 255, 255, 15]);
    assert_eq!(&pixels[12..16], &[240, 240, 240, 200]);
    assert_eq!(&pixels[16..20], &[255, 255, 255, 255]);
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

#[test]
fn air_bubbles_follow_betas_hud_rounding() {
    use game::player::Bubble;
    use game::player::PlayerSurvival;
    let row = |air: i16| {
        let mut survival = PlayerSurvival::default();
        survival.air = air;
        survival.head_in_water = true;
        let count = |kind| (0..10).filter(|&i| survival.bubble(i) == kind).count();
        (count(Bubble::Full), count(Bubble::Popping))
    };
    assert_eq!(row(300), (10, 0));
    assert_eq!(row(151), (5, 1));
    assert_eq!(row(150), (5, 0));
    assert_eq!(row(2), (0, 1));
    assert_eq!(row(0), (0, 0));
    assert_eq!(row(-10), (0, 0));

    // Above water the row is hidden however little air is left.
    let mut surfaced = PlayerSurvival::default();
    surfaced.air = 40;
    assert_eq!(surfaced.bubble(0), Bubble::Empty);
}

#[test]
fn placed_repeater_points_away_from_the_player() {
    // `BlockRedstoneRepeater.onBlockPlacedBy` takes the repeater facing from
    // the player's yaw, so the plate and its two torches face away from the
    // player and the input side faces them.
    for (front, metadata) in [
        (Direction::South, 0),
        (Direction::West, 1),
        (Direction::North, 2),
        (Direction::East, 3),
    ] {
        let mut chunk = Chunk::new();
        chunk.set(8, 64, 8, Block::Stone);
        let mut chunks = world_with(chunk);
        let player = Aabb::new(Vec3::new(2.0, 70.0, 2.0), Vec3::new(2.6, 71.8, 2.6));

        assert!(place_selected_block_facing(
            &mut chunks,
            hit(8, 64, 8, BlockFace::Up, Block::Stone),
            player,
            Block::Repeater,
            0,
            front,
        ));
        assert_eq!(chunks.block_at(8, 65, 8), Some(Block::Repeater));
        assert_eq!(chunks.metadata_at(8, 65, 8), metadata, "{front:?}");
    }
}

#[test]
fn normal_and_sticky_pistons_can_be_placed_facing_up_and_down() {
    for block in [Block::Piston, Block::StickyPiston] {
        let mut floor = Chunk::new();
        floor.set(8, 64, 8, Block::Stone);
        let mut chunks = world_with(floor);
        // Standing two blocks above the placement cell makes the piston face up.
        let above =
            EntitySize::PLAYER.aabb(Vec3::new(8.5, 66.0 + EntitySize::PLAYER.y_offset, 8.5));
        assert!(place_selected_block_facing(
            &mut chunks,
            hit(8, 64, 8, BlockFace::Up, Block::Stone),
            above,
            block,
            0,
            Direction::West,
        ));
        assert_eq!(chunks.block_at(8, 65, 8), Some(block));
        assert_eq!(chunks.metadata_at(8, 65, 8), 1, "{block:?} should face up");

        let mut ceiling = Chunk::new();
        ceiling.set(8, 70, 8, Block::Stone);
        let mut chunks = world_with(ceiling);
        let below =
            EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));
        assert!(place_selected_block_facing(
            &mut chunks,
            hit(8, 70, 8, BlockFace::Down, Block::Stone),
            below,
            block,
            0,
            Direction::East,
        ));
        assert_eq!(chunks.block_at(8, 69, 8), Some(block));
        assert_eq!(
            chunks.metadata_at(8, 69, 8),
            0,
            "{block:?} should face down"
        );
    }
}

#[test]
fn piston_placement_uses_horizontal_facing_when_near_eye_level_or_far_away() {
    for block in [Block::Piston, Block::StickyPiston] {
        for (front, metadata) in [
            (Direction::North, 2),
            (Direction::East, 5),
            (Direction::South, 3),
            (Direction::West, 4),
        ] {
            let mut chunk = Chunk::new();
            chunk.set(8, 64, 8, Block::Stone);
            let mut chunks = world_with(chunk);
            // Next to the piston, not inside its target cell, and at eye level.
            let beside =
                EntitySize::PLAYER.aabb(Vec3::new(9.5, 63.0 + EntitySize::PLAYER.y_offset, 9.5));
            assert!(place_selected_block_facing(
                &mut chunks,
                hit(8, 64, 8, BlockFace::East, Block::Stone),
                beside,
                block,
                0,
                front,
            ));
            assert_eq!(chunks.metadata_at(9, 64, 8), metadata);
        }
        let mut chunk = Chunk::new();
        chunk.set(8, 64, 8, Block::Stone);
        let mut chunks = world_with(chunk);
        let far = EntitySize::PLAYER.aabb(Vec3::new(10.0, 66.0 + EntitySize::PLAYER.y_offset, 8.5));
        assert!(place_selected_block_facing(
            &mut chunks,
            hit(8, 64, 8, BlockFace::Up, Block::Stone),
            far,
            block,
            0,
            Direction::West,
        ));
        assert_eq!(chunks.metadata_at(8, 65, 8), 4);
    }
}

#[test]
fn placed_redstone_controls_keep_their_support_side() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(10, 64, 8, Block::Stone);
    let mut chunks = world_with(chunk);
    let player = Aabb::new(Vec3::new(0.0, 70.0, 0.0), Vec3::new(0.6, 71.8, 0.6));
    // A lever or button on the east face of a cube hangs on its west side.
    for (block, metadata) in [(Block::Lever, 1), (Block::StoneButton, 1)] {
        let mut chunks = world_with({
            let mut chunk = Chunk::new();
            chunk.set(8, 64, 8, Block::Stone);
            chunk
        });
        assert!(place_selected_block_facing(
            &mut chunks,
            hit(8, 64, 8, BlockFace::East, Block::Stone),
            player,
            block,
            0,
            Direction::East,
        ));
        assert_eq!(chunks.metadata_at(9, 64, 8), metadata, "{block:?}");
    }
    // A button cannot sit on the top of a cube, and nothing hangs from air.
    assert!(!place_selected_block_facing(
        &mut chunks,
        hit(10, 64, 8, BlockFace::Up, Block::Stone),
        player,
        Block::StoneButton,
        0,
        Direction::East,
    ));
    assert!(!place_selected_block_facing(
        &mut chunks,
        hit(10, 66, 8, BlockFace::East, Block::Air),
        player,
        Block::Lever,
        0,
        Direction::East,
    ));
    // Dust needs a full cube beneath it.
    assert!(place_selected_block_facing(
        &mut chunks,
        hit(10, 64, 8, BlockFace::Up, Block::Stone),
        player,
        Block::RedstoneWire,
        0,
        Direction::East,
    ));
    assert!(!place_selected_block_facing(
        &mut chunks,
        hit(10, 70, 8, BlockFace::Up, Block::Air),
        player,
        Block::RedstoneWire,
        0,
        Direction::East,
    ));
}

#[test]
fn healing_caps_at_maximum_health() {
    let mut health = PlayerHealth { current: 6 };
    health.heal(8);
    assert_eq!(health.current, 14);
    health.heal(42);
    assert_eq!(health.current, game::player::MAX_PLAYER_HEALTH);
}

#[test]
fn hurt_camera_roll_eases_in_and_out_without_jumps() {
    use game::player::hurt_roll_degrees;

    assert_eq!(hurt_roll_degrees(-0.5), 0.0);
    assert!(hurt_roll_degrees(10.0).abs() < 1.0e-4);
    assert!(hurt_roll_degrees(0.0).abs() < 1.0e-4);

    let mut peak = 0.0_f32;
    let mut previous = hurt_roll_degrees(10.0);
    let mut largest_step = 0.0_f32;
    for step in 1..=600 {
        let roll = hurt_roll_degrees(10.0 - step as f32 / 60.0);
        peak = peak.max(roll);
        largest_step = largest_step.max((roll - previous).abs());
        previous = roll;
    }
    assert!((peak - 14.0).abs() < 0.1, "peak {peak}");
    // At 60 fps the roll may not move more than a degree per frame.
    assert!(largest_step < 1.0, "largest step {largest_step}");
}

use bevy::prelude::*;
use game::block::id::Id;
use game::entity::EntitySize;
use game::entity::shadow::Shadow;
use game::entity::shadow::place_shadow;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;

fn plains() -> BiomeMap {
    BiomeMap::from_cells(
        [Climate {
            temperature: 0.5,
            humidity: 0.5,
            biome: Biome::Plains,
        }; CHUNK_SIZE * CHUNK_SIZE],
    )
}

fn generated(chunk: Chunk) -> GeneratedChunk {
    GeneratedChunk {
        heightmap: Heightmap::from_chunk(&chunk),
        biomes: plains(),
        chunk,
        items: Vec::new(),
        populated: true,
    }
}

fn world_with(chunk: Chunk) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    chunks
}

/// A sunlit floor, top surface at `floor_y + 1`, open to the sky above.
fn lit_floor_world(floor_y: usize) -> WorldChunks {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, floor_y, z, Id::Stone);
        }
    }
    world_with(chunk)
}

/// The same floor, but capped by a ceiling that blocks skylight from the
/// item's cell and no torches, so `getBlockLightValue` there is 0.
fn dark_floor_world(floor_y: usize) -> WorldChunks {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, floor_y, z, Id::Stone);
            chunk.set(x, floor_y + 2, z, Id::Stone);
        }
    }
    world_with(chunk)
}

fn item_position(y: f32) -> Vec3 {
    Vec3::new(8.0, y, 8.0)
}

#[test]
fn grounded_lit_item_casts_a_shadow_on_the_block_below() {
    let chunks = lit_floor_world(63);
    // Resting on the floor: feet (item_y - y_offset) sit at 64.0.
    let item_position = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let camera = Vec3::new(8.0, 70.0, 8.0);

    let (position, alpha) = place_shadow(
        &Shadow::DROPPED_ITEM,
        item_position,
        &EntitySize::DROPPED_ITEM,
        camera,
        &chunks,
        0,
    )
    .expect("a grounded, lit, nearby item should cast a shadow");

    assert_eq!(position.x, 8.0);
    assert_eq!(position.z, 8.0);
    // Sits just above the floor's top surface (64.0), not flush with it.
    assert_eq!(position.y, 64.0 + 1.0 / 64.0);
    // (fade - height_above_ground / 2) * 0.5 * brightness, hand-computed for
    // this exact distance and height above ground with full brightness.
    assert!((alpha - 0.293_19).abs() < 1e-4);
    assert!(alpha > 0.0 && alpha <= 1.0);
}

#[test]
fn shadow_fades_out_past_sixteen_blocks_regardless_of_opacity_scale() {
    let chunks = lit_floor_world(63);
    let position = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let far_camera = Vec3::new(8.0, 64.125 + 20.0, 8.0);

    assert!(
        place_shadow(
            &Shadow::DROPPED_ITEM,
            position,
            &EntitySize::DROPPED_ITEM,
            far_camera,
            &chunks,
            0,
        )
        .is_none()
    );
}

#[test]
fn no_shadow_without_solid_ground_beneath_the_item() {
    let chunks = world_with(Chunk::new());
    let position = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let camera = Vec3::new(8.0, 70.0, 8.0);

    assert!(
        place_shadow(
            &Shadow::DROPPED_ITEM,
            position,
            &EntitySize::DROPPED_ITEM,
            camera,
            &chunks,
            0,
        )
        .is_none()
    );
}

#[test]
fn no_shadow_on_ground_darker_than_beta_light_level_three() {
    let chunks = dark_floor_world(63);
    let position = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let camera = Vec3::new(8.0, 70.0, 8.0);

    assert!(
        place_shadow(
            &Shadow::DROPPED_ITEM,
            position,
            &EntitySize::DROPPED_ITEM,
            camera,
            &chunks,
            0,
        )
        .is_none()
    );
}

#[test]
fn shadow_weakens_as_the_item_rises_above_the_ground() {
    let chunks = lit_floor_world(63);
    let camera = Vec3::new(8.0, 70.0, 8.0);
    let grounded = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let hovering = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset + 0.2);

    let (_, grounded_alpha) = place_shadow(
        &Shadow::DROPPED_ITEM,
        grounded,
        &EntitySize::DROPPED_ITEM,
        camera,
        &chunks,
        0,
    )
    .expect("grounded item should cast a shadow");
    let (_, hovering_alpha) = place_shadow(
        &Shadow::DROPPED_ITEM,
        hovering,
        &EntitySize::DROPPED_ITEM,
        camera,
        &chunks,
        0,
    )
    .expect("slightly raised item should still cast a shadow");

    assert!(hovering_alpha < grounded_alpha);
}

#[test]
fn full_night_skylight_subtraction_can_still_dim_a_shadow_to_nothing() {
    let chunks = lit_floor_world(63);
    let position = item_position(64.0 + EntitySize::DROPPED_ITEM.y_offset);
    let camera = Vec3::new(8.0, 70.0, 8.0);

    // `combined_light` saturates sky light down by `skylight_subtracted`;
    // subtracting the full 15 levels leaves an untorched cell at 0.
    assert!(
        place_shadow(
            &Shadow::DROPPED_ITEM,
            position,
            &EntitySize::DROPPED_ITEM,
            camera,
            &chunks,
            15,
        )
        .is_none()
    );
}

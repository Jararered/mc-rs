use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game::entity::CollisionState;
use game::entity::EntitySize;
use game::entity::Gravity;
use game::entity::Velocity;
use game::physics::Aabb;
use game::physics::PhysicsPlugin;
use game::physics::move_entity;
use game::player::Player;
use game::world::block::block::BlockId;
use game::world::block::properties::blocks_movement;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;

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
    }
}

fn floor_world(floor_y: usize) -> WorldChunks {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, floor_y, z, BlockId::Stone);
        }
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated(chunk));
    chunks
}

#[test]
fn air_and_water_do_not_block_movement() {
    assert!(!blocks_movement(BlockId::Air));
    assert!(!blocks_movement(BlockId::Water));
    assert!(blocks_movement(BlockId::Stone));
    assert!(blocks_movement(BlockId::Leaves));
    assert!(blocks_movement(BlockId::Ice));
}

#[test]
fn aabb_clips_motion_against_a_touching_box() {
    let wall = Aabb::from_block(1, 1, 0);
    let body = Aabb::new(Vec3::new(0.2, 1.0, 0.2), Vec3::new(0.8, 2.8, 0.8));
    assert!((wall.calculate_x_offset(body, 1.0) - 0.2).abs() < 1e-5);
    assert!((wall.calculate_x_offset(body, 0.1) - 0.1).abs() < 1e-5);

    let floor = Aabb::from_block(0, 0, 0);
    assert_eq!(floor.calculate_y_offset(body, -2.0), 0.0);
    assert_eq!(floor.calculate_y_offset(body, 0.5), 0.5);
}

#[test]
fn falling_lands_on_a_stone_floor() {
    let chunks = floor_world(64);
    let size = EntitySize::PLAYER;
    let start = Vec3::new(8.0, 70.0, 8.0);
    let mut aabb = size.aabb(start);
    let mut on_ground = false;
    for _ in 0..20 {
        let movement = move_entity(aabb, Vec3::new(0.0, -1.0, 0.0), 0.5, on_ground, &chunks);
        aabb = movement.aabb;
        on_ground = movement.collision.on_ground;
        if on_ground {
            break;
        }
    }
    assert!(on_ground);
    assert!((aabb.min.y - 65.0).abs() < 1e-4);
    let position = size.position_from_aabb(aabb);
    assert!((position.y - (65.0 + size.y_offset)).abs() < 1e-4);
}

#[test]
fn walking_stops_at_a_wall() {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, 64, z, BlockId::Stone);
        }
    }
    chunk.set(10, 65, 8, BlockId::Stone);
    chunk.set(10, 66, 8, BlockId::Stone);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated(chunk));

    let size = EntitySize::PLAYER;
    let mut aabb = size.aabb(Vec3::new(8.0, 65.0 + size.y_offset, 8.5));
    let movement = move_entity(aabb, Vec3::new(4.0, 0.0, 0.0), 0.5, true, &chunks);
    aabb = movement.aabb;
    assert!(movement.collision.collided_x);
    assert!(aabb.max.x <= 10.0 + 1e-4);
    assert!(aabb.max.x > 9.0);
}

#[test]
fn water_does_not_stop_a_falling_body() {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, 60, z, BlockId::Stone);
            chunk.set(x, 61, z, BlockId::Water);
            chunk.set(x, 62, z, BlockId::Water);
        }
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated(chunk));

    let size = EntitySize::PLAYER;
    let mut aabb = size.aabb(Vec3::new(8.0, 64.0, 8.0));
    let mut on_ground = false;
    for _ in 0..20 {
        let movement = move_entity(aabb, Vec3::new(0.0, -1.0, 0.0), 0.5, on_ground, &chunks);
        aabb = movement.aabb;
        on_ground = movement.collision.on_ground;
        if on_ground {
            break;
        }
    }
    assert!(on_ground);
    assert!((aabb.min.y - 61.0).abs() < 1e-4);
}

#[test]
fn physics_plugin_applies_gravity_until_the_player_lands() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);

    let eye = 72.0;
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.0, eye, 8.0),
        Velocity(Vec3::ZERO),
        EntitySize::PLAYER,
        CollisionState::default(),
        Gravity::DEFAULT,
    ));

    let mut landed_y = None;
    for _ in 0..40 {
        app.update();
        let mut query = app
            .world_mut()
            .query::<(&Transform, &CollisionState, &Velocity)>();
        let (transform, collision, velocity) = query.single(app.world()).unwrap();
        if collision.on_ground {
            landed_y = Some(transform.translation.y);
            assert_eq!(velocity.0.y, 0.0);
            break;
        }
    }

    let y = landed_y.expect("player should land on the stone floor");
    assert!((y - (65.0 + EntitySize::PLAYER.y_offset)).abs() < 0.05);
}

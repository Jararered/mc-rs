use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use game::block::blocks::Block;
use game::block::direction::Direction;
use game::entity::CollisionState;
use game::entity::EntitySize;
use game::entity::Gravity;
use game::entity::Velocity;
use game::physics::Aabb;
use game::physics::BLOCK_REACH;
use game::physics::BlockFace;
use game::physics::PhysicsPlugin;
use game::physics::block_hit_distance;
use game::physics::colliding_aabbs;
use game::physics::move_entity;
use game::physics::move_entity_with_sneak;
use game::physics::raycast_blocks;
use game::physics::water_current;
use game::player::Player;
use game::player::PlayerMovementInput;
use game::player::PlayerSurvival;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::tick::WorldTick;

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

fn floor_world(floor_y: usize) -> WorldChunks {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, floor_y, z, Block::Stone);
        }
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    chunks
}

fn fluid_world(fluid: Block) -> WorldChunks {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, 64, z, Block::Stone);
            for y in 65..69 {
                chunk.set(x, y, z, fluid);
            }
        }
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    chunks
}

fn player_fluid_tick(fluid: Block, velocity: Vec3, jumping: bool) -> (Vec3, Vec3) {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(fluid_world(fluid))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity(velocity),
        EntitySize::PLAYER,
        CollisionState::default(),
        PlayerMovementInput {
            jumping,
            ..default()
        },
    ));
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    let mut query = app.world_mut().query::<(&Transform, &Velocity)>();
    let (transform, velocity) = query.single(app.world()).unwrap();
    (transform.translation, velocity.0)
}

#[test]
fn air_and_water_do_not_block_movement() {
    assert!(!Block::Air.blocks_movement());
    assert!(!Block::Water.blocks_movement());
    assert!(Block::Stone.blocks_movement());
    assert!(Block::Leaves.blocks_movement());
    assert!(Block::Ice.blocks_movement());
}

#[test]
fn ladder_collision_is_a_thin_plate_on_the_supporting_wall() {
    assert_eq!(
        Block::Ladder.collision_bounds_for(Block::Ladder.facing_metadata(Direction::West)),
        Some(([0.0, 0.0, 0.0], [0.125, 1.0, 1.0]))
    );
    assert_eq!(
        Block::Ladder.collision_bounds_for(Block::Ladder.facing_metadata(Direction::South)),
        Some(([0.0, 0.0, 0.875], [1.0, 1.0, 1.0]))
    );
}

#[test]
fn horizontal_collision_with_a_ladder_starts_a_climb() {
    let mut chunk = Chunk::new();
    for y in 65..70 {
        chunk.set(8, y, 8, Block::Stone);
        chunk.set_with_metadata(
            9,
            y,
            8,
            Block::Ladder,
            Block::Ladder.facing_metadata(Direction::West),
        );
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(chunks)
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(9.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity(Vec3::new(-4.0, 0.0, 0.0)),
        EntitySize::PLAYER,
        CollisionState::default(),
        PlayerMovementInput::default(),
    ));

    for _ in 0..2 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
    }
    let mut query = app.world_mut().query::<&Transform>();
    let transform = query.single(app.world()).unwrap();
    assert!(transform.translation.y > 65.0 + EntitySize::PLAYER.y_offset);
}

#[test]
fn fluids_are_replaceable_and_bedrock_is_unbreakable() {
    assert!(!Block::Air.is_targetable());
    assert!(!Block::Water.is_targetable());
    assert!(Block::Stone.is_targetable());
    assert!(Block::Bedrock.is_targetable());
    assert!(Block::Air.is_replaceable());
    assert!(Block::Water.is_replaceable());
    assert!(!Block::Stone.is_replaceable());
    assert!(Block::Stone.is_breakable());
    assert!(Block::Leaves.is_breakable());
    assert!(!Block::Bedrock.is_breakable());
    assert!(!Block::Water.is_breakable());
}

#[test]
fn raycast_only_hits_the_torch_near_its_visible_shaft() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Torch);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    assert_eq!(
        Block::Torch.selection_bounds_for(0),
        ([0.4, 0.0, 0.4], [0.6, 0.625, 0.6])
    );
    assert!(
        raycast_blocks(&chunks, Vec3::new(8.5, 64.4, 7.0), Vec3::Z, BLOCK_REACH)
            .is_some_and(|hit| hit.block == Block::Torch)
    );
    assert!(raycast_blocks(&chunks, Vec3::new(8.8, 64.4, 7.0), Vec3::Z, BLOCK_REACH).is_none());
    assert!(raycast_blocks(&chunks, Vec3::new(8.5, 64.9, 7.0), Vec3::Z, BLOCK_REACH).is_none());
}

#[test]
fn wall_redstone_torches_are_targetable_at_their_actual_mounting_side() {
    for block in [Block::RedstoneTorch, Block::UnlitRedstoneTorch] {
        for (metadata, near, direction) in [
            (1, Vec3::new(7.0, 64.5, 8.5), Vec3::X),
            (2, Vec3::new(10.0, 64.5, 8.5), Vec3::NEG_X),
            (3, Vec3::new(8.5, 64.5, 7.0), Vec3::Z),
            (4, Vec3::new(8.5, 64.5, 10.0), Vec3::NEG_Z),
        ] {
            let mut chunk = Chunk::new();
            chunk.set(8, 64, 8, block);
            chunk.set_metadata(8, 64, 8, metadata);
            let mut chunks = WorldChunks::default();
            chunks.insert(ChunkPosition::ZERO, generated(chunk));
            // A redstone torch is shaped like the plain torch it replaces.
            assert_eq!(
                block.selection_bounds_for(metadata),
                Block::Torch.selection_bounds_for(metadata)
            );
            assert!(
                raycast_blocks(&chunks, near, direction, BLOCK_REACH)
                    .is_some_and(|hit| hit.block == block),
                "{block:?} {metadata}"
            );
            let (off_side, across) = if metadata <= 2 {
                (Vec3::new(8.5, 64.5, 7.0), Vec3::Z)
            } else {
                (Vec3::new(7.0, 64.5, 8.5), Vec3::X)
            };
            assert!(raycast_blocks(&chunks, off_side, across, BLOCK_REACH).is_none());
        }
    }
}

#[test]
fn an_extended_piston_and_its_head_collide_only_where_they_are_solid() {
    let mut chunk = Chunk::new();
    // A base facing east, extended, with its head in the next cell.
    chunk.set(8, 64, 8, Block::Piston);
    chunk.set_metadata(8, 64, 8, 5 | 8);
    chunk.set(9, 64, 8, Block::PistonHead);
    chunk.set_metadata(9, 64, 8, 5);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    assert_eq!(
        Block::Piston.collision_bounds_for(5 | 8),
        Some(([0.0, 0.0, 0.0], [0.75, 1.0, 1.0]))
    );
    assert_eq!(
        Block::PistonHead.collision_bounds_for(5),
        Some(([0.75, 0.0, 0.0], [1.0, 1.0, 1.0]))
    );
    // The recess of the base is empty.
    let recess = Aabb::new(Vec3::new(8.8, 64.2, 8.2), Vec3::new(8.95, 64.8, 8.8));
    assert!(colliding_aabbs(&chunks, recess).is_empty());
    let plate = Aabb::new(Vec3::new(9.8, 64.2, 8.2), Vec3::new(9.95, 64.8, 8.8));
    assert_eq!(colliding_aabbs(&chunks, plate).len(), 1);
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
fn an_item_sunk_into_the_floor_keeps_falling() {
    let chunks = floor_world(64);
    let size = EntitySize::DROPPED_ITEM;
    let resting = size.aabb(Vec3::new(8.0, 65.125, 8.0));
    let sunk = resting.offset(Vec3::new(0.0, -0.04, 0.0));
    let movement = move_entity(sunk, Vec3::new(0.0, -0.2, 0.0), 0.0, true, &chunks);
    assert!(movement.aabb.min.y < 65.0 - 1e-4);
    assert!(!movement.collision.on_ground);
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
fn player_remains_grounded_at_float_rounding_heights() {
    let size = EntitySize::PLAYER;
    for floor_top in [31.0_f32, 32.0] {
        let chunks = floor_world(floor_top as usize - 1);
        let mut eye = Vec3::new(8.5, floor_top + size.y_offset, 8.5);
        for _ in 0..4 {
            let movement = move_entity(
                size.aabb(eye),
                Vec3::new(0.0, -0.04, 0.0),
                0.5,
                true,
                &chunks,
            );
            eye = size.position_from_aabb(movement.aabb);
            assert!(
                movement.collision.on_ground,
                "lost ground at floor Y={floor_top}, eye Y={}",
                eye.y
            );
            assert!((movement.aabb.min.y - floor_top).abs() < 1e-4);
        }
    }
}

#[test]
fn walking_stops_at_a_wall() {
    let mut chunk = Chunk::new();
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            chunk.set(x, 64, z, Block::Stone);
        }
    }
    chunk.set(10, 65, 8, Block::Stone);
    chunk.set(10, 66, 8, Block::Stone);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

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
            chunk.set(x, 60, z, Block::Stone);
            chunk.set(x, 61, z, Block::Water);
            chunk.set(x, 62, z, Block::Water);
        }
    }
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

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
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
        let mut query = app
            .world_mut()
            .query::<(&Transform, &CollisionState, &Velocity)>();
        let (transform, collision, velocity) = query.single(app.world()).unwrap();
        if collision.on_ground {
            landed_y = Some(transform.translation.y);
            assert!(velocity.0.y <= 0.0);
            break;
        }
    }

    let y = landed_y.expect("player should land on the stone floor");
    assert!((y - (65.0 + EntitySize::PLAYER.y_offset)).abs() < 0.05);
}

#[test]
fn player_accelerates_from_rest_instead_of_receiving_instant_speed() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity::default(),
        EntitySize::PLAYER,
        CollisionState {
            on_ground: true,
            ..default()
        },
        PlayerMovementInput {
            forward: 1.0,
            ..default()
        },
    ));

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();

    let mut query = app.world_mut().query::<&Velocity>();
    let speed = query.single(app.world()).unwrap().0.length();
    assert!(speed > 0.0);
    assert!(speed < 4.317);
}

#[test]
fn held_jump_uses_beta_impulse_and_gravity_per_tick() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity::default(),
        EntitySize::PLAYER,
        CollisionState {
            on_ground: true,
            ..default()
        },
        PlayerMovementInput {
            jumping: true,
            ..default()
        },
    ));

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();

    let mut query = app
        .world_mut()
        .query::<(&Transform, &Velocity, &CollisionState)>();
    let (transform, velocity, collision) = query.single(app.world()).unwrap();
    assert!((transform.translation.y - (65.0 + EntitySize::PLAYER.y_offset + 0.42)).abs() < 1e-4);
    assert!((velocity.0.y - ((0.42 - 0.08) * 0.98 * 20.0)).abs() < 1e-3);
    assert!(!collision.on_ground);
}

#[test]
fn water_and_lava_apply_their_beta_drag_and_swimming_jump() {
    let (_, water_motion) = player_fluid_tick(Block::Water, Vec3::new(2.0, 0.0, 0.0), false);
    let (_, lava_motion) = player_fluid_tick(Block::Lava, Vec3::new(2.0, 0.0, 0.0), false);
    assert!((water_motion.x - 1.6).abs() < 1e-4);
    assert!((lava_motion.x - 1.0).abs() < 1e-4);

    let (water_position, water_jump) = player_fluid_tick(Block::Water, Vec3::ZERO, true);
    let (lava_position, lava_jump) = player_fluid_tick(Block::Lava, Vec3::ZERO, true);
    assert!(water_position.y > 65.0 + EntitySize::PLAYER.y_offset);
    assert!(lava_position.y > 65.0 + EntitySize::PLAYER.y_offset);
    assert!((water_jump.y - 0.24).abs() < 1e-4);
    assert!(lava_jump.y.abs() < 1e-4);
}

#[test]
fn holding_jump_repeats_after_landing() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity::default(),
        EntitySize::PLAYER,
        CollisionState {
            on_ground: true,
            ..default()
        },
        PlayerMovementInput {
            jumping: true,
            ..default()
        },
    ));

    for _ in 0..15 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
    }

    let mut query = app.world_mut().query::<(&Transform, &CollisionState)>();
    let (transform, collision) = query.single(app.world()).unwrap();
    assert!(transform.translation.y > 65.0 + EntitySize::PLAYER.y_offset + 0.2);
    assert!(!collision.on_ground);
}

#[test]
fn ground_friction_depends_on_surface_slipperiness() {
    fn final_horizontal_speed(surface: Block) -> f32 {
        let mut chunk = Chunk::new();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                chunk.set(x, 64, z, Block::Stone);
            }
        }
        chunk.set(8, 64, 8, surface);
        let mut chunks = WorldChunks::default();
        chunks.insert(ChunkPosition::ZERO, generated(chunk));
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(chunks)
            .add_plugins(PhysicsPlugin);
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
            Velocity(Vec3::new(2.0, -0.2, 0.0)),
            EntitySize::PLAYER,
            CollisionState {
                on_ground: true,
                ..default()
            },
        ));
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
        let mut query = app.world_mut().query::<&Velocity>();
        query.single(app.world()).unwrap().0.x
    }

    let stone_speed = final_horizontal_speed(Block::Stone);
    let ice_speed = final_horizontal_speed(Block::Ice);
    assert!(ice_speed > stone_speed * 1.5);
}

#[test]
fn sprint_accelerates_toward_its_faster_target_without_a_velocity_jump() {
    fn first_tick_speed(sprinting: bool) -> f32 {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(floor_world(64))
            .add_plugins(PhysicsPlugin);
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
            Velocity::default(),
            EntitySize::PLAYER,
            CollisionState {
                on_ground: true,
                ..default()
            },
            PlayerMovementInput {
                forward: 1.0,
                sprinting,
                ..default()
            },
        ));
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
        let mut query = app.world_mut().query::<&Velocity>();
        query.single(app.world()).unwrap().0.xz().length()
    }

    let walking_speed = first_tick_speed(false);
    let sprint_speed = first_tick_speed(true);
    assert!(sprint_speed > walking_speed);
    assert!(sprint_speed < 5.612);
}

#[test]
fn player_has_air_control_and_momentum_decays_without_input() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);
    let player = app
        .world_mut()
        .spawn((
            Player,
            Transform::from_xyz(8.5, 70.0, 8.5),
            Velocity::default(),
            EntitySize::PLAYER,
            CollisionState::default(),
            PlayerMovementInput {
                forward: 1.0,
                ..default()
            },
        ))
        .id();

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    let first_speed = {
        let mut query = app.world_mut().query::<&Velocity>();
        query.single(app.world()).unwrap().0.xz().length()
    };
    assert!(first_speed > 0.0 && first_speed < 0.5);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<PlayerMovementInput>()
        .unwrap()
        .forward = 0.0;

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    let mut query = app.world_mut().query::<&Velocity>();
    let velocity = query.single(app.world()).unwrap();
    assert!(velocity.0.xz().length() < first_speed);
}

#[test]
fn player_steps_onto_low_obstacles_but_not_full_blocks() {
    fn move_over(obstacle: Block, metadata: u8) -> game::physics::Movement {
        let mut chunk = Chunk::new();
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                chunk.set(x, 64, z, Block::Stone);
            }
        }
        chunk.set_with_metadata(9, 65, 8, obstacle, metadata);
        let mut chunks = WorldChunks::default();
        chunks.insert(ChunkPosition::ZERO, generated(chunk));
        let aabb = EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));
        move_entity(aabb, Vec3::new(0.5, 0.0, 0.0), 0.5, true, &chunks)
    }

    let snow_step = move_over(Block::SnowLayer, 0);
    assert!(snow_step.displacement.x > 0.2);
    assert!((snow_step.aabb.min.y - 65.0).abs() < 1e-4);

    let deep_snow = move_over(Block::SnowLayer, 3);
    assert!(deep_snow.displacement.x > 0.2);
    assert!((deep_snow.aabb.min.y - 65.5).abs() < 1e-4);

    let full_block = move_over(Block::Stone, 0);
    assert!(full_block.displacement.x <= 0.2 + 1e-4);
}

#[test]
fn sneaking_brakes_at_the_edge_of_supported_ground() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    let chunks = generated(chunk);
    let mut world = WorldChunks::default();
    world.insert(ChunkPosition::ZERO, chunks);
    let aabb = EntitySize::PLAYER.aabb(Vec3::new(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5));

    let movement = move_entity_with_sneak(aabb, Vec3::new(0.9, 0.0, 0.0), 0.5, true, true, &world);
    assert!(movement.displacement.x > 0.0);
    assert!(movement.displacement.x < 0.9);
    assert!(!movement.collision.collided_x);
}

#[test]
fn raycast_hits_the_top_of_a_stone_block() {
    let chunks = floor_world(64);
    let hit = raycast_blocks(&chunks, Vec3::new(8.5, 66.5, 8.5), Vec3::NEG_Y, BLOCK_REACH)
        .expect("should hit the floor");
    assert_eq!(hit.x, 8);
    assert_eq!(hit.y, 64);
    assert_eq!(hit.z, 8);
    assert_eq!(hit.face, BlockFace::Up);
    assert_eq!(hit.block, Block::Stone);
}

#[test]
fn raycast_reports_the_face_the_ray_entered() {
    let mut chunk = Chunk::new();
    chunk.set(10, 65, 8, Block::Dirt);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    let hit = raycast_blocks(&chunks, Vec3::new(8.5, 65.5, 8.5), Vec3::X, BLOCK_REACH)
        .expect("should hit the wall");
    assert_eq!((hit.x, hit.y, hit.z), (10, 65, 8));
    assert_eq!(hit.face, BlockFace::West);
    assert_eq!(hit.block, Block::Dirt);
}

#[test]
fn raycast_skips_water_and_hits_the_block_behind_it() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(8, 65, 8, Block::Water);
    chunk.set(8, 66, 8, Block::Water);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    let hit = raycast_blocks(&chunks, Vec3::new(8.5, 68.0, 8.5), Vec3::NEG_Y, BLOCK_REACH)
        .expect("should pass through water");
    assert_eq!((hit.x, hit.y, hit.z), (8, 64, 8));
    assert_eq!(hit.block, Block::Stone);
}

#[test]
fn raycast_misses_when_nothing_is_in_range() {
    let chunks = WorldChunks::default();
    assert!(raycast_blocks(&chunks, Vec3::new(8.5, 70.0, 8.5), Vec3::NEG_Y, 4.0).is_none());
}

fn current_world() -> WorldChunks {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(9, 64, 8, Block::Stone);
    chunk.set(8, 65, 8, Block::Water);
    chunk.set_with_metadata(9, 65, 8, Block::FlowingWater, 1);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));
    chunks
}

#[test]
fn water_current_pushes_submerged_small_bodies_but_not_dry_ones() {
    let chunks = current_world();
    let immersed = EntitySize::DROPPED_ITEM.aabb(Vec3::new(8.5, 65.5, 8.5));
    assert_eq!(water_current(immersed, &chunks), (true, Vec3::X));
    let dry = EntitySize::DROPPED_ITEM.aabb(Vec3::new(8.5, 66.5, 8.5));
    assert_eq!(water_current(dry, &chunks), (false, Vec3::ZERO));
}

#[test]
fn water_current_pushes_player_and_generic_physics_bodies() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            0.05,
        )))
        .insert_resource(current_world())
        .add_plugins(PhysicsPlugin);
    let player = app
        .world_mut()
        .spawn((
            Player,
            Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
            Velocity(Vec3::ZERO),
            EntitySize::PLAYER,
            CollisionState::default(),
            PlayerMovementInput::default(),
        ))
        .id();
    let body = app
        .world_mut()
        .spawn((
            Transform::from_xyz(8.5, 65.5, 8.5),
            Velocity(Vec3::ZERO),
            EntitySize::DROPPED_ITEM,
            CollisionState::default(),
        ))
        .id();
    app.update(); // Initialize frame time without advancing the world tick.
    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    let player_velocity = app.world().entity(player).get::<Velocity>().unwrap().0;
    let body_velocity = app.world().entity(body).get::<Velocity>().unwrap().0;
    assert!(player_velocity.x > 0.0);
    assert!((body_velocity.x - 0.28).abs() < 1e-4);

    app.world_mut().resource_mut::<WorldTick>().advance(0.1); // two ticks
    app.update();
    let body_velocity = app.world().entity(body).get::<Velocity>().unwrap().0;
    assert!((body_velocity.x - 0.84).abs() < 1e-4);
}

#[test]
fn raycast_passes_through_the_empty_part_of_a_partial_block() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::StoneSlab);
    chunk.set(8, 64, 10, Block::Rose);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    // Over the slab's upper half, and past the flower beside its stem.
    assert!(raycast_blocks(&chunks, Vec3::new(8.5, 64.75, 6.0), Vec3::Z, 2.9).is_none());
    assert!(raycast_blocks(&chunks, Vec3::new(8.1, 64.2, 9.0), Vec3::Z, 2.5).is_none());

    let origin = Vec3::new(8.5, 64.25, 6.0);
    let hit = raycast_blocks(&chunks, origin, Vec3::Z, BLOCK_REACH).unwrap();
    assert_eq!((hit.block, hit.face), (Block::StoneSlab, BlockFace::North));
    assert!((block_hit_distance(&chunks, &hit, origin, Vec3::Z) - 2.0).abs() < 1e-5);
}

#[test]
fn raycast_reports_the_face_of_the_box_not_of_the_cell() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::StoneSlab);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    // Enters the cell through its north side, above the slab, and lands on top.
    let origin = Vec3::new(8.5, 65.2, 7.5);
    let direction = Vec3::new(0.0, -1.0, 1.0);
    let hit = raycast_blocks(&chunks, origin, direction, BLOCK_REACH).unwrap();
    assert_eq!((hit.block, hit.face), (Block::StoneSlab, BlockFace::Up));
}

#[test]
fn the_tick_that_jumps_is_still_slowed_by_the_ground_it_left() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(floor_world(64))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, 65.0 + EntitySize::PLAYER.y_offset, 8.5),
        Velocity(Vec3::new(2.0, 0.0, 0.0)),
        EntitySize::PLAYER,
        CollisionState {
            on_ground: true,
            ..default()
        },
        PlayerMovementInput {
            jumping: true,
            ..default()
        },
    ));

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();

    let mut query = app.world_mut().query::<(&Velocity, &CollisionState)>();
    let (velocity, collision) = query.single(app.world()).unwrap();
    assert!(!collision.on_ground);
    assert!((velocity.0.x - 2.0 * 0.6 * 0.91).abs() < 1e-4);
}

/// Drop a player from `feet_y` over [`fluid_world`] and return its fall state
/// after `ticks`.
fn dropped_player(fluid: Block, feet_y: f32, ticks: u32) -> PlayerSurvival {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(fluid_world(fluid))
        .add_plugins(PhysicsPlugin);
    app.world_mut().spawn((
        Player,
        Transform::from_xyz(8.5, feet_y + EntitySize::PLAYER.y_offset, 8.5),
        EntitySize::PLAYER,
    ));
    for _ in 0..ticks {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
    }
    let mut query = app.world_mut().query::<&PlayerSurvival>();
    *query.single(app.world()).unwrap()
}

#[test]
fn a_fall_is_measured_until_the_player_lands() {
    let falling = dropped_player(Block::Air, 75.0, 10);
    assert!(falling.fall_distance > 1.0, "{falling:?}");
    assert_eq!(falling.landed, 0.0);

    // Ten blocks down onto the stone at y = 64.
    let landed = dropped_player(Block::Air, 75.0, 60);
    assert_eq!(landed.fall_distance, 0.0);
    assert!((9.0..=10.0).contains(&landed.landed), "{landed:?}");
}

#[test]
fn water_breaks_a_fall() {
    let landed = dropped_player(Block::Water, 80.0, 200);
    assert_eq!(landed.fall_distance, 0.0);
    assert_eq!(landed.landed, 0.0);
}

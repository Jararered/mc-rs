//! Integration test: boot the engine and generate a single block.
//!
//! The block is built from Bevy's [`Cuboid`] primitive, which is the shape a
//! voxel block is made of. The engine is started headlessly (no window or
//! renderer) so the test can run in CI.

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;

use game::block::block::BlockId;
use game::block::definition::BlockProperties;
use game::block::definition::{self};

/// Edge length of a single block, in world units.
const BLOCK_SIZE: f32 = 1.0;

/// Start the engine with just the plugins needed to build and store meshes.
fn start_engine() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin));
    app
}

/// Generate a single block mesh from a [`Cuboid`] and return its handle.
fn generate_block(app: &mut App) -> Handle<Mesh> {
    app.world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(BLOCK_SIZE, BLOCK_SIZE, BLOCK_SIZE))
}

#[test]
fn engine_generates_a_single_cuboid_block() {
    let mut app = start_engine();

    let block_mesh = generate_block(&mut app);

    // Spawn the block into the world and run one frame of the engine.
    let block = app
        .world_mut()
        .spawn((Mesh3d(block_mesh.clone()), Transform::default()))
        .id();
    app.update();

    // The block entity exists and points at a real mesh.
    assert!(app.world().entity(block).contains::<Mesh3d>());

    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&block_mesh).expect("block mesh should exist");

    // A cuboid is built from 4 vertices per face across 6 faces.
    assert_eq!(mesh.count_vertices(), 24);
}

#[test]
fn every_known_block_value_resolves_to_its_static_definition() {
    let mut found = 0;
    for raw in 0..=u8::MAX {
        let Some(block) = BlockId::from_u8(raw) else {
            continue;
        };
        found += 1;

        let behavior = definition::definition(block);
        let properties = behavior.properties(block);
        assert_eq!(definition::properties(block), properties, "{block:?}");
        assert_eq!(behavior.name(block), block.name());
        assert_ne!(behavior.name(block), "unknown", "{block:?}");
        assert_eq!(behavior.in_world(block), block.in_world());
        assert_eq!(BlockId::from_u8(block.as_u8()), Some(block));
        assert_eq!(
            behavior.opaque_cube(block),
            properties.opaque_cube,
            "{block:?}"
        );
        assert_eq!(
            behavior.light_opacity(block),
            properties.light_opacity,
            "{block:?}"
        );
        assert_eq!(
            behavior.light_emission(block),
            properties.light_emission,
            "{block:?}"
        );
        assert_eq!(
            behavior.crossed_plant(block),
            properties.crossed_plant,
            "{block:?}"
        );
        assert_eq!(behavior.torch(block), properties.torch, "{block:?}");
        assert!(properties.hardness.is_finite(), "{block:?}");
        assert!(properties.slipperiness.is_finite(), "{block:?}");
        assert!(properties.light_opacity <= 15, "{block:?}");
        assert!(properties.light_emission <= 15, "{block:?}");
    }

    assert_eq!(found, 128);
}

#[test]
fn unknown_block_values_use_a_safe_fallback_definition() {
    for unknown in [BlockId::Unknown(180), BlockId::Unknown(201)] {
        let behavior = definition::definition(unknown);
        let properties = behavior.properties(unknown);

        assert_eq!(behavior.name(unknown), "unknown");
        assert!(!behavior.in_world(unknown));
        assert_eq!(properties.hardness, 0.0);
        assert_eq!(
            properties.collision_bounds,
            Some(BlockProperties::FULL_BOUNDS)
        );
    }
}

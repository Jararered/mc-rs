//! Integration test: boot the engine and generate a single block.
//!
//! The block is built from Bevy's [`Cuboid`] primitive, which is the shape a
//! voxel block is made of. The engine is started headlessly (no window or
//! renderer) so the test can run in CI.

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;

use game::block::blocks::Block;
use game::block::blocks::species;
use game::block::definition;
use game::block::definition::BlockProperties;
use game::block::direction::Direction;

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
fn every_known_block_value_resolves_to_its_static_properties() {
    let mut found = 0;
    for raw in 0..=u8::MAX {
        let Some(block) = Block::from_u8(raw) else {
            continue;
        };
        found += 1;

        let properties = definition::properties(block);
        assert_eq!(Block::from_u8(block.as_u8()), Some(block));
        assert_eq!(block.is_opaque_cube(), properties.opaque_cube, "{block:?}");
        assert_eq!(
            block.blocks_movement(),
            properties.blocks_movement,
            "{block:?}"
        );
        assert_eq!(block.slipperiness(), properties.slipperiness, "{block:?}");
        assert_eq!(
            block.collision_bounds(),
            properties.collision_bounds,
            "{block:?}"
        );
        assert_eq!(
            block.selection_bounds(),
            properties.selection_bounds,
            "{block:?}"
        );
        assert_eq!(
            block.is_crossed_plant(),
            properties.crossed_plant,
            "{block:?}"
        );
        assert_eq!(block.light_opacity(), properties.light_opacity, "{block:?}");
        assert_eq!(
            block.light_emission(),
            properties.light_emission,
            "{block:?}"
        );
        assert_eq!(
            block.harvestable_by_hand(),
            properties.harvestable_by_hand,
            "{block:?}"
        );
        assert!(properties.hardness.is_finite(), "{block:?}");
        assert!(properties.slipperiness.is_finite(), "{block:?}");
        assert!(properties.light_opacity <= 15, "{block:?}");
        assert!(properties.light_emission <= 15, "{block:?}");
        // Only Beta's own ids are block values; state lives in metadata.
        assert!(raw <= Block::MAX_ITEM_ID, "{block:?}");
    }

    assert_eq!(found, 97);
}

#[test]
fn unknown_block_values_use_a_safe_fallback_definition() {
    for unknown in [Block::Unknown(180), Block::Unknown(201)] {
        let properties = definition::properties(unknown);

        assert!(!unknown.in_world());
        assert_eq!(properties.hardness, 0.0);
        assert_eq!(
            properties.collision_bounds,
            Some(BlockProperties::FULL_BOUNDS)
        );
        assert_eq!(unknown.collision_bounds(), properties.collision_bounds);
        assert_eq!(unknown.light_opacity(), 15);
        assert_eq!(unknown.light_emission(), 0);
        assert!(unknown.is_solid_material());
        assert_eq!(unknown.explosion_resistance(), 0.0);
    }
    // The ids that used to encode species and facings are no longer blocks.
    for raw in 97..=u8::MAX {
        assert_eq!(Block::from_u8(raw), None, "{raw}");
    }
}

#[test]
fn block_material_and_explosion_queries_preserve_beta_values() {
    assert!(!Block::Air.is_solid_material());
    assert!(!Block::Torch.is_solid_material());
    assert!(!Block::Ladder.is_solid_material());
    assert!(Block::Glass.is_solid_material());
    assert!(Block::Ice.is_solid_material());
    assert!(Block::Grass.supports_plants());
    assert!(!Block::Sand.supports_plants());

    assert_eq!(Block::Bedrock.explosion_resistance(), 6_000_000.0_f32 * 0.6);
    assert_eq!(Block::Obsidian.explosion_resistance(), 1200.0);
    assert_eq!(Block::Stone.explosion_resistance(), 6.0);
    assert_eq!(Block::WoodenPlanks.explosion_resistance(), 3.0);
    assert_eq!(Block::Lava.explosion_resistance(), 100.0);
    assert_eq!(Block::Dirt.explosion_resistance(), Block::Dirt.hardness());
}

#[test]
fn facing_metadata_round_trips_for_every_oriented_block() {
    for block in [
        Block::Torch,
        Block::Ladder,
        Block::Furnace,
        Block::LitFurnace,
        Block::Chest,
        Block::Pumpkin,
    ] {
        for facing in Direction::ALL {
            let metadata = block.facing_metadata(facing);
            assert!(metadata <= 5, "{block:?} {facing:?}");
            assert_eq!(block.facing(metadata), Some(facing), "{block:?} {facing:?}");
        }
    }
    // Beta's values: a floor torch, an unattached ladder, and blocks without
    // an orientation have none.
    assert_eq!(Block::Torch.facing(0), None);
    assert_eq!(Block::Torch.facing(5), None);
    assert_eq!(Block::Ladder.facing(0), None);
    assert_eq!(Block::Stone.facing(3), None);
    assert_eq!(Block::Torch.facing_metadata(Direction::West), 1);
    assert_eq!(Block::Ladder.facing_metadata(Direction::South), 2);
    assert_eq!(Block::Furnace.facing_metadata(Direction::North), 2);
    // A torch or ladder hangs toward the side its metadata names.
    assert_eq!(Block::Torch.support_offset(1), Some([-1, 0, 0]));
    assert_eq!(Block::Ladder.support_offset(3), Some([0, 0, -1]));
}

#[test]
fn oriented_bounds_follow_metadata() {
    let west = Block::Ladder.facing_metadata(Direction::West);
    assert_eq!(
        Block::Ladder.collision_bounds_for(west),
        Some(([0.0, 0.0, 0.0], [0.125, 1.0, 1.0]))
    );
    assert_eq!(
        Block::Ladder.selection_bounds_for(west),
        Block::Ladder.collision_bounds_for(west).unwrap()
    );
    let floor = Block::Torch.selection_bounds_for(0);
    let wall = Block::Torch.selection_bounds_for(Block::Torch.facing_metadata(Direction::East));
    assert_ne!(floor, wall);
    assert_eq!(Block::Torch.collision_bounds_for(1), None);
}

#[test]
fn species_stay_in_metadata_and_stack_by_subtype() {
    for species in [species::OAK, species::SPRUCE, species::BIRCH] {
        for block in [Block::Wood, Block::WoodenPlanks, Block::Leaves] {
            assert_eq!(block.item_form(species), (block, species));
            // Leaf decay flags are not part of the species.
            assert_eq!(block.item_form(species | 8), (block, species));
            assert_eq!(block.placed(species), Some((block, species)));
        }
    }
    assert_eq!(
        Block::TallGrass.item_form(species::FERN),
        (Block::TallGrass, 2)
    );
    assert_eq!(Block::TallGrass.placed(1), Some((Block::TallGrass, 0)));
    assert_eq!(Block::LitFurnace.item_form(4), (Block::Furnace, 0));
    assert_eq!(Block::Torch.placed(5), Some((Block::Torch, 0)));
    assert_eq!(Block::Torch.placed(3), Some((Block::Torch, 3)));
    assert_eq!(Block::Wood.placed(3), None);
    // Species change the drawn block, but leaf decay does not.
    assert_ne!(
        Block::Leaves.appearance_metadata(1),
        Block::Leaves.appearance_metadata(2)
    );
    assert_eq!(
        Block::Leaves.appearance_metadata(1),
        Block::Leaves.appearance_metadata(1 | 8)
    );
}

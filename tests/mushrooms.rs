use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use game::block::id::Id;
use game::block::properties::collision_bounds;
use game::block::properties::is_crossed_plant;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::textures::block_tile;

#[test]
fn mushrooms_use_crossed_sprite_meshes_and_beta_atlas_tiles() {
    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Id::BrownMushroom);
    chunk.set(8, 20, 8, Id::RedMushroom);

    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        meshes.masked.attribute(Mesh::ATTRIBUTE_POSITION.id)
    else {
        panic!("mushrooms should render in the masked plant mesh");
    };
    assert_eq!(positions.len(), 16, "two crossed quads per mushroom");
    assert_eq!(block_tile(Id::BrownMushroom, 0, false), (13, 1));
    assert_eq!(block_tile(Id::RedMushroom, 0, false), (12, 1));
}

#[test]
fn mushrooms_are_small_noncolliding_nonopaque_plants() {
    for mushroom in [Id::BrownMushroom, Id::RedMushroom] {
        assert!(is_crossed_plant(mushroom));
        assert!(!is_opaque_cube(mushroom));
        assert_eq!(collision_bounds(mushroom), None);
        assert_eq!(
            selection_bounds(mushroom),
            ([0.3, 0.0, 0.3], [0.7, 0.4, 0.7])
        );
    }
}

#[test]
fn mushroom_mesh_is_raised_by_two_pixels() {
    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Id::BrownMushroom);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        meshes.masked.attribute(Mesh::ATTRIBUTE_POSITION.id)
    else {
        panic!("mushroom should render in the masked plant mesh");
    };
    let min_y = positions
        .iter()
        .map(|position| position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = positions
        .iter()
        .map(|position| position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let center_x =
        positions.iter().map(|position| position[0]).sum::<f32>() / positions.len() as f32;
    let center_z =
        positions.iter().map(|position| position[2]).sum::<f32>() / positions.len() as f32;
    assert!((min_y - 20.125).abs() < f32::EPSILON);
    assert!((max_y - 21.125).abs() < f32::EPSILON);
    assert!((center_x - 4.5).abs() < f32::EPSILON);
    assert!((center_z - 4.5).abs() < f32::EPSILON);
}

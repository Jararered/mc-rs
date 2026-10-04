use game::block::blocks::Block;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::textures::block_tile;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;

#[test]
fn mushrooms_use_crossed_sprite_meshes_and_beta_atlas_tiles() {
    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Block::BrownMushroom);
    chunk.set(8, 20, 8, Block::RedMushroom);

    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let positions = meshes.masked.positions();
    assert_eq!(positions.len(), 16, "two crossed quads per mushroom");
    assert_eq!(block_tile(Block::BrownMushroom, 0, false), (13, 1));
    assert_eq!(block_tile(Block::RedMushroom, 0, false), (12, 1));
}

#[test]
fn mushrooms_are_small_noncolliding_nonopaque_plants() {
    for mushroom in [Block::BrownMushroom, Block::RedMushroom] {
        assert!(mushroom.is_crossed_plant());
        assert!(!mushroom.is_opaque_cube());
        assert_eq!(mushroom.collision_bounds(), None);
        assert_eq!(
            mushroom.selection_bounds(),
            ([0.3, 0.0, 0.3], [0.7, 0.4, 0.7])
        );
    }
}

#[test]
fn mushroom_mesh_is_raised_by_two_pixels() {
    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Block::BrownMushroom);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    let positions = meshes.masked.positions();
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

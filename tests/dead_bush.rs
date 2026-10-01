use game::block::id::Id;
use game::block::properties::collision_bounds;
use game::block::properties::is_crossed_plant;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::textures::atlas_tile_uvs;
use game::rendering::textures::block_tile;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;

#[test]
fn dead_bush_uses_its_beta_sprite_tile_and_crossed_mesh() {
    assert_eq!(block_tile(Id::DeadBush, 0, false), (7, 3));
    assert!(is_crossed_plant(Id::DeadBush));
    assert!(!is_opaque_cube(Id::DeadBush));
    assert_eq!(collision_bounds(Id::DeadBush), None);
    assert_eq!(
        selection_bounds(Id::DeadBush),
        ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9])
    );

    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, Id::DeadBush);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true);
    assert_eq!(meshes.opaque.vertex_count(), 0);
    let positions = meshes.masked.positions();
    assert_eq!(positions.len(), 8);

    let uvs = meshes.masked.uvs();
    let (u0, v0, u1, v1) = atlas_tile_uvs(7, 3);
    assert_eq!(
        (
            uvs.iter().map(|uv| uv[0]).fold(f32::INFINITY, f32::min),
            uvs.iter().map(|uv| uv[1]).fold(f32::INFINITY, f32::min),
            uvs.iter().map(|uv| uv[0]).fold(f32::NEG_INFINITY, f32::max),
            uvs.iter().map(|uv| uv[1]).fold(f32::NEG_INFINITY, f32::max),
        ),
        (u0, v0, u1, v1)
    );
}

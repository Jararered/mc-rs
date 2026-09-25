use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use game::block::block::BlockId;
use game::block::properties::collision_bounds;
use game::block::properties::is_crossed_plant;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::textures::atlas_tile_uvs;
use game::world::textures::block_tile;

#[test]
fn dead_bush_uses_its_beta_sprite_tile_and_crossed_mesh() {
    assert_eq!(block_tile(BlockId::DeadBush, 0, false), (7, 3));
    assert!(is_crossed_plant(BlockId::DeadBush));
    assert!(!is_opaque_cube(BlockId::DeadBush));
    assert_eq!(collision_bounds(BlockId::DeadBush), None);
    assert_eq!(
        selection_bounds(BlockId::DeadBush),
        ([0.1, 0.0, 0.1], [0.9, 0.8, 0.9])
    );

    let mut chunk = Chunk::new();
    chunk.set(4, 20, 4, BlockId::DeadBush);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    assert_eq!(meshes.opaque.count_vertices(), 0);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        meshes.masked.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("dead bush should render as a crossed plant");
    };
    assert_eq!(positions.len(), 8);

    let Some(VertexAttributeValues::Float32x2(uvs)) = meshes.masked.attribute(Mesh::ATTRIBUTE_UV_0)
    else {
        panic!("dead bush should use the terrain atlas tile");
    };
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

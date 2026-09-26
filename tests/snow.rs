use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use game::block::id::Id;
use game::block::properties::collision_bounds;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::physics::Aabb;
use game::physics::BLOCK_REACH;
use game::physics::colliding_aabbs;
use game::physics::raycast_blocks;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::lighting::Skylight;
use game::world::lighting::light_opacity;
use game::world::meshing::mesh_chunk;

fn generated(chunk: Chunk) -> GeneratedChunk {
    GeneratedChunk {
        heightmap: Heightmap::from_chunk(&chunk),
        biomes: BiomeMap::from_cells(
            [Climate {
                temperature: 0.5,
                humidity: 0.5,
                biome: Biome::Plains,
            }; CHUNK_SIZE * CHUNK_SIZE],
        ),
        chunk,
        items: Vec::new(),
    }
}

#[test]
fn snow_layer_is_non_opaque_and_has_one_eighth_selection_and_collision_height() {
    assert!(!is_opaque_cube(Id::SnowLayer));
    assert!(is_opaque_cube(Id::Snow));
    assert_eq!(light_opacity(Id::SnowLayer), 0);
    assert_eq!(light_opacity(Id::Snow), 15);
    assert_eq!(
        selection_bounds(Id::SnowLayer),
        ([0.0; 3], [1.0, 0.125, 1.0])
    );
    assert_eq!(
        collision_bounds(Id::SnowLayer),
        Some(([0.0; 3], [1.0, 0.125, 1.0]))
    );
    assert_eq!(collision_bounds(Id::Snow), Some(([0.0; 3], [1.0; 3])));
}

#[test]
fn snow_layer_mesh_is_one_eighth_block_high() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::SnowLayer);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION.id)
    else {
        panic!("snow layer mesh should have position data");
    };
    let min_y = positions
        .iter()
        .map(|position| position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = positions
        .iter()
        .map(|position| position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert_eq!(min_y, 64.0);
    assert!((max_y - 64.125).abs() < f32::EPSILON);
}

#[test]
fn a_snow_layer_does_not_hide_the_neighboring_full_block_side() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Stone);
    chunk.set(9, 64, 8, Id::SnowLayer);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION.id)
    else {
        panic!("mesh should have position data");
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL.id)
    else {
        panic!("mesh should have normal data");
    };

    let full_stone_side_is_visible =
        positions
            .chunks_exact(4)
            .zip(normals.chunks_exact(4))
            .any(|(quad, quad_normals)| {
                quad_normals.iter().all(|normal| *normal == [1.0, 0.0, 0.0])
                    && quad
                        .iter()
                        .all(|position| (position[0] - 9.0).abs() < f32::EPSILON)
                    && quad
                        .iter()
                        .map(|position| position[1])
                        .fold(f32::INFINITY, f32::min)
                        == 64.0
                    && quad
                        .iter()
                        .map(|position| position[1])
                        .fold(f32::NEG_INFINITY, f32::max)
                        == 65.0
            });
    assert!(full_stone_side_is_visible);
}

#[test]
fn snow_layer_ray_and_entity_collision_stop_at_its_top() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::SnowLayer);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated(chunk));

    assert!(
        raycast_blocks(&chunks, Vec3::new(7.0, 64.06, 8.5), Vec3::X, BLOCK_REACH)
            .is_some_and(|hit| hit.block == Id::SnowLayer)
    );
    assert!(raycast_blocks(&chunks, Vec3::new(7.0, 64.5, 8.5), Vec3::X, BLOCK_REACH).is_none());

    let above_layer = Aabb::new(Vec3::new(8.2, 64.2, 8.2), Vec3::new(8.8, 64.4, 8.8));
    assert!(colliding_aabbs(&chunks, above_layer).is_empty());
    let intersects_layer = Aabb::new(Vec3::new(8.2, 64.1, 8.2), Vec3::new(8.8, 64.2, 8.8));
    let collisions = colliding_aabbs(&chunks, intersects_layer);
    assert_eq!(collisions.len(), 1);
    assert!((collisions[0].max.y - 64.125).abs() < f32::EPSILON);
}

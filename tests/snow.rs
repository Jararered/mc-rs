use bevy::prelude::*;
use game::block::blocks::Block;
use game::block::properties::collision_bounds;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::physics::Aabb;
use game::physics::BLOCK_REACH;
use game::physics::colliding_aabbs;
use game::physics::raycast_blocks;
use game::rendering::meshing::mesh_chunk;
use game::world::biome::Biome;
use game::world::biome::BiomeMap;
use game::world::biome::Climate;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::GeneratedChunk;
use game::world::chunk::Heightmap;
use game::world::chunk::WorldChunks;
use game::world::lighting::Skylight;
use game::world::lighting::light_opacity;

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
        populated: true,
    }
}

#[test]
fn thin_snow_is_non_opaque_with_selection_but_no_collision() {
    assert!(!is_opaque_cube(Block::SnowLayer));
    assert!(is_opaque_cube(Block::Snow));
    assert_eq!(light_opacity(Block::SnowLayer), 0);
    assert_eq!(light_opacity(Block::Snow), 15);
    assert_eq!(
        selection_bounds(Block::SnowLayer),
        ([0.0; 3], [1.0, 0.125, 1.0])
    );
    assert_eq!(collision_bounds(Block::SnowLayer), None);
    assert_eq!(collision_bounds(Block::Snow), Some(([0.0; 3], [1.0; 3])));
}

#[test]
fn snow_layer_mesh_is_one_eighth_block_high() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::SnowLayer);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let positions = mesh.positions();
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
    chunk.set(8, 64, 8, Block::Stone);
    chunk.set(9, 64, 8, Block::SnowLayer);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let positions = mesh.positions();
    let normals = mesh.normals();

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
    chunk.set(8, 64, 8, Block::SnowLayer);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPosition::ZERO, generated(chunk));

    assert!(
        raycast_blocks(&chunks, Vec3::new(7.0, 64.06, 8.5), Vec3::X, BLOCK_REACH)
            .is_some_and(|hit| hit.block == Block::SnowLayer)
    );
    assert!(raycast_blocks(&chunks, Vec3::new(7.0, 64.5, 8.5), Vec3::X, BLOCK_REACH).is_none());

    let above_layer = Aabb::new(Vec3::new(8.2, 64.2, 8.2), Vec3::new(8.8, 64.4, 8.8));
    assert!(colliding_aabbs(&chunks, above_layer).is_empty());
    let intersects_layer = Aabb::new(Vec3::new(8.2, 64.1, 8.2), Vec3::new(8.8, 64.2, 8.8));
    let collisions = colliding_aabbs(&chunks, intersects_layer);
    assert!(collisions.is_empty());

    chunks
        .get_mut(ChunkPosition::ZERO)
        .unwrap()
        .chunk
        .set_with_metadata(8, 64, 8, Block::SnowLayer, 3);
    assert!(
        raycast_blocks(&chunks, Vec3::new(7.0, 64.4, 8.5), Vec3::X, BLOCK_REACH)
            .is_some_and(|hit| hit.block == Block::SnowLayer)
    );
    let intersects_layer = Aabb::new(Vec3::new(8.2, 64.4, 8.2), Vec3::new(8.8, 64.6, 8.8));
    let collisions = colliding_aabbs(&chunks, intersects_layer);
    assert_eq!(collisions.len(), 1);
    assert!((collisions[0].max.y - 64.5).abs() < f32::EPSILON);
}

#[test]
fn adjacent_snow_layers_merge_tops_and_exposed_sides() {
    let mut chunk = Chunk::new();
    for x in 2..6 {
        for z in 3..7 {
            chunk.set(x, 64, z, Block::SnowLayer);
        }
    }
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let quads = mesh.vertices().chunks_exact(4).collect::<Vec<_>>();
    let facing = |normal: [f32; 3]| {
        quads
            .iter()
            .filter(|quad| quad[0].normal == normal)
            .copied()
            .collect::<Vec<_>>()
    };
    let tops = facing([0.0, 1.0, 0.0]);
    assert_eq!(tops.len(), 1, "the flat snow surface is one quad");
    assert!(tops[0].iter().all(|vertex| {
        vertex.repeat_uv && vertex.position[1] == 64.125 && vertex.texel.tile == [2, 4]
    }));
    let sides = facing([0.0, 0.0, -1.0]);
    assert_eq!(sides.len(), 1, "the exposed edge is one horizontal quad");
    assert!(
        sides[0]
            .iter()
            .all(|vertex| vertex.repeat_uv && vertex.texel.texel == [1, 0])
    );
    assert_eq!(
        sides[0]
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min),
        2.0
    );
    assert_eq!(
        sides[0]
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max),
        6.0
    );
    assert!(
        sides[0]
            .iter()
            .all(|v| (64.0..=64.125).contains(&v.position[1]))
    );
    assert_eq!(facing([0.0, -1.0, 0.0]).len(), 1);
    assert_eq!(quads.len(), 6, "no internal snow-to-snow side faces");
}

#[test]
fn snow_layers_cull_shared_sides_across_chunk_boundaries() {
    use game::rendering::meshing::ChunkNeighbors;
    use game::rendering::meshing::mesh_chunk_with_neighbors;
    let mut chunk = Chunk::new();
    let mut east = Chunk::new();
    for z in 2..6 {
        chunk.set(CHUNK_SIZE - 1, 64, z, Block::SnowLayer);
        east.set(0, 64, z, Block::SnowLayer);
    }
    let mesh = mesh_chunk_with_neighbors(
        &chunk,
        &ChunkNeighbors {
            east: Some(&east),
            ..Default::default()
        },
        &Skylight::from_chunk(&chunk),
    );
    assert!(
        mesh.vertices()
            .chunks_exact(4)
            .all(|quad| { quad[0].normal != [1.0, 0.0, 0.0] })
    );
    assert_eq!(
        mesh.vertices()
            .chunks_exact(4)
            .filter(|quad| quad[0].normal == [0.0, 1.0, 0.0])
            .count(),
        1,
    );
}

#[test]
fn snow_tops_do_not_merge_with_full_snow_blocks() {
    let mut chunk = Chunk::new();
    chunk.set(2, 64, 2, Block::SnowLayer);
    chunk.set(3, 64, 2, Block::Snow);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let heights = mesh
        .vertices()
        .chunks_exact(4)
        .filter(|quad| quad[0].normal == [0.0, 1.0, 0.0])
        .map(|quad| quad[0].position[1])
        .collect::<Vec<_>>();
    assert!(heights.contains(&64.125));
    assert!(heights.contains(&65.0));
}

#[test]
fn snow_on_solid_ground_has_one_greedy_top() {
    let mut chunk = Chunk::new();
    for x in 2..6 {
        for z in 3..7 {
            chunk.set(x, 63, z, Block::Grass);
            chunk.set(x, 64, z, Block::SnowLayer);
        }
    }
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let tops = mesh
        .vertices()
        .chunks_exact(4)
        .filter(|quad| quad[0].normal == [0.0, 1.0, 0.0] && quad[0].position[1] == 64.125)
        .collect::<Vec<_>>();
    assert_eq!(tops.len(), 1, "snow atop terrain should merge too");
    assert!(tops[0].iter().all(|v| v.repeat_uv));
}

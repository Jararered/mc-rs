use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Vec3;
use game::block::id::Id;
use game::block::properties::blocks_movement;
use game::block::properties::collision_bounds;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::item::ItemId;
use game::item::ItemStack;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::place_selected_block;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::generation::WorldGenerator;
use game::world::lighting::Skylight;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::textures::block_tile;

fn world_with(chunk: Chunk) -> WorldChunks {
    let heightmap = Heightmap::from_chunk(&chunk);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPos::ZERO,
        GeneratedChunk {
            chunk,
            heightmap,
            biomes: BiomeMap::from_cells(
                [Climate {
                    temperature: 0.5,
                    humidity: 0.5,
                    biome: Biome::Plains,
                }; CHUNK_SIZE * CHUNK_SIZE],
            ),
            items: Vec::new(),
        },
    );
    chunks
}

#[test]
fn sugar_cane_is_a_centered_crossed_plant_with_beta_appearance() {
    assert_eq!(block_tile(Id::SugarCane, 0, false), (9, 4));
    assert!(!is_opaque_cube(Id::SugarCane));
    assert!(!blocks_movement(Id::SugarCane));
    assert_eq!(collision_bounds(Id::SugarCane), None);
    assert_eq!(
        selection_bounds(Id::SugarCane),
        ([0.125, 0.0, 0.125], [0.875, 1.0, 0.875])
    );

    let mut chunk = Chunk::new();
    chunk.set(3, 10, 5, Id::SugarCane);
    let meshes = mesh_chunk_with_settings(&chunk, &Skylight::from_chunk(&chunk), true, true);
    assert_eq!(meshes.opaque.count_vertices(), 0);
    assert_eq!(meshes.masked.count_vertices(), 8);
    let Some(VertexAttributeValues::Float32x3(positions)) = meshes
        .masked
        .attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION.id)
    else {
        panic!("sugar cane should have crossed-sprite positions");
    };
    let min_x = positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = positions
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_z = positions.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min);
    let max_z = positions
        .iter()
        .map(|p| p[2])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - 3.05).abs() < f32::EPSILON);
    assert!((max_x - 3.95).abs() < f32::EPSILON);
    assert!((min_z - 5.05).abs() < f32::EPSILON);
    assert!((max_z - 5.95).abs() < f32::EPSILON);
}

#[test]
fn sugar_cane_can_be_placed_on_watered_soil_or_sand_and_stacked() {
    let cane = ItemStack::new(ItemId::SugarCane, 1).unwrap();
    assert_eq!(cane.runtime_block(), Some(Id::SugarCane));
    for soil in [Id::Grass, Id::Dirt, Id::Sand] {
        let mut chunk = Chunk::new();
        chunk.set(8, 64, 8, soil);
        chunk.set(9, 64, 8, Id::Water);
        let mut chunks = world_with(chunk);
        let base_hit = BlockHit {
            x: 8,
            y: 64,
            z: 8,
            face: BlockFace::Up,
            block: soil,
        };
        assert!(place_selected_block(
            &mut chunks,
            base_hit,
            Aabb::new(Vec3::splat(100.0), Vec3::splat(101.0)),
            cane.runtime_block().unwrap(),
        ));

        let stack_hit = BlockHit {
            x: 8,
            y: 65,
            z: 8,
            face: BlockFace::Up,
            block: Id::SugarCane,
        };
        assert!(place_selected_block(
            &mut chunks,
            stack_hit,
            Aabb::new(Vec3::splat(100.0), Vec3::splat(101.0)),
            Id::SugarCane,
        ));
        assert_eq!(chunks.block_at(8, 65, 8), Some(Id::SugarCane));
        assert_eq!(chunks.block_at(8, 66, 8), Some(Id::SugarCane));
    }
}

#[test]
fn sugar_cane_requires_water_next_to_its_ground_support() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Id::Dirt);
    let mut chunks = world_with(chunk);
    let hit = BlockHit {
        x: 8,
        y: 64,
        z: 8,
        face: BlockFace::Up,
        block: Id::Dirt,
    };
    assert!(!place_selected_block(
        &mut chunks,
        hit,
        Aabb::new(Vec3::splat(100.0), Vec3::splat(101.0)),
        Id::SugarCane,
    ));
}

#[test]
fn world_generation_places_sugar_cane_near_water() {
    let generator = WorldGenerator::new(0);
    for z in -16..=16 {
        for x in -16..=16 {
            let generated = generator.generate(ChunkPos { x, z });
            if generated.chunk.blocks().contains(&Id::SugarCane) {
                return;
            }
        }
    }
    panic!("seeded chunks should contain a generated sugar-cane patch");
}

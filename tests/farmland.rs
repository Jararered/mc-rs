use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use bevy::prelude::Vec3;
use game::block::block::BlockId;
use game::block::properties::collision_bounds;
use game::block::properties::hardness;
use game::block::properties::is_opaque_cube;
use game::block::properties::selection_bounds;
use game::entity::drops::blocks::DropRoll;
use game::entity::drops::blocks::player_break_drops;
use game::inventory::Hotbar;
use game::item::ItemId;
use game::item::ItemStack;
use game::item::tools::is_hoe;
use game::item::tools::str_vs_block;
use game::physics::BLOCK_REACH;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::physics::raycast_blocks;
use game::player::till_block;
use game::player::till_with_selected_hoe;
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
use game::world::persistence::WorldStorage;
use game::world::textures::atlas_tile_uvs;
use game::world::textures::block_tile;
use game::world::textures::farmland_top_tile;

struct Rolls;

impl DropRoll for Rolls {
    fn next_int(&mut self, _bound: u32) -> u32 {
        0
    }
}

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

fn world_with(chunk: Chunk) -> WorldChunks {
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated(chunk));
    chunks
}

fn hit(x: i32, block: BlockId, face: BlockFace) -> BlockHit {
    BlockHit {
        x,
        y: 64,
        z: 8,
        face,
        block,
    }
}

fn farmland_top_uses_tile(mesh: &Mesh, tile: (u8, u8)) -> bool {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return false;
    };
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return false;
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(tile.0, tile.1);
    positions
        .chunks_exact(4)
        .zip(uvs.chunks_exact(4))
        .any(|(quad, quad_uvs)| {
            quad.iter()
                .all(|position| (position[1] - (64.0 + 15.0 / 16.0)).abs() < f32::EPSILON)
                && quad_uvs
                    .iter()
                    .all(|uv| (u0..=u1).contains(&uv[0]) && (v0..=v1).contains(&uv[1]))
        })
}

#[test]
fn hoe_tills_dirt_from_any_face_and_grass_from_all_but_bottom() {
    for hoe in [
        ItemId::WoodenHoe,
        ItemId::StoneHoe,
        ItemId::IronHoe,
        ItemId::DiamondHoe,
        ItemId::GoldHoe,
    ] {
        assert!(is_hoe(hoe));
    }
    assert!(!is_hoe(ItemId::WoodenShovel));

    let faces = [
        BlockFace::Down,
        BlockFace::Up,
        BlockFace::North,
        BlockFace::South,
        BlockFace::West,
        BlockFace::East,
    ];
    let mut chunk = Chunk::new();
    for (index, face) in faces.into_iter().enumerate() {
        let dirt_x = index as i32 + 1;
        let grass_x = index as i32 + 8;
        chunk.set(dirt_x as usize, 64, 8, BlockId::Dirt);
        chunk.set(grass_x as usize, 64, 8, BlockId::Grass);
        let mut chunks = world_with(chunk.clone());

        assert!(till_block(&mut chunks, hit(dirt_x, BlockId::Dirt, face)));
        assert_eq!(chunks.block_at(dirt_x, 64, 8), Some(BlockId::Farmland));

        let grass_tilled = till_block(&mut chunks, hit(grass_x, BlockId::Grass, face));
        assert_eq!(grass_tilled, face != BlockFace::Down);
        assert_eq!(
            chunks.block_at(grass_x, 64, 8),
            Some(if grass_tilled {
                BlockId::Farmland
            } else {
                BlockId::Grass
            })
        );
    }
}

#[test]
fn dirt_can_be_tilled_when_covered_but_grass_cannot() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Dirt);
    chunk.set(8, 65, 8, BlockId::Stone);
    chunk.set(9, 64, 8, BlockId::Grass);
    chunk.set(9, 65, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);

    assert!(till_block(
        &mut chunks,
        hit(8, BlockId::Dirt, BlockFace::Up)
    ));
    assert!(!till_block(
        &mut chunks,
        hit(9, BlockId::Grass, BlockFace::North)
    ));
    assert!(!till_block(
        &mut chunks,
        hit(8, BlockId::Stone, BlockFace::Up)
    ));
    assert!(!till_block(
        &mut chunks,
        hit(8, BlockId::Farmland, BlockFace::Up)
    ));
    assert_eq!(chunks.block_at(8, 64, 8), Some(BlockId::Farmland));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::Grass));
}

#[test]
fn tilling_rejects_unsupported_blocks_and_bottom_clicks_on_grass() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    chunk.set(9, 64, 8, BlockId::Grass);
    let mut chunks = world_with(chunk);

    assert!(!till_block(
        &mut chunks,
        hit(8, BlockId::Stone, BlockFace::Up)
    ));
    assert!(!till_block(
        &mut chunks,
        hit(9, BlockId::Grass, BlockFace::Down)
    ));
    assert!(!till_block(
        &mut chunks,
        hit(9, BlockId::Farmland, BlockFace::Up)
    ));
    assert_eq!(chunks.block_at(8, 64, 8), Some(BlockId::Stone));
    assert_eq!(chunks.block_at(9, 64, 8), Some(BlockId::Grass));
}

#[test]
fn successful_hoe_use_spends_durability_without_charging_failed_attempts() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Dirt);
    chunk.set(9, 64, 8, BlockId::Stone);
    let mut chunks = world_with(chunk);
    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(ItemStack::new(ItemId::WoodenHoe, 1).unwrap());

    assert!(till_with_selected_hoe(
        &mut chunks,
        &mut hotbar,
        hit(8, BlockId::Dirt, BlockFace::Up)
    ));
    assert_eq!(hotbar.selected_stack().unwrap().data(), 1);
    assert!(!till_with_selected_hoe(
        &mut chunks,
        &mut hotbar,
        hit(8, BlockId::Farmland, BlockFace::Up)
    ));
    assert!(!till_with_selected_hoe(
        &mut chunks,
        &mut hotbar,
        hit(9, BlockId::Stone, BlockFace::Up)
    ));
    assert_eq!(hotbar.selected_stack().unwrap().data(), 1);
    hotbar.slots[0] = Some(ItemStack::new(ItemId::WoodenShovel, 1).unwrap());
    let mut chunk = Chunk::new();
    chunk.set(10, 64, 8, BlockId::Dirt);
    let mut chunks = world_with(chunk);
    assert!(!till_with_selected_hoe(
        &mut chunks,
        &mut hotbar,
        hit(10, BlockId::Dirt, BlockFace::East)
    ));
    assert_eq!(chunks.block_at(10, 64, 8), Some(BlockId::Dirt));
}

#[test]
fn farmland_has_beta_properties_tiles_and_dirt_drop() {
    assert!(BlockId::Farmland.in_world());
    assert_eq!(BlockId::Farmland.placed(0), Some(BlockId::Farmland));
    assert!(!is_opaque_cube(BlockId::Farmland));
    assert_eq!(hardness(BlockId::Farmland), 0.6);
    assert_eq!(light_opacity(BlockId::Farmland), 15);
    assert_eq!(
        collision_bounds(BlockId::Farmland),
        Some(([0.0; 3], [1.0; 3]))
    );
    assert_eq!(
        selection_bounds(BlockId::Farmland),
        ([0.0; 3], [1.0, 15.0 / 16.0, 1.0])
    );
    assert_eq!(farmland_top_tile(false), (7, 5));
    assert_eq!(farmland_top_tile(true), (6, 5));
    assert_eq!(block_tile(BlockId::Farmland, 0, false), (7, 5));
    for face in [1, 2, 3, 4, 5] {
        assert_eq!(block_tile(BlockId::Farmland, face, false), (2, 0));
    }
    let shovel = ItemStack::new(ItemId::WoodenShovel, 1).unwrap();
    assert_eq!(str_vs_block(Some(shovel), BlockId::Farmland), 2.0);
    assert_eq!(
        player_break_drops(BlockId::Farmland, None, &mut Rolls),
        vec![ItemStack::from_block(BlockId::Dirt, 1).unwrap()]
    );
}

#[test]
fn farmland_mesh_and_ray_selection_stop_at_fifteen_sixteenths() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Farmland);
    let chunks = world_with(chunk.clone());
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("farmland mesh should have position data");
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
    assert!((max_y - 64.0 - 15.0 / 16.0).abs() < f32::EPSILON);

    assert!(
        raycast_blocks(&chunks, Vec3::new(7.0, 64.9, 8.5), Vec3::X, BLOCK_REACH)
            .is_some_and(|hit| hit.block == BlockId::Farmland)
    );
    assert!(raycast_blocks(&chunks, Vec3::new(7.0, 64.95, 8.5), Vec3::X, BLOCK_REACH).is_none());
}

#[test]
fn farmland_nearby_water_uses_the_wet_top_tile() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Farmland);
    chunk.set(4, 64, 8, BlockId::Water);

    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));

    assert!(farmland_top_uses_tile(&mesh, farmland_top_tile(true)));
    assert!(!farmland_top_uses_tile(&mesh, farmland_top_tile(false)));
}

#[test]
fn dry_farmland_uses_the_dry_top_tile() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Farmland);

    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));

    assert!(farmland_top_uses_tile(&mesh, farmland_top_tile(false)));
    assert!(!farmland_top_uses_tile(&mesh, farmland_top_tile(true)));
}

#[test]
fn farmland_does_not_hide_a_neighboring_full_block_side() {
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, BlockId::Stone);
    chunk.set(9, 64, 8, BlockId::Farmland);
    let mesh = mesh_chunk(&chunk, &Skylight::from_chunk(&chunk));
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("mesh should have position data");
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("mesh should have normal data");
    };

    assert!(
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
            })
    );
}

#[test]
fn farmland_round_trips_through_persistence() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let saves = PathBuf::from(std::env::temp_dir()).join(format!("farmland-{unique}"));
    fs::create_dir_all(&saves).unwrap();
    let storage = WorldStorage::create(&saves, 0, "Farmland").unwrap();
    let position = ChunkPos::ZERO;
    let mut generated = generated(Chunk::new());
    generated.chunk.set(8, 64, 8, BlockId::Farmland);
    generated.heightmap = Heightmap::from_chunk(&generated.chunk);

    storage.save_chunk(position, &generated).unwrap();
    let loaded = storage.load_chunk(position).unwrap();
    assert_eq!(loaded.chunk.get(8, 64, 8), Some(BlockId::Farmland));
    assert_eq!(loaded.heightmap.get(8, 8), 65);
    fs::remove_dir_all(saves).unwrap();
}

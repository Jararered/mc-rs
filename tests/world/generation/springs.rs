use bevy::math::IVec3;
use game::block::id::Id;
use game::random::JavaRandom;
use game::world::block_ticks::BlockTicks;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::generation::Biome;
use game::world::generation::BiomeMap;
use game::world::generation::Climate;
use game::world::generation::GeneratedChunk;
use game::world::generation::Heightmap;
use game::world::generation::WorldGenerator;
use game::world::generation::springs::generate_attempts;
use game::world::generation::springs::generate_spring;
use game::world::lighting::LightCache;

fn world() -> (WorldChunks, BlockTicks, LightCache) {
    let mut chunks = WorldChunks::default();
    for x in 0..2 {
        for z in 0..2 {
            let chunk = Chunk::new();
            chunks.insert(
                ChunkPosition { x, z },
                GeneratedChunk {
                    heightmap: Heightmap::from_chunk(&chunk),
                    chunk,
                    biomes: BiomeMap::from_cells(
                        [Climate {
                            temperature: 0.5,
                            humidity: 0.5,
                            biome: Biome::Plains,
                        }; CHUNK_SIZE * CHUNK_SIZE],
                    ),
                    items: Vec::new(),
                    populated: false,
                },
            );
        }
    }
    (chunks, BlockTicks::new(123), LightCache::default())
}

fn set(chunks: &mut WorldChunks, at: IVec3, id: Id) {
    chunks.set_block_with_metadata(at.x, at.y, at.z, id, 0);
}

fn pocket(chunks: &mut WorldChunks, at: IVec3) {
    set(chunks, at, Id::Stone);
    set(chunks, at + IVec3::Y, Id::Stone);
    set(chunks, at - IVec3::Y, Id::Stone);
    set(chunks, at + IVec3::NEG_X, Id::Stone);
    set(chunks, at + IVec3::NEG_Z, Id::Stone);
    set(chunks, at + IVec3::Z, Id::Stone);
    set(chunks, at + IVec3::X - IVec3::Y, Id::Stone);
}

#[test]
fn spring_requires_the_java_stone_and_air_pocket() {
    let at = IVec3::new(12, 40, 12);
    let (mut chunks, mut ticks, mut light) = world();
    pocket(&mut chunks, at);
    assert!(generate_spring(
        &mut ticks.world(&mut chunks, &mut light, 0),
        at,
        Id::FlowingWater
    ));
    assert!(matches!(
        chunks.block_at(at.x, at.y, at.z),
        Some(Id::Water | Id::FlowingWater)
    ));
    // The direct update starts the first outward flow before the pass ends.
    assert!(matches!(
        chunks.block_at(at.x + 1, at.y, at.z),
        Some(Id::Water | Id::FlowingWater)
    ));
    assert!(ticks.is_scheduled(at, Id::FlowingWater));
    let origin = ChunkPosition::ZERO;
    let mut chunk = chunks.remove(origin).unwrap().chunk;
    ticks.unload_chunk(origin, &mut chunk);
    assert!(chunk.pending_ticks().iter().any(|tick| {
        tick.index == Chunk::index(at.x as usize, at.y as usize, at.z as usize) as u16
            && tick.block == Id::FlowingWater
    }));
}

#[test]
fn a_spring_can_start_in_air_as_well_as_stone() {
    let at = IVec3::new(12, 40, 12);
    let (mut chunks, mut ticks, mut light) = world();
    pocket(&mut chunks, at);
    set(&mut chunks, at, Id::Air);
    assert!(generate_spring(
        &mut ticks.world(&mut chunks, &mut light, 0),
        at,
        Id::FlowingWater,
    ));
    assert!(matches!(
        chunks.block_at(at.x, at.y, at.z),
        Some(Id::Water | Id::FlowingWater)
    ));
}

#[test]
fn spring_rejects_wrong_caps_center_and_horizontal_neighbors() {
    let at = IVec3::new(12, 40, 12);
    for (changed, id, expected) in [
        (at + IVec3::Y, Id::Dirt, false),
        (at - IVec3::Y, Id::Air, false),
        (at, Id::Dirt, false),
        (at + IVec3::NEG_X, Id::Air, true),
        (at + IVec3::X, Id::Dirt, true),
    ] {
        let (mut chunks, mut ticks, mut light) = world();
        pocket(&mut chunks, at);
        set(&mut chunks, changed, id);
        let previous = chunks.block_at(at.x, at.y, at.z);
        assert_eq!(
            generate_spring(
                &mut ticks.world(&mut chunks, &mut light, 0),
                at,
                Id::FlowingWater
            ),
            expected
        );
        assert_eq!(chunks.block_at(at.x, at.y, at.z), previous);
    }
}

#[test]
fn lava_spring_flows_and_a_missing_immediate_neighborhood_stops_at_the_edge() {
    let at = IVec3::new(23, 40, 12);
    let (mut chunks, mut ticks, mut light) = world();
    pocket(&mut chunks, at);
    assert!(generate_spring(
        &mut ticks.world(&mut chunks, &mut light, 0),
        at,
        Id::FlowingLava
    ));
    assert!(matches!(
        chunks.block_at(at.x, at.y, at.z),
        Some(Id::Lava | Id::FlowingLava)
    ));
    // x=24 can be written, but its 8-block scheduled-tick neighborhood
    // reaches chunk x=2, outside population's four loaded chunks.
    assert_eq!(chunks.block_at(24, 40, 12), Some(Id::FlowingLava));
    assert_eq!(chunks.metadata_at(24, 40, 12), 2);
}

#[test]
fn attempt_positions_and_draw_count_match_java_random() {
    // Java Random(314159): the first water candidate is (8,32,8), the
    // first lava candidate (15,22,13), then the nextInt(1_000_000) is 533616.
    let (mut chunks, mut ticks, mut light) = world();
    let water = IVec3::new(8, 32, 8);
    let lava = IVec3::new(15, 22, 13);
    pocket(&mut chunks, water);
    pocket(&mut chunks, lava);
    let mut rand = JavaRandom::new(314159);
    generate_attempts(
        &mut ticks.world(&mut chunks, &mut light, 0),
        &mut rand,
        ChunkPosition::ZERO,
    );
    assert_eq!(rand.next_int(1_000_000), 533616);
    assert!(matches!(
        chunks.block_at(water.x, water.y, water.z),
        Some(Id::Water | Id::FlowingWater)
    ));
    assert!(matches!(
        chunks.block_at(lava.x, lava.y, lava.z),
        Some(Id::Lava | Id::FlowingLava)
    ));
}

#[test]
fn populated_chunks_contain_overworld_springs() {
    let area = WorldGenerator::new(0).generate_area(ChunkPosition::ZERO, 4);
    let water = area
        .values()
        .flat_map(|generated| generated.chunk.blocks())
        .filter(|&id| id == Id::FlowingWater)
        .count();
    assert!(water > 0, "expected at least one flowing water spring");
    let lava_above_caves = area.values().any(|generated| {
        (10..128).any(|y| {
            (0..16).any(|x| (0..16).any(|z| generated.chunk.get(x, y, z) == Some(Id::FlowingLava)))
        })
    });
    assert!(
        lava_above_caves,
        "expected a lava spring above the cave lava layer"
    );
}

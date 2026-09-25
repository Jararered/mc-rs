use game::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPos;
use game::world::generation::WorldGenerator;

#[test]
fn underground_generation_is_deterministic_and_order_independent() {
    let first = WorldGenerator::new(2147381);
    let second = WorldGenerator::new(2147381);
    let positions = [
        ChunkPos { x: -1, z: 0 },
        ChunkPos { x: 0, z: 0 },
        ChunkPos { x: 0, z: 1 },
    ];
    let forward: Vec<_> = positions
        .iter()
        .map(|&pos| {
            let generated = first.generate(pos);
            let mut chests = generated
                .chunk
                .chests()
                .map(|(index, chest)| (index, chest.slots))
                .collect::<Vec<_>>();
            chests.sort_unstable_by_key(|(index, _)| *index);
            (generated.chunk.blocks().to_vec(), chests)
        })
        .collect();
    for (pos, (expected_blocks, expected_chests)) in
        positions.into_iter().rev().zip(forward.into_iter().rev())
    {
        let generated = second.generate(pos);
        let mut chests = generated
            .chunk
            .chests()
            .map(|(index, chest)| (index, chest.slots))
            .collect::<Vec<_>>();
        chests.sort_unstable_by_key(|(index, _)| *index);
        assert_eq!(generated.chunk.blocks(), expected_blocks);
        assert_eq!(chests, expected_chests);
    }
}

#[test]
fn common_ores_generate_and_cave_lava_stays_below_y10() {
    let generator = WorldGenerator::new(37);
    let mut coal = 0;
    let mut iron = 0;
    for pos in [
        ChunkPos { x: 0, z: 0 },
        ChunkPos { x: 1, z: 0 },
        ChunkPos { x: 0, z: 1 },
        ChunkPos { x: 1, z: 1 },
    ] {
        let generated = generator.generate(pos);
        for y in 0..CHUNK_HEIGHT {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    match generated.chunk.get(x, y, z).unwrap() {
                        BlockId::CoalOre => coal += 1,
                        BlockId::IronOre => iron += 1,
                        BlockId::FlowingLava if y < 10 => {}
                        BlockId::FlowingLava => panic!("cave lava above Beta's Y=10 cutoff"),
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(coal > 0 && iron > 0, "expected common ore veins");
}

#[test]
fn population_places_clay_and_dungeon_blocks() {
    let generator = WorldGenerator::new(0);
    let mut clay = 0;
    let mut spawners = 0;
    let mut chests = 0;
    let mut lake_lava = 0;
    let mut cave_lava = 0;
    let mut rare_ores = [0; 5];
    let mut stored_chests = 0;
    let mut looted_chests = 0;
    let mut dungeon_loot_stacks = 0;
    for z in -8..8 {
        for x in -8..8 {
            let generated = generator.generate(ChunkPos { x, z });
            for block in generated.chunk.blocks() {
                match block {
                    BlockId::Clay => clay += 1,
                    BlockId::MobSpawner => spawners += 1,
                    BlockId::Chest => chests += 1,
                    BlockId::Lava => lake_lava += 1,
                    BlockId::FlowingLava => cave_lava += 1,
                    BlockId::GoldOre => rare_ores[0] += 1,
                    BlockId::RedstoneOre => rare_ores[1] += 1,
                    BlockId::DiamondOre => rare_ores[2] += 1,
                    BlockId::LapisOre => rare_ores[3] += 1,
                    BlockId::IronOre => rare_ores[4] += 1,
                    _ => {}
                }
            }
            for (_, chest) in generated.chunk.chests() {
                stored_chests += 1;
                let items = chest.slots.iter().flatten().count();
                if items > 0 {
                    looted_chests += 1;
                    dungeon_loot_stacks += items;
                }
            }
        }
    }
    assert!(clay > 0, "expected underwater clay patches");
    assert!(spawners > 0, "expected generated dungeon rooms");
    assert!(chests > 0, "expected generated dungeon chests");
    assert_eq!(
        stored_chests, chests,
        "every chest needs block-local storage"
    );
    assert!(looted_chests > 0, "expected generated dungeon chest loot");
    assert!(dungeon_loot_stacks > 0);
    assert!(lake_lava > 0, "expected a lava lake");
    assert!(cave_lava > 0, "expected low cave lava");
    assert!(
        rare_ores.iter().all(|count| *count > 0),
        "expected each ore kind"
    );
}

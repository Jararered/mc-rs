use game::block::blocks::Block;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPosition;
use game::world::generation::overworld::OverworldGenerator;

/// Beta `replaceBlocksForBiome` places filler instead of grass when the first
/// solid is below sea level - 1, so water never sits on grass.
#[test]
fn underwater_surface_is_dirt_not_grass() {
    let mut water_over_dirt = 0;
    // The chunk with the most underwater dirt in each of seeds 0 to 3, out to
    // 3 chunks: from a few dozen beds to a whole sea floor. Pinned, so the
    // test does not generate the rest every run.
    for (seed, x, z) in [(0, 2, 0), (1, 1, 2), (2, -1, -1), (3, -2, -3)] {
        let generated = OverworldGenerator::new(seed).generate(ChunkPosition { x, z });
        for lz in 0..CHUNK_SIZE {
            for lx in 0..CHUNK_SIZE {
                for y in 1..CHUNK_HEIGHT {
                    let above = generated.chunk.get(lx, y, lz).unwrap();
                    let below = generated.chunk.get(lx, y - 1, lz).unwrap();
                    if !matches!(above, Block::Water | Block::Ice) {
                        continue;
                    }
                    assert_ne!(
                        below,
                        Block::Grass,
                        "seed {seed} chunk ({x},{z}) column ({lx},{lz}): grass under water at y={}",
                        y - 1
                    );
                    if below == Block::Dirt {
                        water_over_dirt += 1;
                    }
                }
            }
        }
    }
    assert!(
        water_over_dirt > 0,
        "expected some underwater dirt beds in the sample"
    );
}

#[test]
fn desert_biome_columns_use_sand_as_top_and_filler() {
    let generator = OverworldGenerator::new(12345);
    let generated = generator.generate(ChunkPosition::ZERO);
    let mut desert_columns = 0;
    let mut sandstone_columns = 0;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            if generated.biomes.get(x, z).biome != game::world::biome::Biome::Desert {
                continue;
            }
            desert_columns += 1;
            // Cacti stand on the sand.
            let mut top = generated.heightmap.get(x, z) as usize;
            while generated.chunk.get(x, top - 1, z) == Some(Block::Cactus) {
                top -= 1;
            }
            assert_eq!(generated.chunk.get(x, top - 1, z), Some(Block::Sand));
            assert_eq!(generated.chunk.get(x, top - 2, z), Some(Block::Sand));

            let mut y = top - 2;
            while y > 0 && generated.chunk.get(x, y - 1, z) == Some(Block::Sand) {
                y -= 1;
            }
            let mut sandstone = 0;
            while y > 0 && generated.chunk.get(x, y - 1, z) == Some(Block::Sandstone) {
                sandstone += 1;
                y -= 1;
            }
            assert!(sandstone <= 3, "sandstone layer exceeded three blocks");
            sandstone_columns += usize::from(sandstone > 0);
        }
    }
    assert!(desert_columns > 0);
    assert!(
        sandstone_columns > 0,
        "expected sandstone under some desert sand"
    );
    let repeated = OverworldGenerator::new(12345).generate(ChunkPosition::ZERO);
    assert_eq!(generated.chunk.blocks(), repeated.chunk.blocks());
}

#[test]
fn dirt_filler_does_not_transition_to_sandstone() {
    let generated = OverworldGenerator::new(0).generate(ChunkPosition::ZERO);
    let mut dirt_columns = 0;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let top = generated.heightmap.get(x, z) as usize;
            if top < 2 || generated.chunk.get(x, top - 1, z) != Some(Block::Grass) {
                continue;
            }
            dirt_columns += 1;
            let mut y = top - 1;
            while y > 0 && generated.chunk.get(x, y - 1, z) == Some(Block::Dirt) {
                y -= 1;
            }
            assert_ne!(generated.chunk.get(x, y - 1, z), Some(Block::Sandstone));
        }
    }
    assert!(dirt_columns > 0, "expected grass and dirt surface columns");
}

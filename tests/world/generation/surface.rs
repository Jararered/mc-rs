use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPos;
use game::world::generation::WorldGenerator;

/// Beta `replaceBlocksForBiome` places filler instead of grass when the first
/// solid is below sea level - 1, so water never sits on grass.
#[test]
fn underwater_surface_is_dirt_not_grass() {
    let mut water_over_dirt = 0;
    for seed in 0..4 {
        let generator = WorldGenerator::new(seed);
        for z in -3..3 {
            for x in -3..3 {
                let generated = generator.generate(ChunkPos { x, z });
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        for y in 1..CHUNK_HEIGHT {
                            let above = generated.chunk.get(lx, y, lz).unwrap();
                            let below = generated.chunk.get(lx, y - 1, lz).unwrap();
                            if !matches!(above, BlockId::Water | BlockId::Ice) {
                                continue;
                            }
                            assert_ne!(
                                below,
                                BlockId::Grass,
                                "seed {seed} chunk ({x},{z}) column ({lx},{lz}): grass under water at y={}",
                                y - 1
                            );
                            if below == BlockId::Dirt {
                                water_over_dirt += 1;
                            }
                        }
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
    let generated = WorldGenerator::new(12345).generate(ChunkPos::ZERO);
    let mut desert_columns = 0;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            if generated.biomes.get(x, z).biome != game::world::generation::Biome::Desert {
                continue;
            }
            desert_columns += 1;
            let top = generated.heightmap.get(x, z) as usize;
            assert_eq!(generated.chunk.get(x, top - 1, z), Some(BlockId::Sand));
            assert_eq!(generated.chunk.get(x, top - 2, z), Some(BlockId::Sand));
        }
    }
    assert!(desert_columns > 0);
}

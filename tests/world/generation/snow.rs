use game::block::block::BlockId;
use game::block::properties::blocks_movement;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPos;
use game::world::generation::Biome;
use game::world::generation::WorldGenerator;

#[test]
fn snow_layers_are_generated_only_on_cold_exposed_snow_biome_surfaces() {
    let mut checked_layers = 0;
    let mut found_snow = false;

    'search: for seed in 0..4 {
        let generator = WorldGenerator::new(seed);
        for z in (-4..=4).step_by(2) {
            for x in (-4..=4).step_by(2) {
                let generated = generator.generate(ChunkPos { x, z });
                for local_z in 0..CHUNK_SIZE {
                    for local_x in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_HEIGHT {
                            if generated.chunk.get(local_x, y, local_z) != Some(BlockId::SnowLayer)
                            {
                                continue;
                            }
                            found_snow = true;
                            checked_layers += 1;
                            let climate = generated.biomes.get(local_x, local_z);
                            assert!(matches!(
                                climate.biome,
                                Biome::Taiga | Biome::Tundra | Biome::IceDesert
                            ));
                            assert!(y > 0 && y + 1 < CHUNK_HEIGHT);
                            assert_eq!(
                                generated.chunk.get(local_x, y + 1, local_z),
                                Some(BlockId::Air)
                            );
                            let support = generated.chunk.get(local_x, y - 1, local_z).unwrap();
                            assert_ne!(support, BlockId::Ice);
                            assert!(blocks_movement(support));
                            let adjusted_temperature =
                                climate.temperature - (y as f64 - 64.0) / 64.0 * 0.3;
                            assert!(adjusted_temperature < 0.5);
                        }
                    }
                }
                if found_snow {
                    break 'search;
                }
            }
        }
    }

    assert!(found_snow, "expected a sampled cold biome to receive snow");
    assert!(checked_layers > 0);
}

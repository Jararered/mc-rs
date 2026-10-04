use game::block::blocks::Block;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPosition;
use game::world::generation::overworld::OverworldGenerator;

#[test]
fn snow_layers_are_generated_only_on_cold_exposed_surfaces() {
    let mut checked_layers = 0;
    let mut found_snow = false;

    'search: for seed in 0..4 {
        let generator = OverworldGenerator::new(seed);
        for z in (-4..=4).step_by(2) {
            for x in (-4..=4).step_by(2) {
                let generated = generator.generate(ChunkPosition { x, z });
                for local_z in 0..CHUNK_SIZE {
                    for local_x in 0..CHUNK_SIZE {
                        for y in 0..CHUNK_HEIGHT {
                            if generated.chunk.get(local_x, y, local_z) != Some(Block::SnowLayer) {
                                continue;
                            }
                            found_snow = true;
                            checked_layers += 1;
                            // Beta checks only the temperature, so cold swamps
                            // are snowy too.
                            let climate = generated.biomes.get(local_x, local_z);
                            assert!(y > 0 && y + 1 < CHUNK_HEIGHT);
                            assert_eq!(
                                generated.chunk.get(local_x, y + 1, local_z),
                                Some(Block::Air)
                            );
                            let support = generated.chunk.get(local_x, y - 1, local_z).unwrap();
                            assert_ne!(support, Block::Ice);
                            assert!(support.blocks_movement());
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

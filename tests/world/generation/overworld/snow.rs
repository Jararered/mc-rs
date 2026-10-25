use game::block::blocks::Block;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::ChunkPosition;
use game::world::generation::overworld::OverworldGenerator;

#[test]
fn snow_layers_are_generated_only_on_cold_exposed_surfaces() {
    // The first chunk with snow that a search of seeds 0 to 3, out to 4
    // chunks, found. Pinned, so the test does not walk there every run.
    let generated = OverworldGenerator::new(3).generate(ChunkPosition { x: -4, z: 2 });
    let mut checked_layers = 0;
    for local_z in 0..CHUNK_SIZE {
        for local_x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_HEIGHT {
                if generated.chunk.get(local_x, y, local_z) != Some(Block::SnowLayer) {
                    continue;
                }
                checked_layers += 1;
                // Beta checks only the temperature, so cold swamps are snowy
                // too.
                let climate = generated.biomes.get(local_x, local_z);
                assert!(y > 0 && y + 1 < CHUNK_HEIGHT);
                assert_eq!(
                    generated.chunk.get(local_x, y + 1, local_z),
                    Some(Block::Air)
                );
                let support = generated.chunk.get(local_x, y - 1, local_z).unwrap();
                assert_ne!(support, Block::Ice);
                assert!(support.blocks_movement());
                let adjusted_temperature = climate.temperature - (y as f64 - 64.0) / 64.0 * 0.3;
                assert!(adjusted_temperature < 0.5);
            }
        }
    }
    assert!(
        checked_layers > 0,
        "expected a sampled cold biome to receive snow"
    );
}

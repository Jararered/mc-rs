use game::world::chunk::ChunkPosition;
use game::world::generation::WorldGenerator;

#[test]
fn seeded_terrain_noise_has_stable_column_heights_across_coordinate_signs() {
    let generator = WorldGenerator::new(123_456_789);
    let samples = [
        (0, 0),
        (1, 1),
        (15, 15),
        (16, 16),
        (-1, -1),
        (-16, -16),
        (-17, 31),
        (127, -93),
    ]
    .map(|(x, z)| generator.column_top(x, z));

    assert_eq!(samples, [71, 71, 71, 71, 71, 69, 71, 83]);
}

#[test]
fn generated_climate_is_bounded_and_continuous_across_chunk_edges() {
    let generator = WorldGenerator::new(88_721);
    let left = generator.generate(ChunkPosition { x: -1, z: 2 });
    let right = generator.generate(ChunkPosition { x: 0, z: 2 });
    for z in 0..16 {
        let a = left.biomes.get(15, z);
        let b = right.biomes.get(0, z);
        assert!((0.0..=1.0).contains(&a.temperature));
        assert!((0.0..=1.0).contains(&a.humidity));
        assert!(
            (a.temperature - b.temperature).abs() < 0.1,
            "temperature jumps at the x chunk boundary at z={z}: {} -> {}",
            a.temperature,
            b.temperature
        );
        assert!(
            (a.humidity - b.humidity).abs() < 0.1,
            "humidity jumps at the x chunk boundary at z={z}: {} -> {}",
            a.humidity,
            b.humidity
        );
    }
}

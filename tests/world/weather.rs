use game::world::biome::Biome;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;
use game::world::weather::WorldWeather;
use game::world::weather::can_strike;

#[test]
fn rain_and_thunder_toggle_on_world_ticks() {
    let mut weather = WorldWeather {
        raining: true,
        thundering: true,
        rain_time: 1,
        thunder_time: 1,
        rain_strength: 1.0,
        thunder_strength: 1.0,
        ..Default::default()
    };
    assert_eq!(weather.skylight_penalty(), 8);
    weather.step();
    assert!(!weather.raining && !weather.thundering);
    assert!(weather.rain_strength < 1.0 && weather.thunder_strength < 1.0);
    weather.step();
    assert!(weather.rain_time > 0 && weather.thunder_time > 0);
}

#[test]
fn lightning_needs_an_exposed_nondry_column() {
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
        super::block_ticks::generated(Chunk::new(), Biome::Plains),
    );
    assert!(can_strike(&chunks, 8, 65, 8));
    chunks
        .get_mut(ChunkPosition::ZERO)
        .unwrap()
        .chunk
        .set(8, 70, 8, game::block::id::Id::Stone);
    assert!(!can_strike(&chunks, 8, 65, 8));
    let mut desert = WorldChunks::default();
    desert.insert(
        ChunkPosition::ZERO,
        super::block_ticks::generated(Chunk::new(), Biome::Desert),
    );
    assert!(!can_strike(&desert, 8, 65, 8));
}

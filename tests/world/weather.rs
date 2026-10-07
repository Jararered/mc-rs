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
    assert!((weather.weighted_thunder() - 1.0).abs() < 1e-6);
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
    chunks.get_mut(ChunkPosition::ZERO).unwrap().chunk.set(
        8,
        70,
        8,
        game::block::blocks::Block::Stone,
    );
    assert!(!can_strike(&chunks, 8, 65, 8));
    let mut desert = WorldChunks::default();
    desert.insert(
        ChunkPosition::ZERO,
        super::block_ticks::generated(Chunk::new(), Biome::Desert),
    );
    assert!(!can_strike(&desert, 8, 65, 8));
}

#[test]
fn weather_dims_daylight_without_darkening_the_night() {
    use game::world::environment::skylight_subtracted;
    use game::world::environment::skylight_subtracted_in_weather;

    let (noon, midnight) = (0.0, 0.5);
    assert_eq!(skylight_subtracted_in_weather(noon, 0.0, 0.0), 0);
    assert_eq!(skylight_subtracted_in_weather(noon, 1.0, 0.0), 3);
    assert_eq!(skylight_subtracted_in_weather(noon, 1.0, 1.0), 5);
    assert_eq!(skylight_subtracted_in_weather(midnight, 0.0, 0.0), 11);
    assert_eq!(skylight_subtracted_in_weather(midnight, 1.0, 1.0), 11);
    for angle in [0.0, 0.2, 0.26, 0.5, 0.74, 0.9] {
        assert_eq!(
            skylight_subtracted_in_weather(angle, 0.0, 0.0),
            skylight_subtracted(angle)
        );
    }

    let storm = WorldWeather {
        rain_strength: 1.0,
        thunder_strength: 0.5,
        ..Default::default()
    };
    assert_eq!(
        storm.skylight_subtracted(noon),
        skylight_subtracted_in_weather(noon, 1.0, 0.5)
    );
    assert_eq!(
        game::world::weather::skylight_subtracted(None, midnight),
        11
    );
}

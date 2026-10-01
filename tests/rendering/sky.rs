use game::player::interpolated_swing;
use game::rendering::sky::base_fog_rgb;
use game::rendering::sky::sky_disc_color;
use game::rendering::sky::sky_fog_end;
use game::rendering::sky::sky_rgb;
use game::rendering::sky::star_brightness;
use game::rendering::sky::sunrise_rgba;
use game::rendering::sky::view_distance_blocks;
use game::rendering::sky::world_fog_range;
use game::world::environment::celestial_angle;
use game::world::environment::daylight_factor;
use game::world::persistence::WorldManifest;
use game::world::tick::MAX_TICKS_PER_FRAME;
use game::world::tick::WorldTick;

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
}

#[test]
fn celestial_angle_matches_beta_day_marks() {
    near(celestial_angle(6_000, 0.0), 0.0);
    near(celestial_angle(0, 0.0), 0.7845);
    near(celestial_angle(18_000, 0.0), 0.5);
    let sunset = celestial_angle(12_000, 0.0);
    assert!(sunset > 0.2 && sunset < 0.25, "sunset angle {sunset}");
    assert!(daylight_factor(0.0) > 0.99);
    assert!(daylight_factor(0.5) < 0.01);
}

#[test]
fn fog_distances_follow_the_loaded_chunk_radius() {
    for (chunks, far) in [(4, 64.0), (8, 128.0), (16, 256.0), (32, 512.0)] {
        near(view_distance_blocks(chunks), far);
        let (start, end) = world_fog_range(far);
        near(start, far * 0.8);
        near(end, far);
        near(sky_fog_end(far), far * 0.8);
    }
}

#[test]
fn noon_is_bright_and_midnight_is_dark() {
    let noon = sky_rgb(0.5, 0.0);
    let night = sky_rgb(0.5, 0.5);
    assert!(noon[2] > noon[0], "daytime sky is blue");
    assert!(night.iter().all(|channel| *channel < 0.05));
    let noon_fog = base_fog_rgb(0.0);
    let night_fog = base_fog_rgb(0.5);
    assert!(noon_fog[2] > night_fog[2]);
    assert!(night_fog[2] < 0.2);
    assert!(sunrise_rgba(0.0).is_none());
    assert!(sunrise_rgba(celestial_angle(12_000, 0.0)).is_some());
    near(star_brightness(0.0), 0.0);
    assert!(star_brightness(0.5) > 0.4);
    for angle in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0] {
        assert!(
            (0.0..=1.0).contains(&star_brightness(angle)),
            "star brightness out of range at angle {angle}"
        );
    }
}

#[test]
fn sky_gradient_is_sky_overhead_and_fog_at_the_horizon() {
    let sky = [0.2, 0.4, 1.0];
    let fog = [0.6, 0.7, 0.75];
    let horizon = sky_disc_color(0.0, sky, fog, 200.0);
    for (actual, expected) in horizon.into_iter().zip(fog) {
        near(actual, expected);
    }
    let zenith = sky_disc_color(std::f32::consts::FRAC_PI_2, sky, fog, 200.0);
    assert!(
        (zenith[2] - sky[2]).abs() < (zenith[2] - fog[2]).abs(),
        "zenith stays near the sky color"
    );
}

#[test]
fn world_tick_emits_one_tick_per_fiftieth_and_caps_a_long_frame() {
    let mut tick = WorldTick::default();
    assert_eq!(tick.advance(0.025), 0);
    assert!((tick.partial() - 0.5).abs() < 0.01);
    assert_eq!(tick.advance(0.025), 1);
    assert_eq!(tick.world_time(), 1);
    assert!(tick.partial() < 0.01);

    let mut burst = WorldTick::default();
    assert_eq!(burst.advance(1.0), MAX_TICKS_PER_FRAME);
    assert_eq!(burst.world_time(), u64::from(MAX_TICKS_PER_FRAME));
    assert_eq!(burst.advance(0.0), 0);
}

#[test]
fn arm_swing_interpolates_across_the_wrap() {
    near(interpolated_swing(0.0, 0.125, 0.0), 0.0);
    near(interpolated_swing(0.0, 0.125, 1.0), 0.125);
    near(interpolated_swing(0.875, 0.0, 0.5), 0.9375);
}

#[test]
fn manifest_without_world_time_starts_at_zero() {
    let manifest: WorldManifest = serde_json::from_str(
        r#"{"name":"New World","seed":1,"created_unix_millis":0,"last_played_unix_millis":0,"format_version":1}"#,
    )
    .unwrap();
    assert_eq!(manifest.world_time, 0);
}

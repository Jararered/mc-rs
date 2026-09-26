use bevy::mesh::Indices;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::Mesh;
use bevy::prelude::Vec3;
use game::player::interpolated_swing;
use game::world::persistence::WorldManifest;
use game::world::sky::base_fog_rgb;
use game::world::sky::celestial_angle;
use game::world::sky::daylight_factor;
use game::world::sky::distance_light_weight;
use game::world::sky::mix_fog_toward_sky;
use game::world::sky::sky_color_blend;
use game::world::sky::sky_disc_color;
use game::world::sky::sky_fog_end;
use game::world::sky::sky_rgb;
use game::world::sky::star_brightness;
use game::world::sky::sunrise_rgba;
use game::world::sky::view_distance_blocks;
use game::world::sky::void_rgb;
use game::world::sky::world_fog_range;
use game::world::tick::MAX_TICKS_PER_FRAME;
use game::world::tick::WorldTick;

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
}

#[test]
fn celestial_angle_matches_beta_day_marks() {
    near(celestial_angle(6_000, 0.0), 0.0);
    near(celestial_angle(0, 0.0), 0.7845);
    near(
        celestial_angle(0, 0.0),
        celestial_angle(game::world::tick::DAY_LENGTH, 0.0),
    );
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
fn render_distance_blends_fog_and_eye_light_at_the_expected_limits() {
    near(sky_color_blend(16.0), 0.0);
    near(sky_color_blend(32.0), 0.0);
    let far_blend = 1.0 - 2.0_f32.powf(-0.25);
    near(sky_color_blend(64.0), far_blend);
    near(distance_light_weight(32.0), 0.0);
    near(distance_light_weight(64.0), 1.0 / 3.0);
    near(distance_light_weight(256.0), 1.0);
    near(distance_light_weight(1.0), 0.0);

    let fog = [0.2, 0.4, 0.6];
    let sky = [0.8, 0.6, 0.4];
    assert_eq!(mix_fog_toward_sky(fog, sky, 16.0), fog);
    let blended = mix_fog_toward_sky(fog, sky, 64.0);
    for (actual, expected) in blended.into_iter().zip(
        fog.into_iter()
            .zip(sky)
            .map(|(fog, sky)| fog + (sky - fog) * far_blend),
    ) {
        near(actual, expected);
    }
}

#[test]
fn sky_light_and_below_horizon_color_keep_their_ranges() {
    assert_eq!(game::world::sky::skylight_subtracted(0.0), 0);
    assert_eq!(game::world::sky::skylight_subtracted(0.5), 11);

    let sky = [0.5, 0.25, 1.0];
    for (actual, expected) in void_rgb(sky).into_iter().zip([0.14, 0.09, 0.7]) {
        near(actual, expected);
    }
    let cold = game::world::sky::biome_sky_rgb(-100.0);
    let temperate = game::world::sky::biome_sky_rgb(0.5);
    let hot = game::world::sky::biome_sky_rgb(100.0);
    for color in [cold, temperate, hot] {
        assert!(
            color
                .into_iter()
                .all(|channel| (0.0..=1.0).contains(&channel))
        );
    }
    assert_ne!(cold, hot, "temperature should change the biome sky color");
}

#[test]
fn star_field_quads_stay_on_the_radius_one_hundred_sphere() {
    let mesh = game::world::sky::star_field_mesh();
    let VertexAttributeValues::Float32x3(positions) = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .expect("star mesh has positions")
    else {
        panic!("star positions should be 3D floats");
    };
    let indices = mesh.indices().expect("star mesh has triangle indices");
    let Indices::U32(indices) = indices else {
        panic!("star indices should be 32-bit");
    };

    assert_eq!(positions.len() % 4, 0);
    assert_eq!(indices.len(), positions.len() / 4 * 6);
    assert!(positions.iter().flatten().all(|value| value.is_finite()));
    for quad in positions.chunks_exact(4) {
        let center = quad
            .iter()
            .fold(Vec3::ZERO, |sum, point| sum + Vec3::from_array(*point))
            / 4.0;
        assert!((center.length() - 100.0).abs() < 1.0e-3);
    }
}

#[test]
fn sunrise_fan_has_a_fixed_center_and_alpha_scaled_horizon_ring() {
    let mesh = game::world::sky::sunrise_fan_mesh([0.8, 0.4, 0.2, 0.25]);
    let VertexAttributeValues::Float32x3(positions) = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .expect("sunrise mesh has positions")
    else {
        panic!("sunrise positions should be 3D floats");
    };
    let VertexAttributeValues::Float32x4(colors) = mesh
        .attribute(Mesh::ATTRIBUTE_COLOR)
        .expect("sunrise mesh has colors")
    else {
        panic!("sunrise colors should be 4D floats");
    };

    assert_eq!(positions.len(), 18);
    assert_eq!(positions[0], [0.0, 100.0, 0.0]);
    assert_eq!(colors[0], [0.8, 0.4, 0.2, 0.25]);
    for (step, point) in positions[1..].iter().enumerate() {
        near((point[0] * point[0] + point[1] * point[1]).sqrt(), 120.0);
        let expected_z = -((step as f32) * std::f32::consts::TAU / 16.0).cos() * 10.0;
        near(point[2], expected_z);
        assert_eq!(colors[step + 1], [0.8, 0.4, 0.2, 0.0]);
    }
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

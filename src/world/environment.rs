//! Deterministic Overworld daylight calculations, shared by simulation and rendering.

use super::tick::DAY_LENGTH;

const MC_PI: f32 = 3.141_592_7;

/// Day length fraction after Beta's sunrise offset and cosine smoothing.
/// Noon (`world_time = 6000`) is `0`.
pub fn celestial_angle(world_time: u64, partial: f32) -> f32 {
    let day = (world_time % DAY_LENGTH) as f32;
    let mut angle = (day + partial) / DAY_LENGTH as f32 - 0.25;
    if angle < 0.0 {
        angle += 1.0;
    }
    if angle > 1.0 {
        angle -= 1.0;
    }
    let base = angle;
    let cosine = (f64::from(angle) * std::f64::consts::PI).cos();
    let fraction = ((cosine + 1.0) / 2.0) as f32;
    let smoothed = 1.0 - fraction;
    base + (smoothed - base) / 3.0
}

/// `cos(angle * 2π) * 2 + 0.5`, clamped, shared by sky, fog, and sunlight.
pub fn daylight_factor(angle: f32) -> f32 {
    ((angle * MC_PI * 2.0).cos() * 2.0 + 0.5).clamp(0.0, 1.0)
}

/// `World.calculateSkylightSubtracted` with no rain or thunder.
pub fn skylight_subtracted(angle: f32) -> u8 {
    skylight_subtracted_in_weather(angle, 0.0, 0.0)
}

/// `World.calculateSkylightSubtracted`. `thunder` is Beta's weighted thunder
/// strength, already multiplied by the rain strength. Weather dims what
/// daylight there is, so a rainy night is no darker than a clear one.
pub fn skylight_subtracted_in_weather(angle: f32, rain: f32, thunder: f32) -> u8 {
    let mut light = daylight_factor(angle);
    light = (f64::from(light) * (1.0 - f64::from(rain * 5.0) / 16.0)) as f32;
    light = (f64::from(light) * (1.0 - f64::from(thunder * 5.0) / 16.0)) as f32;
    ((1.0 - light) * 11.0) as u8
}

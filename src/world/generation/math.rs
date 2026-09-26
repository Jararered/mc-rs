//! Beta `MathHelper`: float sine and cosine read from a 65536-entry table.
//!
//! Caves and mineral veins integrate hundreds of these samples, so the
//! table's small errors against true sine decide which blocks are carved.

use std::sync::OnceLock;

fn sin_table() -> &'static [f32] {
    static TABLE: OnceLock<Box<[f32]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..65536)
            .map(|index| (f64::from(index) * std::f64::consts::PI * 2.0 / 65536.0).sin() as f32)
            .collect()
    })
}

/// `MathHelper.sin`.
pub(super) fn sin(value: f32) -> f32 {
    sin_table()[((value * 10430.378) as i32 & 0xffff) as usize]
}

/// `MathHelper.cos`.
pub(super) fn cos(value: f32) -> f32 {
    sin_table()[((value * 10430.378 + 16384.0) as i32 & 0xffff) as usize]
}

/// `MathHelper.floor_double`.
pub(super) fn floor_double(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated - 1
    } else {
        truncated
    }
}

/// The float value of pi Beta uses throughout generation.
pub(super) const PI: f32 = 3.141_592_7;

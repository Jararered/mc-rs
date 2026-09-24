use std::fs;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormats;
use bevy::image::ImageSampler;
use bevy::image::ImageType;
use bevy::prelude::Color;
use bevy::prelude::Image;
use bevy::prelude::Resource;

use crate::world::generation::Climate;

pub const PALETTE_SIZE: usize = 256;
pub(crate) const DEFAULT_GRASS_COLOR: [f32; 3] = [0.55, 0.8, 0.4];

/// A 256×256 climate palette, as used by `grasscolor.png` and `foliagecolor.png`.
#[derive(Clone, Default)]
struct ColorPalette {
    rgba: Option<Arc<[u8]>>,
}

impl ColorPalette {
    fn load(path: &str) -> Self {
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        let Ok(image) = Image::from_buffer(
            &bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            RenderAssetUsages::MAIN_WORLD,
        ) else {
            return Self::default();
        };
        if image.texture_descriptor.size.width != PALETTE_SIZE as u32
            || image.texture_descriptor.size.height != PALETTE_SIZE as u32
        {
            return Self::default();
        }
        let Some(data) = image.data else {
            return Self::default();
        };
        if data.len() != PALETTE_SIZE * PALETTE_SIZE * 4 {
            return Self::default();
        }
        Self {
            rgba: Some(data.into()),
        }
    }

    fn from_rgba(rgba: Vec<u8>) -> Self {
        Self {
            rgba: Some(rgba.into()),
        }
    }

    fn sample(&self, climate: Climate, fallback: [f32; 3]) -> [f32; 3] {
        let Some(rgba) = &self.rgba else {
            return fallback;
        };
        let offset = palette_index(climate.temperature, climate.humidity) * 4;
        let color = Color::srgb_u8(rgba[offset], rgba[offset + 1], rgba[offset + 2]).to_linear();
        [color.red, color.green, color.blue]
    }
}

/// Climate-sampled grass tint, loaded from `grasscolor.png`.
#[derive(Clone, Resource, Default)]
pub struct GrassColors(ColorPalette);

impl GrassColors {
    pub(crate) fn load() -> Self {
        Self(ColorPalette::load("assets/misc/grasscolor.png"))
    }

    /// Build a palette from raw RGBA bytes, primarily for tests and future
    /// texture-pack loading paths.
    pub fn from_rgba(rgba: Vec<u8>) -> Self {
        Self(ColorPalette::from_rgba(rgba))
    }

    pub fn sample(&self, climate: Climate) -> [f32; 3] {
        self.0.sample(climate, DEFAULT_GRASS_COLOR)
    }

    /// Sample the climate palette, using the standard grass tint when the
    /// column climate is unavailable.
    pub fn sample_optional(&self, climate: Option<Climate>) -> [f32; 3] {
        climate.map_or(DEFAULT_GRASS_COLOR, |climate| self.sample(climate))
    }
}

/// Climate-sampled foliage tint, loaded from `foliagecolor.png`.
#[derive(Clone, Resource, Default)]
pub struct FoliageColors(ColorPalette);

impl FoliageColors {
    pub(crate) fn load() -> Self {
        Self(ColorPalette::load("assets/misc/foliagecolor.png"))
    }

    pub fn from_rgba(rgba: Vec<u8>) -> Self {
        Self(ColorPalette::from_rgba(rgba))
    }

    pub fn sample(&self, climate: Climate) -> [f32; 3] {
        self.0.sample(climate, [0.28, 0.71, 0.09])
    }
}

pub fn palette_index(temperature: f64, humidity: f64) -> usize {
    let temperature = temperature.clamp(0.0, 1.0);
    let humidity = humidity.clamp(0.0, 1.0);
    let x = ((1.0 - temperature) * 255.0) as usize;
    let y = ((1.0 - temperature * humidity) * 255.0) as usize;
    y * PALETTE_SIZE + x
}

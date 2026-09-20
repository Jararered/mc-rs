use std::{fs, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    prelude::{Color, Image, Resource},
};

use crate::world::generation::Climate;

pub const PALETTE_SIZE: usize = 256;

#[derive(Clone, Resource, Default)]
pub struct GrassColors {
    rgba: Option<Arc<[u8]>>,
}

impl GrassColors {
    pub(crate) fn load() -> Self {
        let Ok(bytes) = fs::read("assets/misc/grasscolor.png") else {
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

    /// Build a palette from raw RGBA bytes, primarily for tests and future
    /// texture-pack loading paths.
    pub fn from_rgba(rgba: Vec<u8>) -> Self {
        Self {
            rgba: Some(rgba.into()),
        }
    }

    pub fn sample(&self, climate: Climate) -> [f32; 3] {
        let Some(rgba) = &self.rgba else {
            return [0.55, 0.8, 0.4];
        };
        let offset = palette_index(climate.temperature, climate.humidity) * 4;
        let color = Color::srgb_u8(rgba[offset], rgba[offset + 1], rgba[offset + 2]).to_linear();
        [color.red, color.green, color.blue]
    }
}

pub fn palette_index(temperature: f64, humidity: f64) -> usize {
    let temperature = temperature.clamp(0.0, 1.0);
    let humidity = humidity.clamp(0.0, 1.0);
    let x = ((1.0 - temperature) * 255.0) as usize;
    let y = ((1.0 - temperature * humidity) * 255.0) as usize;
    y * PALETTE_SIZE + x
}

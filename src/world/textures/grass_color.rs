use std::{fs, sync::Arc};

use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    prelude::{Color, Image, Resource},
};

use crate::world::generation::Climate;

const PALETTE_SIZE: usize = 256;

#[derive(Clone, Resource, Default)]
pub(crate) struct GrassColors {
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

    pub(crate) fn sample(&self, climate: Climate) -> [f32; 3] {
        let Some(rgba) = &self.rgba else {
            return [0.55, 0.8, 0.4];
        };
        let offset = palette_index(climate.temperature, climate.humidity) * 4;
        let color = Color::srgb_u8(rgba[offset], rgba[offset + 1], rgba[offset + 2]).to_linear();
        [color.red, color.green, color.blue]
    }
}

fn palette_index(temperature: f64, humidity: f64) -> usize {
    let temperature = temperature.clamp(0.0, 1.0);
    let humidity = humidity.clamp(0.0, 1.0);
    let x = ((1.0 - temperature) * 255.0) as usize;
    let y = ((1.0 - temperature * humidity) * 255.0) as usize;
    y * PALETTE_SIZE + x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::Biome;

    #[test]
    fn grass_palette_uses_temperature_and_temperature_weighted_humidity() {
        assert_eq!(palette_index(1.0, 1.0), 0);
        assert_eq!(palette_index(0.0, 0.0), 255 * 256 + 255);
        assert_eq!(palette_index(1.0, 0.0), 255 * 256);
        assert_eq!(palette_index(0.5, 1.0), 127 * 256 + 127);
    }

    #[test]
    fn grass_color_varies_with_climate_within_one_biome() {
        let mut rgba = vec![0; PALETTE_SIZE * PALETTE_SIZE * 4];
        rgba[0..4].copy_from_slice(&[0, 255, 0, 255]);
        let dry = palette_index(1.0, 0.0) * 4;
        rgba[dry..dry + 4].copy_from_slice(&[255, 0, 0, 255]);
        let colors = GrassColors {
            rgba: Some(rgba.into()),
        };
        let wet = colors.sample(Climate {
            temperature: 1.0,
            humidity: 1.0,
            biome: Biome::Forest,
        });
        let dry = colors.sample(Climate {
            temperature: 1.0,
            humidity: 0.0,
            biome: Biome::Forest,
        });
        assert_eq!(wet, [0.0, 1.0, 0.0]);
        assert_eq!(dry, [1.0, 0.0, 0.0]);
    }
}

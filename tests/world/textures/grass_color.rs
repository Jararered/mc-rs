use game::world::{
    generation::{Biome, Climate},
    textures::{GrassColors, PALETTE_SIZE, palette_index},
};

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
    let colors = GrassColors::from_rgba(rgba);
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

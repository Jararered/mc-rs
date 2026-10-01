use bevy::asset::RenderAssetUsages;
use bevy::prelude::Image;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use game::rendering::textures::ATLAS_GRID;
use game::rendering::textures::ATLAS_PAD_TEXELS;
use game::rendering::textures::ATLAS_TILE_PX;
use game::rendering::textures::FlowingWaterTexture;
use game::rendering::textures::StillWaterTexture;
use game::rendering::textures::WATER_FLOW_TILE;
use game::rendering::textures::WATER_STILL_TILE;
use game::rendering::textures::pad_atlas_tiles;
use game::rendering::textures::write_atlas_tile;

#[test]
fn first_still_water_tick_is_the_resting_beta_color() {
    let mut water = StillWaterTexture::new();
    water.tick();
    for pixel in water.rgba().chunks_exact(4) {
        assert_eq!(pixel, [32, 50, 255, 146]);
    }
}

#[test]
fn still_water_pixels_stay_in_the_beta_color_range() {
    let mut water = StillWaterTexture::new();
    for _ in 0..80 {
        water.tick();
        for pixel in water.rgba().chunks_exact(4) {
            assert!((32..=64).contains(&pixel[0]), "r {}", pixel[0]);
            assert!((50..=114).contains(&pixel[1]), "g {}", pixel[1]);
            assert_eq!(pixel[2], 255);
            assert!((146..=196).contains(&pixel[3]), "a {}", pixel[3]);
        }
    }
}

#[test]
fn still_water_develops_ripples() {
    let mut water = StillWaterTexture::new();
    water.tick();
    let first = water.rgba().to_vec();
    for _ in 0..40 {
        water.tick();
    }
    assert_ne!(
        water.rgba(),
        first.as_slice(),
        "still water should leave the uniform resting frame"
    );
    let unique: std::collections::HashSet<_> = water.rgba().chunks_exact(4).collect();
    assert!(
        unique.len() > 1,
        "ripples should produce more than one color"
    );
}

#[test]
fn flowing_water_scrolls_the_generated_frame() {
    let mut water = FlowingWaterTexture::new();
    for _ in 0..30 {
        water.tick();
    }
    let before = water.rgba().to_vec();
    water.tick();
    assert_ne!(
        water.rgba(),
        before.as_slice(),
        "flowing water should scroll one texel row per tick"
    );
}

#[test]
fn generated_water_overwrites_the_atlas_still_tile_and_gutter() {
    let mut image = blank_atlas();
    pad_atlas_tiles(&mut image);
    let mut water = StillWaterTexture::new();
    water.tick();
    write_atlas_tile(
        &mut image,
        WATER_STILL_TILE.0,
        WATER_STILL_TILE.1,
        water.rgba(),
    );

    let pad = ATLAS_PAD_TEXELS;
    let stride = ATLAS_TILE_PX + 2 * pad;
    let padded = ATLAS_GRID * stride;
    let data = image.data.as_ref().expect("atlas pixels");
    let inner = pixel(
        data,
        padded,
        u32::from(WATER_STILL_TILE.0) * stride + pad,
        u32::from(WATER_STILL_TILE.1) * stride + pad,
    );
    assert_eq!(inner, [32, 50, 255, 146]);
    let gutter = pixel(
        data,
        padded,
        u32::from(WATER_STILL_TILE.0) * stride,
        u32::from(WATER_STILL_TILE.1) * stride + pad,
    );
    assert_eq!(gutter, inner);
}

#[test]
fn flowing_water_fills_the_2x2_atlas_block() {
    let mut image = blank_atlas();
    pad_atlas_tiles(&mut image);
    let mut water = FlowingWaterTexture::new();
    water.tick();
    for dy in 0..2u8 {
        for dx in 0..2u8 {
            write_atlas_tile(
                &mut image,
                WATER_FLOW_TILE.0 + dx,
                WATER_FLOW_TILE.1 + dy,
                water.rgba(),
            );
        }
    }
    let pad = ATLAS_PAD_TEXELS;
    let stride = ATLAS_TILE_PX + 2 * pad;
    let padded = ATLAS_GRID * stride;
    let data = image.data.as_ref().expect("atlas pixels");
    for dy in 0..2u32 {
        for dx in 0..2u32 {
            let sample = pixel(
                data,
                padded,
                (u32::from(WATER_FLOW_TILE.0) + dx) * stride + pad,
                (u32::from(WATER_FLOW_TILE.1) + dy) * stride + pad,
            );
            assert_eq!(sample, [32, 50, 255, 146]);
        }
    }
}

fn blank_atlas() -> Image {
    let width = ATLAS_GRID * ATLAS_TILE_PX;
    Image::new(
        Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![200u8; (width * width * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    )
}

fn pixel(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [data[i], data[i + 1], data[i + 2], data[i + 3]]
}

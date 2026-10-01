mod grass_color;
mod water;
mod wireframe;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::Image;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;
use game::rendering::textures::ATLAS_GRID;
use game::rendering::textures::ATLAS_PAD_TEXELS;
use game::rendering::textures::ATLAS_TILE_PX;
use game::rendering::textures::atlas_tile_uvs;
use game::rendering::textures::pad_atlas_tiles;

#[test]
fn atlas_uvs_sit_inside_the_padded_tile_not_on_the_grid_line() {
    let (u0, v0, u1, v1) = atlas_tile_uvs(1, 2);
    let gutter =
        ATLAS_PAD_TEXELS as f32 / (ATLAS_GRID * (ATLAS_TILE_PX + 2 * ATLAS_PAD_TEXELS)) as f32;
    assert!(u0 >= 1.0 / 16.0 + gutter - 1e-6);
    assert!(u1 <= 2.0 / 16.0 - gutter + 1e-6);
    assert!(v0 >= 2.0 / 16.0 + gutter - 1e-6);
    assert!(v1 <= 3.0 / 16.0 - gutter + 1e-6);
    assert!(u0 > 1.0 / 16.0);
    assert!(u1 < 2.0 / 16.0);
}

#[test]
fn padding_duplicates_tile_edges_into_the_gutter() {
    let tile = ATLAS_TILE_PX;
    let grid = ATLAS_GRID;
    let width = grid * tile;
    let mut data = vec![0u8; (width * width * 4) as usize];
    for ty in 0..grid {
        for tx in 0..grid {
            for py in 0..tile {
                for px in 0..tile {
                    let i = (((ty * tile + py) * width + tx * tile + px) * 4) as usize;
                    data[i] = tx as u8;
                    data[i + 1] = ty as u8;
                    data[i + 2] = px as u8;
                    data[i + 3] = 255;
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width,
            height: width,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    );
    pad_atlas_tiles(&mut image);
    let pad = ATLAS_PAD_TEXELS;
    let stride = tile + 2 * pad;
    let padded = grid * stride;
    assert_eq!(image.texture_descriptor.size.width, padded);
    let dst = image.data.expect("padded atlas should keep pixel data");
    // Gutter west of tile (1, 0) should copy that tile's x=0 column, not tile 0.
    let gx = 1 * stride;
    let gy = pad;
    let i = ((gy * padded + gx) * 4) as usize;
    assert_eq!(&dst[i..i + 4], &[1, 0, 0, 255]);
}

use game::ui::block_icons::ICON_SIZE;
use game::ui::block_icons::rasterize_icon;
use game::world::block::block::BlockId;

fn pixel(image: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * ICON_SIZE as usize + x) * 4;
    image[offset..offset + 4].try_into().unwrap()
}

#[test]
fn wood_icon_has_top_and_differently_lit_sides() {
    let mut terrain = vec![0; 256 * 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            for (tile_x, color) in [(5, [220, 30, 20, 255]), (4, [80, 180, 40, 255])] {
                let offset = (((16 + y) * 256 + tile_x * 16 + x) * 4) as usize;
                terrain[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    let icon = rasterize_icon(&terrain, 256, BlockId::Wood);
    assert_eq!(pixel(&icon, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&icon, 16, 8), [220, 30, 20, 255]);
    let left = pixel(&icon, 8, 18);
    let right = pixel(&icon, 23, 18);
    assert_eq!(left[3], 255);
    assert_eq!(right[3], 255);
    assert!(left[1] > right[1]);
}

#[test]
fn transparent_texels_remain_transparent() {
    let terrain = vec![0; 256 * 256 * 4];
    let icon = rasterize_icon(&terrain, 256, BlockId::Leaves);
    assert!(icon.chunks_exact(4).all(|pixel| pixel[3] == 0));
}

#[test]
fn padded_terrain_samples_inner_tile_pixels() {
    let width = 320usize;
    let mut terrain = vec![0; width * width * 4];
    // Stone uses tile (1, 0). Its padded slot is x=20..40 with
    // the real tile at x=22..38.
    for y in 0..20 {
        for x in 20..40 {
            let at = (y * width + x) * 4;
            terrain[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    for y in 2..18 {
        for x in 22..38 {
            let at = (y * width + x) * 4;
            terrain[at..at + 4].copy_from_slice(&[0, 200, 0, 255]);
        }
    }
    let icon = rasterize_icon(&terrain, width as u32, BlockId::Stone);
    assert_eq!(pixel(&icon, 16, 8), [0, 200, 0, 255]);
}

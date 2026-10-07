//! Beta inventory sprites. The two original atlases are sampled after
//! loading, producing a compact icon atlas for all registered item identities
//! and their visual subtypes. No reference texture is written to disk.
//!
//! Beta draws a block item as a small 3D model at screen resolution, so its
//! edges and texels stay sharp at every GUI scale. The software rasterizer
//! here gets the same result by drawing each icon at exactly the size the GUI
//! shows it, `16 * GameSettings::gui_scale` pixels, and rebuilding the atlas
//! when that scale changes.
use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::math::Rect;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::render::render_resource::TextureFormat;

use crate::app::settings::DEFAULT_GUI_SCALE;
use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::item::ItemData;
use crate::item::ItemRegistry;
use crate::item::ItemStack;
use crate::rendering::appearance::Appearance;
use crate::rendering::appearance::Shape;
use crate::rendering::appearance::block_appearance;
use crate::rendering::appearance::item_tile;
use crate::rendering::meshing::geometry::BlockFaceGeometry;

/// Shared icon assets for inventory widgets and world item sprites.
pub struct ItemIconsPlugin;

impl Plugin for ItemIconsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup)
            .add_systems(Update, build);
    }
}

/// Icon edge in pixels at the default GUI scale, and the size
/// [`rasterize_icon`] draws.
pub const ICON_SIZE: u32 = icon_size_for(DEFAULT_GUI_SCALE);
/// Edge of an inventory icon in GUI pixels.
const ICON_GUI: u32 = 16;
const COLUMNS: u32 = 16;

/// Icon edge in screen pixels for a `GameSettings::gui_scale` value.
pub const fn icon_size_for(gui_scale: f32) -> u32 {
    let scale = gui_scale.round() as u32;
    ICON_GUI * if scale == 0 { 1 } else { scale }
}

#[derive(Resource)]
pub struct BlockIcons {
    pub image: Handle<Image>,
    terrain: Handle<Image>,
    items: Handle<Image>,
    rectangles: HashMap<(u16, u16), Rect>,
    /// Pixel edge of one icon in the atlas as it is currently built.
    icon_size: u32,
    ready: bool,
}

impl BlockIcons {
    pub fn rect_for_stack(&self, stack: ItemStack) -> Option<Rect> {
        if !self.ready {
            return None;
        }
        let data = match stack.definition().data {
            ItemData::Subtype(_) => stack.data(),
            _ => 0,
        };
        self.rectangles.get(&(stack.item().as_u16(), data)).copied()
    }
    pub fn ready(&self) -> bool {
        self.ready
    }

    /// Pixel edge of one icon in the built atlas.
    pub fn icon_size(&self) -> u32 {
        self.icon_size
    }

    /// UV rectangle for a world item quad, in normalized atlas coordinates.
    pub fn uv_for_stack(&self, stack: ItemStack) -> Option<(f32, f32, f32, f32)> {
        let rect = self.rect_for_stack(stack)?;
        // Cells keep their place when the atlas is rebuilt at another size, so
        // these stay valid across a GUI scale change.
        let atlas = (self.icon_size * COLUMNS) as f32;
        Some((
            rect.min.x / atlas,
            rect.min.y / atlas,
            rect.max.x / atlas,
            rect.max.y / atlas,
        ))
    }
}

pub fn setup(mut commands: Commands, server: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let load = |path| server.load(path);
    let atlas = atlas_image(
        ICON_SIZE,
        vec![0; (ICON_SIZE * COLUMNS).pow(2) as usize * 4],
    );
    commands.insert_resource(BlockIcons {
        image: images.add(atlas),
        terrain: load("terrain.png"),
        items: load("gui/items.png"),
        rectangles: HashMap::new(),
        icon_size: ICON_SIZE,
        ready: false,
    });
}

fn atlas_image(icon_size: u32, pixels: Vec<u8>) -> Image {
    let mut atlas = Image::new(
        Extent3d {
            width: icon_size * COLUMNS,
            height: icon_size * COLUMNS,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    atlas.sampler = ImageSampler::Default;
    atlas
}

struct Source {
    pixels: Vec<u8>,
    width: u32,
    stride: u32,
    pad: u32,
}
impl Source {
    fn from_image(image: &Image) -> Option<Self> {
        let width = image.texture_descriptor.size.width;
        let height = image.texture_descriptor.size.height;
        if width != height
            || width < 256
            || width % 16 != 0
            || !matches!(
                image.texture_descriptor.format,
                TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
            )
        {
            return None;
        }
        let pixels = image.data.as_ref()?;
        if pixels.len() != (width * height * 4) as usize {
            return None;
        }
        let stride = width / 16;
        // World rendering may have added a two-texel gutter around every
        // original 16-texel tile; HD packs scale the gutter with the tile.
        let pad = if stride.is_power_of_two() {
            0
        } else {
            stride / 10
        };
        Some(Self {
            pixels: pixels.clone(),
            width,
            stride,
            pad,
        })
    }
    fn pixel(&self, tile: u8, u: f32, v: f32) -> [u8; 4] {
        let size = self.stride - self.pad * 2;
        let x = u32::from(tile % 16) * self.stride
            + self.pad
            + ((u.clamp(0.0, 0.99999) * size as f32) as u32).min(size - 1);
        let y = u32::from(tile / 16) * self.stride
            + self.pad
            + ((v.clamp(0.0, 0.99999) * size as f32) as u32).min(size - 1);
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }
}

pub fn build(
    mut icons: ResMut<BlockIcons>,
    mut images: ResMut<Assets<Image>>,
    settings: Option<Res<GameSettings>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let size = settings.map_or(ICON_SIZE, |settings| icon_size_for(settings.gui_scale));
    if icons.ready && icons.icon_size == size {
        return;
    }
    let Some(terrain) = images.get(&icons.terrain).and_then(Source::from_image) else {
        return;
    };
    let Some(items) = images.get(&icons.items).and_then(Source::from_image) else {
        return;
    };
    let atlas_size = size * COLUMNS;
    let mut pixels = vec![0; (atlas_size * atlas_size * 4) as usize];
    let mut rectangles = HashMap::new();
    let mut index = 0u32;
    for definition in ItemRegistry::iter() {
        let max_data = match definition.data {
            ItemData::Subtype(max) => max,
            _ => 0,
        };
        for data in 0..=max_data {
            let id = definition.item.as_u16();
            let icon = if id < 256 {
                render_block_icon(&terrain, size, id as u8, data)
            } else if let Some(tile) = item_tile(id, data) {
                render_flat(&items, size, tile, [255; 3])
            } else {
                continue;
            };
            let left = index % COLUMNS * size;
            let top = index / COLUMNS * size;
            if top + size > atlas_size {
                warn!("Inventory icon atlas is full");
                return;
            }
            for y in 0..size {
                let src = (y * size * 4) as usize;
                let dst = (((top + y) * atlas_size + left) * 4) as usize;
                pixels[dst..dst + (size * 4) as usize]
                    .copy_from_slice(&icon[src..src + (size * 4) as usize]);
            }
            rectangles.insert(
                (id, data),
                Rect::new(
                    left as f32,
                    top as f32,
                    (left + size) as f32,
                    (top + size) as f32,
                ),
            );
            index += 1;
        }
    }
    let Some(mut atlas) = images.get_mut(&icons.image) else {
        return;
    };
    *atlas = atlas_image(size, pixels);
    // A resized atlas is a new GPU texture; materials that sample it (dropped
    // item sprites) must be prepared again to bind it.
    if let Some(materials) = materials.as_mut() {
        let users: Vec<_> = materials
            .iter()
            .filter(|(_, material)| material.base_color_texture.as_ref() == Some(&icons.image))
            .map(|(id, _)| id)
            .collect();
        for id in users {
            let _ = materials.get_mut(id);
        }
    }
    icons.rectangles = rectangles;
    icons.icon_size = size;
    icons.ready = true;
    info!("Built {index} inventory icons from terrain and item atlases");
}

fn render_flat(source: &Source, size: u32, tile: u8, tint: [u8; 3]) -> Vec<u8> {
    let mut out = vec![0; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            // Sample at the pixel center so every texel gets the same share.
            let mut pixel = source.pixel(
                tile,
                (x as f32 + 0.5) / size as f32,
                (y as f32 + 0.5) / size as f32,
            );
            for c in 0..3 {
                pixel[c] = (u16::from(pixel[c]) * u16::from(tint[c]) / 255) as u8;
            }
            let at = ((y * size + x) * 4) as usize;
            out[at..at + 4].copy_from_slice(&pixel);
        }
    }
    out
}

fn render_block_icon(source: &Source, size: u32, id: u8, data: u16) -> Vec<u8> {
    let appearance = block_appearance(id, data);
    if appearance.shape == Shape::Flat {
        return render_flat(source, size, appearance.top, appearance.tint);
    }
    let mut out = Canvas {
        pixels: vec![0; (size * size * 4) as usize],
        size,
    };
    match appearance.shape {
        Shape::Cube => {
            let bounds = if id == Block::Farmland.as_u8() {
                [0.0, 0.0, 0.0, 1.0, 0.9375, 1.0] // Farmland
            } else {
                [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
            };
            draw_box(&mut out, source, appearance, bounds);
        }
        Shape::Slab => {
            let bounds = if id == Block::Cake.as_u8() {
                [0.0625, 0.0, 0.0625, 0.9375, 0.5, 0.9375] // Cake
            } else {
                [0.0, 0.0, 0.0, 1.0, 0.5, 1.0]
            };
            draw_box(&mut out, source, appearance, bounds);
        }
        Shape::Thin => {
            let bounds = match id {
                id if id == Block::StonePressurePlate.as_u8()
                    || id == Block::WoodenPressurePlate.as_u8() =>
                {
                    [0.0, 0.375, 0.0, 1.0, 0.625, 1.0]
                } // Pressure plates
                id if id == Block::StoneButton.as_u8() => {
                    [0.3125, 0.375, 0.375, 0.6875, 0.625, 0.625]
                } // Button
                id if id == Block::SnowLayer.as_u8() => [0.0, 0.0, 0.0, 1.0, 0.125, 1.0], // Snow layer
                id if id == Block::Trapdoor.as_u8() => [0.0, 0.40625, 0.0, 1.0, 0.59375, 1.0], // Trapdoor
                _ => [0.0, 0.0, 0.0, 1.0, 0.125, 1.0],
            };
            draw_box(&mut out, source, appearance, bounds);
        }
        Shape::Stairs => {
            // RenderBlocks.renderBlockOnInventory builds the back full-height
            // half followed by the front half-height step.
            draw_box(&mut out, source, appearance, [0.0, 0.0, 0.0, 1.0, 1.0, 0.5]);
            draw_box(&mut out, source, appearance, [0.0, 0.0, 0.5, 1.0, 0.5, 1.0]);
        }
        Shape::Fence => {
            // Four inventory bounds from RenderBlocks: two uprights and two rails.
            draw_box(
                &mut out,
                source,
                appearance,
                [0.375, 0.0, 0.0, 0.625, 1.0, 0.25],
            );
            draw_box(
                &mut out,
                source,
                appearance,
                [0.375, 0.0, 0.75, 0.625, 1.0, 1.0],
            );
            draw_box(
                &mut out,
                source,
                appearance,
                [0.4375, 0.8125, -0.125, 0.5625, 0.9375, 1.125],
            );
            draw_box(
                &mut out,
                source,
                appearance,
                [0.4375, 0.3125, -0.125, 0.5625, 0.4375, 1.125],
            );
        }
        Shape::Cactus => draw_cactus(&mut out, source, appearance),
        Shape::Flat => unreachable!(),
    }
    out.pixels
}

/// One icon being drawn, `size` pixels on each edge.
struct Canvas {
    pixels: Vec<u8>,
    size: u32,
}

impl Canvas {
    // Orthographic projection of the inventory orientation used by RenderItem:
    // a 45-degree yaw with elevated view. Each visible quad samples the matching
    // clipped region of its Beta terrain tile. The block spans 14 of the icon's
    // 16 GUI pixels.
    fn project(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        let gui = self.size as f32 / ICON_GUI as f32;
        (
            (8.0 + 7.0 * (x - z)) * gui,
            (8.0 + 3.5 * (x + z) - 8.0 * y) * gui,
        )
    }
}

fn draw_cactus(out: &mut Canvas, source: &Source, look: Appearance) {
    let geometry = BlockFaceGeometry::cactus();
    for (face_index, tile, light) in [
        (4, look.left, 0.78),
        (2, look.right, 0.62),
        (0, look.top, 1.0),
    ] {
        let face = geometry.face(face_index);
        let points = face.corners.map(|[x, y, z]| out.project(x, y, z));
        draw_face(
            out,
            source,
            tile,
            look.tint,
            light,
            points,
            face.uvs.map(|[u, v]| (u, v)),
        );
    }
}

fn draw_box(out: &mut Canvas, source: &Source, look: Appearance, bounds: [f32; 6]) {
    let [x0, y0, z0, x1, y1, z1] = bounds;
    let left = [
        out.project(x0, y1, z1),
        out.project(x1, y1, z1),
        out.project(x1, y0, z1),
        out.project(x0, y0, z1),
    ];
    let right = [
        out.project(x1, y1, z1),
        out.project(x1, y1, z0),
        out.project(x1, y0, z0),
        out.project(x1, y0, z1),
    ];
    let top = [
        out.project(x0, y1, z0),
        out.project(x1, y1, z0),
        out.project(x1, y1, z1),
        out.project(x0, y1, z1),
    ];
    let side_uv = [
        (x0, 1.0 - y1),
        (x1, 1.0 - y1),
        (x1, 1.0 - y0),
        (x0, 1.0 - y0),
    ];
    let other_uv = [
        (z1, 1.0 - y1),
        (z0, 1.0 - y1),
        (z0, 1.0 - y0),
        (z1, 1.0 - y0),
    ];
    let top_uv = [(x0, z0), (x1, z0), (x1, z1), (x0, z1)];
    draw_face(out, source, look.left, look.tint, 0.78, left, side_uv);
    draw_face(out, source, look.right, look.tint, 0.62, right, other_uv);
    draw_face(out, source, look.top, look.tint, 1.0, top, top_uv);
}

fn draw_face(
    out: &mut Canvas,
    source: &Source,
    tile: u8,
    tint: [u8; 3],
    light: f32,
    points: [(f32, f32); 4],
    uv: [(f32, f32); 4],
) {
    for y in 0..out.size {
        for x in 0..out.size {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let uv = triangle(p, points[0], points[1], points[2], [uv[0], uv[1], uv[2]])
                .or_else(|| triangle(p, points[0], points[2], points[3], [uv[0], uv[2], uv[3]]));
            let Some((u, v)) = uv else {
                continue;
            };
            let mut pixel = source.pixel(tile, u, v);
            if pixel[3] == 0 {
                continue;
            }
            for c in 0..3 {
                pixel[c] = (pixel[c] as f32 * tint[c] as f32 / 255.0 * light).round() as u8;
            }
            let at = ((y * out.size + x) * 4) as usize;
            out.pixels[at..at + 4].copy_from_slice(&pixel);
        }
    }
}

fn triangle(
    p: (f32, f32),
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    uv: [(f32, f32); 3],
) -> Option<(f32, f32)> {
    let d = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
    let w0 = ((b.1 - c.1) * (p.0 - c.0) + (c.0 - b.0) * (p.1 - c.1)) / d;
    let w1 = ((c.1 - a.1) * (p.0 - c.0) + (a.0 - c.0) * (p.1 - c.1)) / d;
    let w2 = 1.0 - w0 - w1;
    if w0 < -0.0001 || w1 < -0.0001 || w2 < -0.0001 {
        return None;
    }
    Some((
        w0 * uv[0].0 + w1 * uv[1].0 + w2 * uv[2].0,
        w0 * uv[0].1 + w1 * uv[1].1 + w2 * uv[2].1,
    ))
}

/// Useful for comparing native block states with the Beta item appearance.
pub fn rasterize_icon(source: &[u8], width: u32, block: Block, metadata: u8) -> Vec<u8> {
    let stride = width / 16;
    let image = Source {
        pixels: source.to_vec(),
        width,
        stride,
        pad: if stride.is_power_of_two() {
            0
        } else {
            stride / 10
        },
    };
    if width < 256 || width % 16 != 0 || source.len() != (width as usize * width as usize * 4) {
        return vec![0; (ICON_SIZE * ICON_SIZE * 4) as usize];
    }
    let (block, metadata) = block.item_form(metadata);
    render_block_icon(&image, ICON_SIZE, block.as_u8(), u16::from(metadata))
}

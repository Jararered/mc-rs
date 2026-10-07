use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::extract_resource::ExtractResourcePlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::Origin3d;
use bevy::render::render_resource::TexelCopyBufferLayout;
use bevy::render::render_resource::TexelCopyTextureInfo;
use bevy::render::render_resource::TextureAspect;
use bevy::render::render_resource::TextureId;
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;

use super::ATLAS_GRID;
use super::ATLAS_PAD_TEXELS;
use super::ATLAS_TILE_PX;
use crate::world::tick::WorldTick;

/// Still water occupies atlas tile (13, 12). Flowing water is a 2×2 of the same
/// generated 16×16, matching Beta `TextureWaterFlowFX.tileSize`.
pub const WATER_STILL_TILE: (u8, u8) = (13, 12);
pub const WATER_FLOW_TILE: (u8, u8) = (14, 12);
/// Beta's still and flowing lava atlas tile positions.
pub const LAVA_STILL_TILE: (u8, u8) = (13, 14);
pub const LAVA_FLOW_TILE: (u8, u8) = (14, 14);

const TILE: usize = ATLAS_TILE_PX as usize;
const TILE_PIXELS: usize = TILE * TILE;

/// Beta `TextureWaterFX` still-water simulation.
#[derive(Clone)]
pub struct StillWaterTexture {
    current: [f32; TILE_PIXELS],
    previous: [f32; TILE_PIXELS],
    heights: [f32; TILE_PIXELS],
    impulses: [f32; TILE_PIXELS],
    rgba: [u8; TILE_PIXELS * 4],
    random: UnitRandom,
}

impl Default for StillWaterTexture {
    fn default() -> Self {
        Self::new()
    }
}

impl StillWaterTexture {
    pub fn new() -> Self {
        Self {
            current: [0.0; TILE_PIXELS],
            previous: [0.0; TILE_PIXELS],
            heights: [0.0; TILE_PIXELS],
            impulses: [0.0; TILE_PIXELS],
            rgba: [0; TILE_PIXELS * 4],
            random: UnitRandom::new(0x5DEECE66D),
        }
    }

    pub fn tick(&mut self) {
        for x in 0..TILE {
            for y in 0..TILE {
                let mut sum = 0.0;
                for nx in x as i32 - 1..=x as i32 + 1 {
                    let sx = (nx & 15) as usize;
                    let sy = y;
                    sum += self.current[sx + sy * TILE];
                }
                self.previous[x + y * TILE] = sum / 3.3 + self.heights[x + y * TILE] * 0.8;
            }
        }

        for x in 0..TILE {
            for y in 0..TILE {
                let index = x + y * TILE;
                self.heights[index] += self.impulses[index] * 0.05;
                if self.heights[index] < 0.0 {
                    self.heights[index] = 0.0;
                }
                self.impulses[index] -= 0.1;
                if self.random.next_double() < 0.05 {
                    self.impulses[index] = 0.5;
                }
            }
        }

        std::mem::swap(&mut self.current, &mut self.previous);
        write_water_pixels(&self.current, &mut self.rgba, 0);
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Beta `TextureWaterFlowFX` flowing-water simulation.
#[derive(Clone)]
pub struct FlowingWaterTexture {
    current: [f32; TILE_PIXELS],
    previous: [f32; TILE_PIXELS],
    heights: [f32; TILE_PIXELS],
    impulses: [f32; TILE_PIXELS],
    rgba: [u8; TILE_PIXELS * 4],
    tick: i32,
    random: UnitRandom,
}

impl Default for FlowingWaterTexture {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowingWaterTexture {
    pub fn new() -> Self {
        Self {
            current: [0.0; TILE_PIXELS],
            previous: [0.0; TILE_PIXELS],
            heights: [0.0; TILE_PIXELS],
            impulses: [0.0; TILE_PIXELS],
            rgba: [0; TILE_PIXELS * 4],
            tick: 0,
            random: UnitRandom::new(0x9E3779B97F4A7C15),
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);

        for x in 0..TILE {
            for y in 0..TILE {
                let mut sum = 0.0;
                for ny in y as i32 - 2..=y as i32 {
                    let sx = x;
                    let sy = (ny & 15) as usize;
                    sum += self.current[sx + sy * TILE];
                }
                self.previous[x + y * TILE] = sum / 3.2 + self.heights[x + y * TILE] * 0.8;
            }
        }

        for x in 0..TILE {
            for y in 0..TILE {
                let index = x + y * TILE;
                self.heights[index] += self.impulses[index] * 0.05;
                if self.heights[index] < 0.0 {
                    self.heights[index] = 0.0;
                }
                self.impulses[index] -= 0.3;
                if self.random.next_double() < 0.2 {
                    self.impulses[index] = 0.5;
                }
            }
        }

        std::mem::swap(&mut self.current, &mut self.previous);
        write_water_pixels(&self.current, &mut self.rgba, self.tick);
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Beta `TextureLavaFX` and `TextureLavaFlowFX` cellular heat simulation.
/// `flowing` selects the vertically scrolling 2×2 flow tile animation.
#[derive(Clone)]
pub struct LavaTexture {
    current: [f32; TILE_PIXELS],
    previous: [f32; TILE_PIXELS],
    heights: [f32; TILE_PIXELS],
    impulses: [f32; TILE_PIXELS],
    rgba: [u8; TILE_PIXELS * 4],
    tick: i32,
    flowing: bool,
    random: UnitRandom,
}

impl LavaTexture {
    pub fn still() -> Self {
        Self::new(false)
    }

    pub fn flowing() -> Self {
        Self::new(true)
    }

    fn new(flowing: bool) -> Self {
        Self {
            current: [0.0; TILE_PIXELS],
            previous: [0.0; TILE_PIXELS],
            heights: [0.0; TILE_PIXELS],
            impulses: [0.0; TILE_PIXELS],
            rgba: [0; TILE_PIXELS * 4],
            tick: 0,
            flowing,
            random: UnitRandom::new(if flowing {
                0x4C415641464C4F57
            } else {
                0x4C4156415354494C
            }),
        }
    }

    pub fn tick(&mut self) {
        if self.flowing {
            self.tick = self.tick.wrapping_add(1);
        }

        for x in 0..TILE {
            for y in 0..TILE {
                let mut sum = 0.0;
                let y_shift = ((y as f32 * std::f32::consts::TAU / TILE as f32).sin() * 1.2) as i32;
                let x_shift = ((x as f32 * std::f32::consts::TAU / TILE as f32).sin() * 1.2) as i32;

                for nx in x as i32 - 1..=x as i32 + 1 {
                    for ny in y as i32 - 1..=y as i32 + 1 {
                        let sx = (nx + y_shift).rem_euclid(16) as usize;
                        let sy = (ny + x_shift).rem_euclid(16) as usize;
                        sum += self.current[sx + sy * TILE];
                    }
                }

                let index = x + y * TILE;
                let corners = self.heights[(x & 15) + (y & 15) * TILE]
                    + self.heights[((x + 1) & 15) + (y & 15) * TILE]
                    + self.heights[((x + 1) & 15) + ((y + 1) & 15) * TILE]
                    + self.heights[(x & 15) + ((y + 1) & 15) * TILE];
                self.previous[index] = sum / 10.0 + corners / 4.0 * 0.8;

                self.heights[index] += self.impulses[index] * 0.01;
                if self.heights[index] < 0.0 {
                    self.heights[index] = 0.0;
                }
                self.impulses[index] -= 0.06;
                if self.random.next_double() < 0.005 {
                    self.impulses[index] = 1.5;
                }
            }
        }

        std::mem::swap(&mut self.current, &mut self.previous);
        write_lava_pixels(
            &self.current,
            &mut self.rgba,
            if self.flowing { self.tick } else { 0 },
        );
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

fn write_lava_pixels(values: &[f32; TILE_PIXELS], rgba: &mut [u8; TILE_PIXELS * 4], scroll: i32) {
    for index in 0..TILE_PIXELS {
        let sample = if scroll == 0 {
            index
        } else {
            (index as i32 - scroll / 3 * TILE as i32).rem_euclid(TILE_PIXELS as i32) as usize
        };
        let intensity = (values[sample] * 2.0).clamp(0.0, 1.0);
        let squared = intensity * intensity;
        let red = (intensity * 100.0 + 155.0) as u8;
        let green = (squared * 255.0) as u8;
        let blue = (squared * squared * 128.0) as u8;
        let offset = index * 4;
        rgba[offset] = red;
        rgba[offset + 1] = green;
        rgba[offset + 2] = blue;
        rgba[offset + 3] = 255;
    }
}

fn write_water_pixels(values: &[f32; TILE_PIXELS], rgba: &mut [u8; TILE_PIXELS * 4], scroll: i32) {
    for index in 0..TILE_PIXELS {
        let sample = if scroll == 0 {
            index
        } else {
            (index as i32 - scroll * TILE as i32).rem_euclid(TILE_PIXELS as i32) as usize
        };
        let mut intensity = values[sample];
        intensity = intensity.clamp(0.0, 1.0);
        let squared = intensity * intensity;
        let red = (32.0 + squared * 32.0) as u8;
        let green = (50.0 + squared * 64.0) as u8;
        let blue = 255;
        let alpha = (146.0 + squared * 50.0) as u8;
        let offset = index * 4;
        rgba[offset] = red;
        rgba[offset + 1] = green;
        rgba[offset + 2] = blue;
        rgba[offset + 3] = alpha;
    }
}

/// Copy a generated 16×16 water frame into one atlas tile, including padding.
pub fn write_atlas_tile(image: &mut Image, tile_x: u8, tile_y: u8, rgba: &[u8]) {
    let Some(layout) = AtlasLayout::from_image(image) else {
        return;
    };
    let Some(frame) = padded_frame(layout, rgba) else {
        return;
    };
    let Some(dst) = image.data.as_mut() else {
        return;
    };
    let bpp = 4usize;
    if dst.len() != layout.atlas_px as usize * layout.atlas_px as usize * bpp {
        return;
    }
    let stride = layout.stride as usize;
    for row in 0..stride {
        let target = ((usize::from(tile_y) * stride + row) * layout.atlas_px as usize
            + usize::from(tile_x) * stride)
            * bpp;
        dst[target..target + stride * bpp]
            .copy_from_slice(&frame[row * stride * bpp..(row + 1) * stride * bpp]);
    }
}

/// A generated 16×16 frame scaled to the atlas tile size, with its edge texels
/// repeated into the padding. `stride × stride` RGBA texels.
fn padded_frame(layout: AtlasLayout, rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != TILE_PIXELS * 4 {
        return None;
    }
    let bpp = 4usize;
    let tile = layout.tile_px;
    let pad = layout.pad_px;
    let stride = layout.stride;
    let mut frame = vec![0u8; stride as usize * stride as usize * bpp];
    for py in 0..stride {
        for px in 0..stride {
            let sx = (px as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
            let sy = (py as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
            let src_x = sx * ATLAS_TILE_PX / tile;
            let src_y = sy * ATLAS_TILE_PX / tile;
            let src_index = (src_y as usize * TILE + src_x as usize) * bpp;
            let dst_index = (py * stride + px) as usize * bpp;
            frame[dst_index..dst_index + bpp].copy_from_slice(&rgba[src_index..src_index + bpp]);
        }
    }
    Some(frame)
}

#[derive(Clone, Copy)]
struct AtlasLayout {
    tile_px: u32,
    pad_px: u32,
    stride: u32,
    atlas_px: u32,
}

impl AtlasLayout {
    fn from_image(image: &Image) -> Option<Self> {
        let width = image.texture_descriptor.size.width;
        let height = image.texture_descriptor.size.height;
        if width != height || width % ATLAS_GRID != 0 {
            return None;
        }
        let stride = width / ATLAS_GRID;
        let padded_denom = ATLAS_TILE_PX + 2 * ATLAS_PAD_TEXELS;
        if stride * ATLAS_TILE_PX % padded_denom == 0 {
            let tile = stride * ATLAS_TILE_PX / padded_denom;
            if tile >= ATLAS_TILE_PX && tile.is_power_of_two() {
                let pad = ATLAS_PAD_TEXELS * tile / ATLAS_TILE_PX;
                return Some(Self {
                    tile_px: tile,
                    pad_px: pad,
                    stride,
                    atlas_px: width,
                });
            }
        }
        Some(Self {
            tile_px: stride,
            pad_px: 0,
            stride,
            atlas_px: width,
        })
    }
}

/// The tile Beta's `Block.portal` draws, which `TexturePortalFX` animates.
pub const PORTAL_TILE: (u8, u8) = (14, 0);

/// Beta `TexturePortalFX`: thirty-two frames of two counter-rotating spirals,
/// worked out once and shown one per tick.
pub struct PortalTexture {
    frames: Vec<[u8; 1024]>,
    tick: usize,
}

impl Default for PortalTexture {
    fn default() -> Self {
        Self::new()
    }
}

impl PortalTexture {
    pub fn new() -> Self {
        use crate::world::generation::math;
        let mut random = crate::random::JavaRandom::new(100);
        let mut frames = vec![[0u8; 1024]; 32];
        for (frame, pixels) in frames.iter_mut().enumerate() {
            for x in 0..16 {
                for y in 0..16 {
                    let mut value = 0.0f32;
                    for spiral in 0..2 {
                        let center = (spiral * 8) as f32;
                        let mut dx = (x as f32 - center) / 16.0 * 2.0;
                        let mut dy = (y as f32 - center) / 16.0 * 2.0;
                        if dx < -1.0 {
                            dx += 2.0;
                        }
                        if dx >= 1.0 {
                            dx -= 2.0;
                        }
                        if dy < -1.0 {
                            dy += 2.0;
                        }
                        if dy >= 1.0 {
                            dy -= 2.0;
                        }
                        let distance = dx * dx + dy * dy;
                        let angle = f64::from(dy).atan2(f64::from(dx)) as f32
                            + (frame as f32 / 32.0 * math::PI * 2.0 - distance * 10.0
                                + (spiral * 2) as f32)
                                * (spiral * 2 - 1) as f32;
                        let wave = (math::sin(angle) + 1.0) / 2.0 / (distance + 1.0);
                        value += wave * 0.5;
                    }
                    value += random.next_float() * 0.1;
                    // Java narrows each `int` to a byte, wrapping past 255.
                    let blue = (value * 100.0 + 155.0) as i32;
                    let red = (value * value * 200.0 + 55.0) as i32;
                    let green = (value * value * value * value * 255.0) as i32;
                    let alpha = (value * 100.0 + 155.0) as i32;
                    let pixel = (y * 16 + x) * 4;
                    pixels[pixel] = red as u8;
                    pixels[pixel + 1] = green as u8;
                    pixels[pixel + 2] = blue as u8;
                    pixels[pixel + 3] = alpha as u8;
                }
            }
        }
        Self { frames, tick: 0 }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn rgba(&self) -> &[u8] {
        &self.frames[self.tick & 31]
    }
}

#[derive(Resource)]
pub(super) struct WaterAnimator {
    portal: PortalTexture,
    still: StillWaterTexture,
    flow: FlowingWaterTexture,
    lava: LavaTexture,
    lava_flow: LavaTexture,
}

/// Frames in [`FluidFrames::frames`] and the atlas tiles each one fills.
/// Flowing water and lava repeat one frame over a 2×2 block of tiles, like
/// Beta's `tileSize = 2` texture effects.
const FLUID_TILES: [(usize, (u8, u8)); 11] = [
    (4, PORTAL_TILE),
    (0, WATER_STILL_TILE),
    (1, WATER_FLOW_TILE),
    (1, (WATER_FLOW_TILE.0 + 1, WATER_FLOW_TILE.1)),
    (1, (WATER_FLOW_TILE.0, WATER_FLOW_TILE.1 + 1)),
    (1, (WATER_FLOW_TILE.0 + 1, WATER_FLOW_TILE.1 + 1)),
    (2, LAVA_STILL_TILE),
    (3, LAVA_FLOW_TILE),
    (3, (LAVA_FLOW_TILE.0 + 1, LAVA_FLOW_TILE.1)),
    (3, (LAVA_FLOW_TILE.0, LAVA_FLOW_TILE.1 + 1)),
    (3, (LAVA_FLOW_TILE.0 + 1, LAVA_FLOW_TILE.1 + 1)),
];

/// The newest padded fluid frames, copied into the render world when they
/// change. The render world writes them into the atlas texture tile by tile,
/// the way Beta's `glTexSubImage2D` did, so the atlas asset itself is never
/// modified and never re-uploaded.
#[derive(Resource, Clone)]
pub(super) struct FluidFrames {
    atlas: AssetId<Image>,
    stride: u32,
    frames: [Vec<u8>; 5],
    version: u64,
}

impl ExtractResource for FluidFrames {
    type Source = Self;

    fn extract_resource(source: &Self) -> Self {
        source.clone()
    }
}

impl FluidFrames {
    fn new(atlas: AssetId<Image>, layout: AtlasLayout, animator: &WaterAnimator) -> Option<Self> {
        let mut frames = Self {
            atlas,
            stride: layout.stride,
            frames: Default::default(),
            version: 0,
        };
        frames.update(layout, animator)?;
        Some(frames)
    }

    fn update(&mut self, layout: AtlasLayout, animator: &WaterAnimator) -> Option<()> {
        self.frames = [
            padded_frame(layout, animator.still.rgba())?,
            padded_frame(layout, animator.flow.rgba())?,
            padded_frame(layout, animator.lava.rgba())?,
            padded_frame(layout, animator.lava_flow.rgba())?,
            padded_frame(layout, animator.portal.rgba())?,
        ];
        self.version += 1;
        Some(())
    }
}

/// Where the atlas stores its tiles, kept for rebuilding fluid frames.
#[derive(Resource, Clone, Copy)]
pub(super) struct FluidAtlasLayout(AtlasLayout);

pub(super) fn start_fluid_animation(
    commands: &mut Commands,
    atlas: &Handle<Image>,
    image: &mut Image,
) {
    let mut animator = WaterAnimator {
        portal: PortalTexture::new(),
        still: StillWaterTexture::new(),
        flow: FlowingWaterTexture::new(),
        lava: LavaTexture::still(),
        lava_flow: LavaTexture::flowing(),
    };
    animator.still.tick();
    animator.flow.tick();
    animator.lava.tick();
    animator.lava_flow.tick();
    animator.portal.tick();
    // The first frame goes out with the atlas upload itself; later frames are
    // tile writes from the render world.
    for (frame, (tile_x, tile_y)) in FLUID_TILES {
        let rgba = match frame {
            0 => animator.still.rgba(),
            1 => animator.flow.rgba(),
            2 => animator.lava.rgba(),
            3 => animator.lava_flow.rgba(),
            _ => animator.portal.rgba(),
        };
        write_atlas_tile(image, tile_x, tile_y, rgba);
    }
    if let Some(layout) = AtlasLayout::from_image(image)
        && let Some(frames) = FluidFrames::new(atlas.id(), layout, &animator)
    {
        commands.insert_resource(frames);
        commands.insert_resource(FluidAtlasLayout(layout));
    }
    commands.insert_resource(animator);
}

pub(super) fn animate_fluid_textures(
    animator: Option<ResMut<WaterAnimator>>,
    layout: Option<Res<FluidAtlasLayout>>,
    frames: Option<ResMut<FluidFrames>>,
    tick: Res<WorldTick>,
) {
    let Some(mut animator) = animator else {
        return;
    };
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    for _ in 0..ticks {
        animator.still.tick();
        animator.flow.tick();
        animator.lava.tick();
        animator.lava_flow.tick();
        animator.portal.tick();
    }
    if let (Some(layout), Some(mut frames)) = (layout, frames) {
        frames.update(layout.0, &animator);
    }
}

pub(super) fn render_plugin(app: &mut App) {
    // Headless apps have no render world, so there is no texture to write.
    if app.get_sub_app(RenderApp).is_none() {
        return;
    }
    app.add_plugins(ExtractResourcePlugin::<FluidFrames>::default());
    app.sub_app_mut(RenderApp).add_systems(
        Render,
        write_fluid_tiles.in_set(RenderSystems::PrepareResources),
    );
}

#[derive(Default)]
struct WrittenFluidFrames {
    version: u64,
    texture: Option<TextureId>,
}

/// Copy the newest fluid frames into their atlas tiles. A re-created atlas
/// texture gets them again even if the frames have not advanced.
fn write_fluid_tiles(
    frames: Option<Res<FluidFrames>>,
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
    mut written: Local<WrittenFluidFrames>,
) {
    let Some(frames) = frames else {
        return;
    };
    let Some(atlas) = images.get(frames.atlas) else {
        return;
    };
    let texture = atlas.texture.id();
    if written.version == frames.version && written.texture == Some(texture) {
        return;
    }
    let stride = frames.stride;
    let size = Extent3d {
        width: stride,
        height: stride,
        depth_or_array_layers: 1,
    };
    for (frame, (tile_x, tile_y)) in FLUID_TILES {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &atlas.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: u32::from(tile_x) * stride,
                    y: u32::from(tile_y) * stride,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &frames.frames[frame],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride * 4),
                rows_per_image: Some(stride),
            },
            size,
        );
    }
    written.version = frames.version;
    written.texture = Some(texture);
}

/// Java `Math.random()` / `Random.nextDouble`.
#[derive(Clone)]
struct UnitRandom {
    state: u64,
}

impl UnitRandom {
    const MULTIPLIER: u64 = 0x5deece66d;
    const ADDEND: u64 = 0xb;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next_bits(&mut self, bits: u32) -> u32 {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND)
            & Self::MASK;
        (self.state >> (48 - bits)) as u32
    }

    fn next_double(&mut self) -> f64 {
        let bits = ((self.next_bits(26) as u64) << 27) | self.next_bits(27) as u64;
        bits as f64 / (1u64 << 53) as f64
    }
}

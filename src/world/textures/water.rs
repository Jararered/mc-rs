use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;

use super::ATLAS_GRID;
use super::ATLAS_PAD_TEXELS;
use super::ATLAS_TILE_PX;

/// Still water occupies atlas tile (13, 12). Flowing water is a 2×2 of the same
/// generated 16×16, matching Beta `TextureWaterFlowFX.tileSize`.
pub const WATER_STILL_TILE: (u8, u8) = (13, 12);
pub const WATER_FLOW_TILE: (u8, u8) = (14, 12);
const WATER_FLOW_TILE_SIZE: u8 = 2;

const TILE: usize = ATLAS_TILE_PX as usize;
const TILE_PIXELS: usize = TILE * TILE;
const TICK_SECS: f32 = 1.0 / 20.0;
const MAX_TICKS_PER_FRAME: u32 = 4;

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
    if rgba.len() != TILE_PIXELS * 4 {
        return;
    }
    let Some(layout) = AtlasLayout::from_image(image) else {
        return;
    };
    let Some(dst) = image.data.as_mut() else {
        return;
    };
    let bpp = 4usize;
    if dst.len() != layout.atlas_px as usize * layout.atlas_px as usize * bpp {
        return;
    }

    let tile = layout.tile_px;
    let pad = layout.pad_px;
    let stride = layout.stride;
    let atlas = layout.atlas_px;
    for py in 0..stride {
        for px in 0..stride {
            let sx = (px as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
            let sy = (py as i32 - pad as i32).clamp(0, tile as i32 - 1) as u32;
            let src_x = sx * ATLAS_TILE_PX / tile;
            let src_y = sy * ATLAS_TILE_PX / tile;
            let src_index = (src_y as usize * TILE + src_x as usize) * bpp;
            let dst_index = ((u32::from(tile_y) * stride + py) * atlas
                + u32::from(tile_x) * stride
                + px) as usize
                * bpp;
            dst[dst_index..dst_index + bpp].copy_from_slice(&rgba[src_index..src_index + bpp]);
        }
    }
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

#[derive(Resource)]
pub(super) struct WaterAnimator {
    atlas: Handle<Image>,
    still: StillWaterTexture,
    flow: FlowingWaterTexture,
    accumulator: f32,
}

pub(super) fn start_water_animation(
    commands: &mut Commands,
    atlas: Handle<Image>,
    image: &mut Image,
) {
    image.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
    let mut still = StillWaterTexture::new();
    let mut flow = FlowingWaterTexture::new();
    still.tick();
    flow.tick();
    write_water_frames(image, &still, &flow);
    commands.insert_resource(WaterAnimator {
        atlas,
        still,
        flow,
        accumulator: 0.0,
    });
}

pub(super) fn animate_water_textures(
    animator: Option<ResMut<WaterAnimator>>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut animator) = animator else {
        return;
    };
    animator.accumulator += time.delta_secs();
    let mut ticks = 0;
    while animator.accumulator >= TICK_SECS && ticks < MAX_TICKS_PER_FRAME {
        animator.accumulator -= TICK_SECS;
        animator.still.tick();
        animator.flow.tick();
        ticks += 1;
    }
    if ticks == 0 {
        return;
    }
    let Some(mut image) = images.get_mut(&animator.atlas) else {
        return;
    };
    write_water_frames(&mut image, &animator.still, &animator.flow);
}

fn write_water_frames(image: &mut Image, still: &StillWaterTexture, flow: &FlowingWaterTexture) {
    write_atlas_tile(image, WATER_STILL_TILE.0, WATER_STILL_TILE.1, still.rgba());
    for dy in 0..WATER_FLOW_TILE_SIZE {
        for dx in 0..WATER_FLOW_TILE_SIZE {
            write_atlas_tile(
                image,
                WATER_FLOW_TILE.0 + dx,
                WATER_FLOW_TILE.1 + dy,
                flow.rgba(),
            );
        }
    }
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

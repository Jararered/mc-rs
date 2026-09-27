#define_import_path game::block_vertex

// Mirrors `BlockShadingSettings` in `block_material.rs`.
struct BlockShadingSettings {
    skylight_subtracted: f32,
    flags: u32,
    wiggle_amplitude: f32,
    _padding: f32,
}

const OLD_LIGHTING: u32 = 1u;
const SMOOTH_LIGHTING: u32 = 2u;
// Mirrors the packing constants in `meshing/vertex.rs`.
const POSITION_MIN: f32 = -8.0;
const HORIZONTAL_STEPS: f32 = 256.0;
const VERTICAL_STEPS: f32 = 128.0;
const BRIGHT_TINT: f32 = 1.12;

struct DecodedBlockVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    color: vec4<f32>,
}

fn sign_not_zero(value: vec2<f32>) -> vec2<f32> {
    return select(vec2(-1.0), vec2(1.0), value >= vec2(0.0));
}

fn decode_normal(bits: vec2<u32>) -> vec3<f32> {
    let encoded = (vec2<f32>(bits) - 31.0) / 31.0;
    var normal = vec3(encoded, 1.0 - abs(encoded.x) - abs(encoded.y));
    if normal.z < 0.0 {
        normal = vec3((1.0 - abs(normal.yx)) * sign_not_zero(normal.xy), normal.z);
    }
    return normalize(normal);
}

/// `WorldProvider.lightBrightnessTable`, matching `beta_brightness`.
fn beta_brightness(level: f32) -> f32 {
    let darkness = 1.0 - level / 15.0;
    return (1.0 - darkness) / (darkness * 3.0 + 1.0) * 0.95 + 0.05;
}

/// One `sky << 4 | block` sample after `Chunk.getBlockLightValue`.
fn sample_brightness(sample: u32, skylight_subtracted: f32) -> f32 {
    let sky = f32((sample >> 4u) & 15u);
    let block = f32(sample & 15u);
    return beta_brightness(max(max(sky - skylight_subtracted, 0.0), block));
}

/// Beta's per-face shade, by the normal's dominant axis.
fn face_shade(normal: vec3<f32>) -> f32 {
    let size = abs(normal);
    if size.y >= size.x && size.y >= size.z {
        return select(0.55, 1.0, normal.y > 0.0);
    }
    if size.x >= size.z {
        return 0.6;
    }
    return 0.8;
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let low = color / 12.92;
    let high = pow((color + 0.055) / 1.055, vec3(2.4));
    return select(high, low, color <= vec3(0.04045));
}

fn decode_block_vertex(packed: vec4<u32>, settings: BlockShadingSettings) -> DecodedBlockVertex {
    var out: DecodedBlockVertex;
    out.position = vec3(
        f32(packed.x & 0x1fffu) / HORIZONTAL_STEPS,
        f32(packed.y & 0xfffu) / VERTICAL_STEPS,
        f32((packed.x >> 13u) & 0x1fffu) / HORIZONTAL_STEPS,
    ) + POSITION_MIN;
    out.normal = decode_normal(vec2((packed.x >> 26u) & 0x3fu, (packed.z >> 24u) & 0x3fu));

    let u = (packed.y >> 12u) & 0x1ffu;
    let v = (packed.y >> 21u) & 0x1ffu;
    let tile = vec2<f32>(vec2(u >> 5u, v >> 5u));
    let texel = vec2<f32>(vec2(u & 31u, v & 31u));
    let pad = f32(#{ATLAS_PAD_TEXELS});
    let stride = f32(#{ATLAS_TILE_PX}) + 2.0 * pad;
    // Beta repeats the flowing-fluid frame over 2×2 tiles. Local texels
    // 16..31 must land in the *next padded tile*, not the intervening gutter
    // or the unrelated still-water tile. Keep this in step with AtlasTexel::uv.
    let flowing = (u >> 5u) == 14u && ((v >> 5u) == 12u || (v >> 5u) == 14u);
    let step = floor(texel / 16.0) * select(0.0, 1.0, flowing);
    out.uv = ((tile + step) * stride + pad + texel - step * 16.0)
        / (f32(#{ATLAS_GRID}) * stride);

    let srgb = vec3<f32>(vec3(packed.z & 0xffu, (packed.z >> 8u) & 0xffu, (packed.z >> 16u) & 0xffu));
    var tint = srgb_to_linear(srgb / 255.0);
    if (packed.z >> 31u) == 1u {
        tint *= BRIGHT_TINT;
    }

    let smooth_lighting = (settings.flags & SMOOTH_LIGHTING) != 0u;
    var light = 1.0;
    if (settings.flags & OLD_LIGHTING) != 0u {
        let subtracted = settings.skylight_subtracted;
        if smooth_lighting {
            light = 0.25 * (sample_brightness(packed.w & 0xffu, subtracted)
                + sample_brightness((packed.w >> 8u) & 0xffu, subtracted)
                + sample_brightness((packed.w >> 16u) & 0xffu, subtracted)
                + sample_brightness(packed.w >> 24u, subtracted));
        } else {
            light = sample_brightness(packed.w & 0xffu, subtracted);
        }
        if ((packed.z >> 30u) & 1u) == 1u {
            light *= face_shade(out.normal);
        }
    }
    var occlusion = 1.0;
    if smooth_lighting {
        occlusion = 1.0 - f32(packed.y >> 30u) * 0.2;
    }
    out.color = vec4(tint * (light * occlusion), 1.0);
    return out;
}

/// Slight wind offset in world units. Phase is the rest position so shared
/// vertices of neighbouring leaf faces stay coincident across chunks.
fn leaf_offset(world_pos: vec3<f32>, time: f32, amplitude: f32) -> vec3<f32> {
    if amplitude == 0.0 {
        return vec3(0.0);
    }
    let phase = world_pos.x * 0.35 + world_pos.z * 0.28;
    let wind = sin(time * 1.1 + phase);
    let gust = sin(time * 0.37 + world_pos.x * 0.11 + world_pos.z * 0.09);
    return vec3(
        wind * amplitude + gust * amplitude * 0.5,
        sin(time * 0.8 + phase) * amplitude * 0.25,
        cos(time * 0.9 + phase * 1.1) * amplitude + gust * amplitude * 0.35,
    );
}

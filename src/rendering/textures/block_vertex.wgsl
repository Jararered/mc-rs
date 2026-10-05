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

// Mirrors the record layout in `meshing/quads.rs`.
const QUAD_STEPS: f32 = 64.0;
const QUAD_WORDS: u32 = 8u;
const QUAD_SHADE: u32 = 0x02000000u;
const QUAD_REPEAT: u32 = 0x04000000u;
const QUAD_SNOW_SIDE: u32 = 0x08000000u;
const QUAD_TRIANGLE: u32 = 0x10000000u;
const QUAD_BRIGHT: u32 = 0x20000000u;

struct DecodedBlockVertex {
    position: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    color: vec4<f32>,
    /// False for a proxy vertex past the layer's last quad.
    visible: bool,
}

struct RepeatUv {
    uv: vec2<f32>,
    alpha: f32,
}

const REPEAT_NORMAL: u32 = 63u;

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

fn face_normal(face: u32) -> vec3<f32> {
    switch face {
        case 0u: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 1u: { return vec3<f32>(0.0, -1.0, 0.0); }
        case 2u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 3u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(0.0, 0.0, -1.0); }
    }
}

/// Block-space coordinates that increase with the face's atlas axes.
/// Wrapping happens in the fragment shader; both corners of a wide quad can
/// sit on integers, so `fract` at the vertex would collapse the tile.
fn face_block_uv(position: vec3<f32>, face: u32) -> vec2<f32> {
    switch face {
        case 0u: { return position.xz; }
        case 1u: { return position.zx; }
        case 2u, 3u: { return vec2<f32>(position.z, -position.y); }
        default: { return vec2<f32>(position.x, -position.y); }
    }
}

fn atlas_uv(tile: vec2<f32>, texel: vec2<f32>) -> vec2<f32> {
    let pad = f32(#{ATLAS_PAD_TEXELS});
    let stride = f32(#{ATLAS_TILE_PX}) + 2.0 * pad;
    // Beta repeats the flowing-fluid frame over 2×2 tiles. Local texels
    // 16..31 must land in the *next padded tile*, not the intervening gutter
    // or the unrelated still-water tile. Keep this in step with AtlasTexel::uv.
    let flowing = tile.x == 14.0 && (tile.y == 12.0 || tile.y == 14.0);
    let step = floor(texel / 16.0) * select(0.0, 1.0, flowing);
    return ((tile + step) * stride + pad + texel - step * 16.0)
        / (f32(#{ATLAS_GRID}) * stride);
}

/// Greedy quads store the tile in vertex alpha as `(tile + 0.5) / 256` and the
/// unwrapped face position in `uv`. Every other face keeps an atlas UV and alpha 1.
fn fix_repeat_uv(uv: vec2<f32>, alpha: f32) -> RepeatUv {
    if alpha >= 0.999 {
        return RepeatUv(uv, alpha);
    }
    let packed = floor(alpha * 256.0);
    let tile_y = floor(packed / 16.0);
    let tile_x = packed - tile_y * 16.0;
    let texel = fract(uv) * 16.0;
    return RepeatUv(atlas_uv(vec2<f32>(tile_x, tile_y), texel), 1.0);
}

fn decode_block_vertex(packed: vec4<u32>, settings: BlockShadingSettings) -> DecodedBlockVertex {
    var out: DecodedBlockVertex;
    out.position = vec3(
        f32(packed.x & 0x1fffu) / HORIZONTAL_STEPS,
        f32(packed.y & 0xfffu) / VERTICAL_STEPS,
        f32((packed.x >> 13u) & 0x1fffu) / HORIZONTAL_STEPS,
    ) + POSITION_MIN;

    let normal_x = (packed.x >> 26u) & 0x3fu;
    let normal_y = (packed.z >> 24u) & 0x3fu;
    let u = (packed.y >> 12u) & 0x1ffu;
    let v = (packed.y >> 21u) & 0x1ffu;
    let tile = vec2<f32>(f32(u >> 5u), f32(v >> 5u));
    let texel = vec2<f32>(f32(u & 31u), f32(v & 31u));
    var alpha = 1.0;
    if normal_x == REPEAT_NORMAL {
        out.normal = face_normal(normal_y);
        out.uv = face_block_uv(out.position, normal_y);
        if texel.x == 1.0 && normal_y >= 2u && normal_y <= 5u {
            // Snow sides are 1/8 high, but still show the whole tile.
            out.uv.y *= 8.0;
        }
        alpha = (tile.x + tile.y * 16.0 + 0.5) / 256.0;
    } else {
        out.normal = decode_normal(vec2(normal_x, normal_y));
        out.uv = atlas_uv(tile, texel);
    }

    out.color = block_color(
        packed.z & 0xffffffu,
        (packed.z >> 31u) == 1u,
        packed.w,
        ((packed.z >> 30u) & 1u) == 1u,
        out.normal,
        alpha,
        settings,
    );
    out.visible = true;
    return out;
}

/// Tint times Beta's light for one vertex. `light` holds four `sky << 4 |
/// block` samples, the flat one in the low byte.
fn block_color(
    srgb_bits: u32,
    bright: bool,
    light_samples: u32,
    shade: bool,
    normal: vec3<f32>,
    alpha: f32,
    settings: BlockShadingSettings,
) -> vec4<f32> {
    let srgb = vec3<f32>(vec3(srgb_bits & 0xffu, (srgb_bits >> 8u) & 0xffu, (srgb_bits >> 16u) & 0xffu));
    var tint = srgb_to_linear(srgb / 255.0);
    if bright {
        tint *= BRIGHT_TINT;
    }

    let smooth_lighting = (settings.flags & SMOOTH_LIGHTING) != 0u;
    var light = 1.0;
    if (settings.flags & OLD_LIGHTING) != 0u {
        let subtracted = settings.skylight_subtracted;
        if smooth_lighting {
            light = 0.25 * (sample_brightness(light_samples & 0xffu, subtracted)
                + sample_brightness((light_samples >> 8u) & 0xffu, subtracted)
                + sample_brightness((light_samples >> 16u) & 0xffu, subtracted)
                + sample_brightness(light_samples >> 24u, subtracted));
        } else {
            light = sample_brightness(light_samples & 0xffu, subtracted);
        }
        if shade {
            light *= face_shade(normal);
        }
    }
    // Beta's smooth lighting is only the average of the four neighbor
    // brightnesses; it has no separate occlusion term. The packed AO bits are
    // left unused. Beta multiplies brightness into gamma-encoded texels, and
    // the atlas is sampled as linear, so encode the factor the same way.
    let factor = srgb_to_linear(vec3(light)).x;
    return vec4(tint * factor, alpha);
}

/// Sign-extend the low `bits` of `value`.
fn sign_extend(value: u32, bits: u32) -> f32 {
    let shift = 32u - bits;
    return f32(i32(value << shift) >> shift);
}

/// One corner of a quad record (`meshing/quads.rs`). `low` and `high` are the
/// record's eight words; `corner` counts around the quad from its origin.
fn decode_block_quad(
    low: vec4<u32>,
    high: vec4<u32>,
    corner_index: u32,
    settings: BlockShadingSettings,
) -> DecodedBlockVertex {
    var out: DecodedBlockVertex;
    let flags = low.y;
    // A triangle draws corners 0, 1, 2; its second triangle collapses.
    let corner = select(corner_index, 0u, (flags & QUAD_TRIANGLE) != 0u && corner_index == 3u);
    let along1 = f32(corner == 1u || corner == 2u);
    let along2 = f32(corner >= 2u);

    let origin = vec3<f32>(vec3(
        low.x & 0x7ffu,
        (low.x >> 11u) & 0x7ffu,
        (low.x >> 22u) | ((low.y & 1u) << 10u),
    ));
    let edge1 = vec3(
        sign_extend(low.y >> 1u, 12u),
        sign_extend(low.y >> 13u, 12u),
        sign_extend(low.z, 12u),
    );
    let edge2 = vec3(
        sign_extend(low.z >> 12u, 12u),
        sign_extend(low.w, 12u),
        sign_extend(low.w >> 12u, 12u),
    );
    out.position = (origin + edge1 * along1 + edge2 * along2) / QUAD_STEPS + POSITION_MIN;

    let normal_mode = (low.w >> 24u) & 7u;
    if normal_mode == 0u {
        out.normal = normalize(cross(edge1, edge2));
    } else {
        out.normal = face_normal(normal_mode - 1u);
    }

    let tile = vec2<f32>(f32((low.z >> 24u) & 15u), f32(low.z >> 28u));
    var alpha = 1.0;
    if (flags & QUAD_REPEAT) != 0u {
        let face = normal_mode - 1u;
        out.uv = face_block_uv(out.position, face);
        if (flags & QUAD_SNOW_SIDE) != 0u && face >= 2u && face <= 5u {
            // Snow sides are 1/8 high, but still show the whole tile.
            out.uv.y *= 8.0;
        }
        alpha = (tile.x + tile.y * 16.0 + 0.5) / 256.0;
    } else {
        let texel = vec2<f32>(f32(low.w >> 27u), f32(high.x & 31u));
        let texel1 = vec2(sign_extend(high.x >> 5u, 6u), sign_extend(high.x >> 11u, 6u));
        let texel2 = vec2(sign_extend(high.x >> 17u, 6u), sign_extend(high.x >> 23u, 6u));
        out.uv = atlas_uv(tile, texel + texel1 * along1 + texel2 * along2);
    }

    // The centre, the mids on either side of this corner, and its diagonal.
    var mids = array<u32, 4>(
        (high.z >> 8u) & 0xffu,
        (high.z >> 16u) & 0xffu,
        high.z >> 24u,
        high.w & 0xffu,
    );
    var diagonals = array<u32, 4>(
        (high.w >> 8u) & 0xffu,
        (high.w >> 16u) & 0xffu,
        high.w >> 24u,
        high.y >> 24u,
    );
    let light = (high.z & 0xffu)
        | (mids[(corner + 3u) & 3u] << 8u)
        | (mids[corner] << 16u)
        | (diagonals[corner] << 24u);
    out.color = block_color(
        high.y & 0xffffffu,
        (flags & QUAD_BRIGHT) != 0u,
        light,
        (flags & QUAD_SHADE) != 0u,
        out.normal,
        alpha,
        settings,
    );
    out.visible = true;
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

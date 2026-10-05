#import bevy_pbr::{
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::main_pass_post_lighting_processing,
    mesh_functions,
    mesh_view_bindings::globals,
}

/// `x` is the layer: 0 a lit skin, 1 a glow (spider eyes), 2 an additive
/// charge. `y` scrolls the texture, in UV units per second.
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> creature_params: vec4<f32>;

/// Per-box state packed in `MeshTag`. See `creature_tag` in `shading.rs`.
struct Tag {
    brightness: f32,
    hurt: f32,
    flash: f32,
    alpha: f32,
}

fn creature_tag(instance_index: u32) -> Tag {
    let packed = unpack4x8unorm(mesh_functions::get_tag(instance_index));
    return Tag(1.0 - packed.x, packed.y, packed.z, 1.0 - packed.w);
}

/// `RenderHelper.enableStandardItemLighting`: two lights fixed in the world,
/// each with 0.6 diffuse, over 0.4 ambient.
fn item_lighting(normal: vec3<f32>) -> f32 {
    let first = normalize(vec3(0.2, 1.0, -0.7));
    let second = normalize(vec3(-0.2, 1.0, 0.7));
    return 0.4 + 0.6 * (max(dot(normal, first), 0.0) + max(dot(normal, second), 0.0));
}

/// sRGB-to-linear for a scalar shading factor.
fn gamma_factor(value: f32) -> f32 {
    return select(pow((value + 0.055) / 1.055, 2.4), value / 12.92, value <= 0.04045);
}

@fragment
fn fragment(vertex: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex;
#ifdef VERTEX_UVS_A
    in.uv += vec2(globals.time * creature_params.y);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let tag = creature_tag(in.instance_index);
    let mode = u32(creature_params.x + 0.5);
    let shade = item_lighting(pbr_input.N);
    var out: FragmentOutput;

    if mode == 2u {
        // `RenderCreeper`'s charge: half-bright, unlit, added on top.
        let base = pbr_input.material.base_color;
        out.color = vec4(base.rgb * 0.5, 1.0);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }
    if mode == 1u {
        // `RenderSpider`'s eyes: lit but not darkened, fading in at night.
        let base = pbr_input.material.base_color;
        out.color = vec4(base.rgb * min(shade, 1.0), base.a * tag.alpha);
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
        return out;
    }

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    let base = pbr_input.material.base_color;
    var color: vec3<f32>;
    var hurt_tone: vec3<f32>;
    var flash_tone: vec3<f32>;
    {
        // Fixed-function lighting clamps the lit vertex color before it
        // modulates the texture. Both overlays are lit the same way.
        // The factor is gamma-encoded in Beta, so convert it before it
        // multiplies the linear texture.
        let light = gamma_factor(min(tag.brightness * shade, 1.0));
        color = base.rgb * light;
        hurt_tone = vec3(light, 0.0, 0.0);
        flash_tone = vec3(gamma_factor(min(shade, 1.0)));
    }
    // `RenderLiving`'s red pass while hurt or dying, then a creeper's flash.
    color = mix(color, hurt_tone, tag.hurt * 0.4);
    color = mix(color, flash_tone, tag.flash);
    out.color = vec4(color, base.a * tag.alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

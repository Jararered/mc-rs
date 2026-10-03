#import bevy_pbr::{
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    mesh_functions,
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let low = color / 12.92;
    let high = pow((color + 0.055) / 1.055, vec3(2.4));
    return select(high, low, color <= vec3(0.04045));
}

/// `MeshTag` holds the complement of an sRGB tint in RGB and the entity's
/// world brightness in alpha. See `creature_tag` in `shading.rs`.
fn creature_tint(instance_index: u32) -> vec4<f32> {
    let tag = unpack4x8unorm(~mesh_functions::get_tag(instance_index));
    return vec4(srgb_to_linear(tag.rgb), tag.a);
}

/// `RenderHelper.enableStandardItemLighting`: two lights fixed in the world,
/// each with 0.6 diffuse, over 0.4 ambient.
fn item_lighting(normal: vec3<f32>) -> f32 {
    let first = normalize(vec3(0.2, 1.0, -0.7));
    let second = normalize(vec3(-0.2, 1.0, 0.7));
    return 0.4 + 0.6 * (max(dot(normal, first), 0.0) + max(dot(normal, second), 0.0));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    let tint = creature_tint(in.instance_index);
    let base = pbr_input.material.base_color;

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        pbr_input.material.base_color = vec4(base.rgb * tint.rgb, base.a);
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        // Fixed-function lighting clamps the lit vertex color before it
        // modulates the texture.
        let light = min(tint.rgb * tint.a * item_lighting(pbr_input.N), vec3(1.0));
        out.color = vec4(base.rgb * light, base.a);
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// Beta's fixed-function fog blends in gamma space: the framebuffer holds
// gamma-encoded values and GL mixes them directly. Bevy mixes in linear light,
// which makes bright fog over dark terrain look much denser for the same
// factor. This blends gamma-encoded colors and then hands the result to
// Bevy's post-lighting step with its own fog switched off.
#define_import_path game::gamma_fog

#import bevy_pbr::{
    pbr_types,
    pbr_functions::main_pass_post_lighting_processing,
    mesh_view_bindings::{view, fog},
    mesh_view_types::{FOG_MODE_LINEAR, FOG_MODE_EXPONENTIAL, FOG_MODE_EXPONENTIAL_SQUARED},
}

fn encode(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3(0.0)), vec3(1.0 / 2.2));
}

fn decode(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3(0.0)), vec3(2.2));
}

fn gamma_fog_post_lighting(pbr_input: pbr_types::PbrInput, input_color: vec4<f32>) -> vec4<f32> {
    var input = pbr_input;
    var color = input_color;
#ifdef DISTANCE_FOG
    if (input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT) != 0u {
        let distance = length(view.world_position.xyz - input.world_position.xyz);
        var amount = 0.0;
        if fog.mode == FOG_MODE_LINEAR {
            amount = 1.0 - clamp((fog.be.y - distance) / (fog.be.y - fog.be.x), 0.0, 1.0);
        } else if fog.mode == FOG_MODE_EXPONENTIAL {
            amount = 1.0 - 1.0 / exp(distance * fog.be.x);
        } else if fog.mode == FOG_MODE_EXPONENTIAL_SQUARED {
            let d = distance * fog.be.x;
            amount = 1.0 - 1.0 / exp(d * d);
        }
        amount *= fog.base_color.a;
        color = vec4(decode(mix(encode(color.rgb), encode(fog.base_color.rgb), amount)), color.a);
        input.material.flags &= ~pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    }
#endif
    return main_pass_post_lighting_processing(input, color);
}

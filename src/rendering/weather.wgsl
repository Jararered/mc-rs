#import bevy_pbr::{
    pbr_functions::alpha_discard,
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::main_pass_post_lighting_processing,
    mesh_functions,
    mesh_view_bindings::{globals, view},
}

/// World ticks per second; the scroll rates in `uv_b` are per tick.
const TICKS_PER_SECOND: f32 = 20.0;

/// Rain and snow columns from `EntityRenderer.renderRainSnow`. `uv_b` is the
/// column's texture scroll per tick. `MeshTag` packs the rain strength in
/// bits 0..8, the column radius in bits 8..16, and the snow flag in bit 16.
/// See `weather.rs`.
@fragment
fn fragment(vertex: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex;
    let tag = mesh_functions::get_tag(in.instance_index);
    let strength = f32(tag & 0xffu) / 255.0;
    let radius = max(f32((tag >> 8u) & 0xffu), 1.0);
    let snow = ((tag >> 16u) & 1u) == 1u;

    in.uv = in.uv + fract(in.uv_b * (globals.time * TICKS_PER_SECOND));

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let offset = (in.world_position.xz - view.world_position.xz) / radius;
    let edge = 1.0 - dot(offset, offset);
    let fade = select(edge * 0.5 + 0.5, edge * 0.3 + 0.5, snow);
    pbr_input.material.base_color.a *= max(fade, 0.0) * strength;
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = pbr_input.material.base_color;
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}

// Bevy's unlit material sampling and fog, with atlas UV wrapping for greedy quads.
#import bevy_pbr::{
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
}
#import game::block_vertex::fix_repeat_uv

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var in = vertex_output;
    let repeat = fix_repeat_uv(in.uv, in.color.a);
    in.uv = repeat.uv;
    in.color.a = repeat.alpha;

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, pbr_input.material.base_color);
    return out;
}

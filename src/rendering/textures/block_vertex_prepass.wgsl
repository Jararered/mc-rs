#import bevy_pbr::{
    mesh_functions,
    prepass_io::VertexOutput,
    view_transformations::position_world_to_clip,
}
#import bevy_render::globals::Globals
#import game::block_vertex::{BlockShadingSettings, decode_block_vertex, leaf_offset}

// The prepass view bind group holds `Globals` at binding 1, but no Bevy
// shader module declares it there for prepass vertex shaders.
@group(0) @binding(1)
var<uniform> prepass_globals: Globals;

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> block_shading: BlockShadingSettings;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) packed: vec4<u32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let decoded = decode_block_vertex(vertex.packed, block_shading);
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4(decoded.position, 1.0),
    );
    out.world_position = vec4(
        world_position.xyz + leaf_offset(
            world_position.xyz,
            prepass_globals.time,
            block_shading.wiggle_amplitude,
        ),
        world_position.w,
    );
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0); // Clamp depth to avoid clipping
#endif

#ifdef VERTEX_UVS_A
    out.uv = decoded.uv;
#endif

#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        decoded.normal,
        vertex.instance_index,
    );
#endif

#ifdef VERTEX_COLORS
    out.color = decoded.color;
#endif

#ifdef MOTION_VECTOR_PREPASS
    let previous_from_local = mesh_functions::get_previous_world_from_local(vertex.instance_index);
    let previous_position = mesh_functions::mesh_position_local_to_world(
        previous_from_local,
        vec4(decoded.position, 1.0),
    );
    out.previous_world_position = vec4(
        previous_position.xyz + leaf_offset(
            previous_position.xyz,
            prepass_globals.time - prepass_globals.delta_time,
            block_shading.wiggle_amplitude,
        ),
        previous_position.w,
    );
#endif

#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif

#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
#endif
    return out;
}

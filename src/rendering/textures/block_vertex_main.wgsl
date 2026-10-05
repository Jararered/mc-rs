#import bevy_pbr::{
    mesh_functions,
    forward_io::VertexOutput,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
}
#import game::block_vertex::{
    BlockShadingSettings, DecodedBlockVertex, QUAD_WORDS, decode_block_quad, decode_block_vertex,
    leaf_offset,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> block_shading: BlockShadingSettings;

#ifdef PULLED_QUADS
// Chunk layers: every quad record of every layer, each layer behind a header
// record that holds its quad count. `MeshTag` is the layer's header index.
@group(#{MATERIAL_BIND_GROUP}) @binding(101)
var<storage, read> chunk_quads: array<u32>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // `quad << 2 | corner` from the shared proxy mesh.
    @location(0) quad_corner: u32,
};

fn decode(vertex: Vertex) -> DecodedBlockVertex {
    let header = mesh_functions::get_tag(vertex.instance_index) * QUAD_WORDS;
    let quad = vertex.quad_corner >> 2u;
    if quad >= chunk_quads[header] {
        var hidden: DecodedBlockVertex;
        hidden.visible = false;
        return hidden;
    }
    let at = header + (quad + 1u) * QUAD_WORDS;
    return decode_block_quad(
        vec4(chunk_quads[at], chunk_quads[at + 1u], chunk_quads[at + 2u], chunk_quads[at + 3u]),
        vec4(chunk_quads[at + 4u], chunk_quads[at + 5u], chunk_quads[at + 6u], chunk_quads[at + 7u]),
        vertex.quad_corner & 3u,
        block_shading,
    );
}
#else
struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) packed: vec4<u32>,
};

fn decode(vertex: Vertex) -> DecodedBlockVertex {
    return decode_block_vertex(vertex.packed, block_shading);
}
#endif

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let decoded = decode(vertex);
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4(decoded.position, 1.0),
    );
    // Bevy's own time uniform drives the leaf wiggle, so no material changes
    // per frame.
    out.world_position = vec4(
        world_position.xyz + leaf_offset(world_position.xyz, globals.time, block_shading.wiggle_amplitude),
        world_position.w,
    );
    out.position = position_world_to_clip(out.world_position.xyz);
    if !decoded.visible {
        // Every vertex of an unused proxy quad lands on one point, so its
        // triangles have no area.
        out.position = vec4(0.0, 0.0, 0.0, 1.0);
    }
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        decoded.normal,
        vertex.instance_index,
    );
#ifdef VERTEX_UVS_A
    out.uv = decoded.uv;
#endif
#ifdef VERTEX_COLORS
    out.color = decoded.color;
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

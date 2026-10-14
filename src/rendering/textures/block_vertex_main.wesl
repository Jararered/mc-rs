import bevy_pbr::render::{
    mesh_functions,
    forward_io::VertexOutput,
    mesh_view_bindings::globals,
    view_transformations::position_world_to_clip,
};
import package::rendering::textures::block_vertex::{
    BlockShadingSettings, DecodedBlockVertex, QUAD_WORDS, decode_block_quad, decode_block_vertex,
    leaf_offset,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> block_shading: BlockShadingSettings;

// Chunk layers: every quad record of every layer, each layer behind a header
// record that holds its quad count. `MeshTag` is the layer's header index.
@if(PULLED_QUADS)
@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var<storage, read> chunk_quads: array<u32>;

@if(PULLED_QUADS)
struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // `quad << 2 | corner` from the shared proxy mesh.
    @location(0) quad_corner: u32,
};

@if(PULLED_QUADS && !REGION_DRAWS)
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

// A chunk region: `MeshTag` is its draw list's header record, which holds
// the entry count and the quads drawn. Each entry after it is a run of one
// section layer's records: the quad count up to the run's end, its first
// record, and the section's offset in the region (`chunk_regions.rs`).
@if(REGION_DRAWS)
fn decode(vertex: Vertex) -> DecodedBlockVertex {
    let header = mesh_functions::get_tag(vertex.instance_index) * QUAD_WORDS;
    let quad = vertex.quad_corner >> 2u;
    if quad >= chunk_quads[header + 1u] {
        var hidden: DecodedBlockVertex;
        hidden.visible = false;
        return hidden;
    }
    // The first entry whose run ends past this quad.
    let entries = header + QUAD_WORDS;
    var low = 0u;
    var high = chunk_quads[header] - 1u;
    while low < high {
        let middle = (low + high) / 2u;
        if chunk_quads[entries + middle * 4u] > quad {
            high = middle;
        } else {
            low = middle + 1u;
        }
    }
    let entry = entries + low * 4u;
    var begin = 0u;
    if low > 0u {
        begin = chunk_quads[entry - 4u];
    }
    let at = (chunk_quads[entry + 1u] + quad - begin) * QUAD_WORDS;
    let offset = chunk_quads[entry + 2u];
    var decoded = decode_block_quad(
        vec4(chunk_quads[at], chunk_quads[at + 1u], chunk_quads[at + 2u], chunk_quads[at + 3u]),
        vec4(chunk_quads[at + 4u], chunk_quads[at + 5u], chunk_quads[at + 6u], chunk_quads[at + 7u]),
        vertex.quad_corner & 3u,
        block_shading,
    );
    decoded.position += vec3(
        f32(offset & 7u),
        f32((offset >> 3u) & 7u),
        f32((offset >> 6u) & 7u),
    ) * 16.0;
    return decoded;
}

@if(!PULLED_QUADS)
struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) packed: vec4<u32>,
};

@if(!PULLED_QUADS)
fn decode(vertex: Vertex) -> DecodedBlockVertex {
    return decode_block_vertex(vertex.packed, block_shading);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let decoded = decode(vertex);
    if !decoded.visible {
        // Every vertex of an unused proxy quad lands on one point, so its
        // triangles have no area. The rest of a proxy past the list's end is
        // all such quads, so skip the transforms for them.
        out.position = vec4(0.0, 0.0, 0.0, 1.0);
        return out;
    }
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
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        decoded.normal,
        vertex.instance_index,
    );
@if(VERTEX_UVS_A)
    out.uv = decoded.uv;
@if(VERTEX_COLORS)
    out.color = decoded.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index,
        world_from_local[3],
    );
    return out;
}

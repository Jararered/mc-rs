#define_import_path game::leaf_wiggle

/// Slight wind offset in world units. Phase is the rest position so shared
/// vertices of neighbouring leaf faces stay coincident across chunks.
fn leaf_offset(world_pos: vec3<f32>, time: f32, amplitude: f32) -> vec3<f32> {
    let phase = world_pos.x * 0.35 + world_pos.z * 0.28;
    let wind = sin(time * 1.1 + phase);
    let gust = sin(time * 0.37 + world_pos.x * 0.11 + world_pos.z * 0.09);
    return vec3(
        wind * amplitude + gust * amplitude * 0.5,
        sin(time * 0.8 + phase) * amplitude * 0.25,
        cos(time * 0.9 + phase * 1.1) * amplitude + gust * amplitude * 0.35,
    );
}

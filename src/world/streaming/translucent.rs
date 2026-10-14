//! Water drawn back to front. A section's blended records are sorted
//! farthest from the camera first and written over their range in the quad
//! buffer. Sections near the camera sort again whenever it enters another
//! block; farther ones only when it enters another section, since the order
//! of their quads barely changes in between.

use std::time::Instant;

use bevy::prelude::*;

use super::StreamingDiagnostics;
use super::WorldStreaming;
use crate::player::PlayerCamera;
use crate::rendering::chunk_quads::ChunkQuadWrites;
use crate::rendering::chunk_quads::ChunkQuads;
use crate::rendering::chunk_regions::section_origin;
use crate::rendering::meshing::layer_bytes;
use crate::rendering::meshing::sort_back_to_front;
use crate::rendering::occlusion::camera_section;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;

/// Chunks around the camera whose water sorts on every block it moves.
const NEAR_CHUNKS: i32 = 3;
/// Most sections sorted in one frame, nearest first. The rest wait.
const MAX_SORTS_PER_FRAME: usize = 64;

/// Whether a layer last sorted for the camera at `sorted` must sort again
/// for the camera at `eye`.
pub fn needs_sort(
    sorted: Option<IVec3>,
    eye: IVec3,
    camera_chunk: ChunkPosition,
    position: ChunkPosition,
) -> bool {
    let Some(sorted) = sorted else {
        return true;
    };
    let near = (position.x - camera_chunk.x).abs() <= NEAR_CHUNKS
        && (position.z - camera_chunk.z).abs() <= NEAR_CHUNKS;
    if near {
        sorted != eye
    } else {
        camera_section(sorted.as_vec3()) != camera_section(eye.as_vec3())
    }
}

pub(crate) fn sort_translucent_layers(
    mut streaming: ResMut<WorldStreaming>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    quads: Option<ResMut<ChunkQuads>>,
    perf: Option<ResMut<StreamingDiagnostics>>,
    mut due: Local<Vec<(f32, ChunkPosition, usize)>>,
    mut last: Local<Option<(IVec3, u64)>>,
) {
    let (Some(mut quads), Ok(camera)) = (quads, camera.single()) else {
        return;
    };
    let streaming = streaming.bypass_change_detection();
    let eye = camera.translation();
    let block = eye.floor().as_ivec3();
    // Nothing moved and nothing new arrived; a capped frame leaves `last`
    // unset so the rest sort next frame.
    if *last == Some((block, streaming.layer_revision)) {
        return;
    }
    let start = Instant::now();
    let camera_chunk = ChunkPosition::from_world(eye.x, eye.z);
    due.clear();
    for (&position, rendered) in &mut streaming.rendered {
        for (section, _, translucent) in rendered.translucent_layers() {
            if needs_sort(translucent.sorted_for, block, camera_chunk, position) {
                let centre = section_origin(position, section) + SECTION_HEIGHT as f32 / 2.0;
                due.push((centre.distance_squared(eye), position, section));
            }
        }
    }
    due.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let capped = due.len() > MAX_SORTS_PER_FRAME;
    for &(_, position, section) in due.iter().take(MAX_SORTS_PER_FRAME) {
        let Some(rendered) = streaming.rendered.get_mut(&position) else {
            continue;
        };
        for (layer_section, range, translucent) in rendered.translucent_layers() {
            if layer_section != section {
                continue;
            }
            let local = eye - section_origin(position, section);
            sort_back_to_front(&mut translucent.records, local.to_array());
            quads.rewrite(
                range,
                layer_bytes(&translucent.records, &translucent.facings),
            );
            translucent.sorted_for = Some(block);
        }
    }
    *last = (!capped).then_some((block, streaming.layer_revision));
    if !due.is_empty()
        && let Some(mut perf) = perf
    {
        perf.sorting.record(start.elapsed());
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        sort_translucent_layers
            .in_set(ChunkQuadWrites)
            .after(TransformSystems::Propagate)
            .run_if(resource_exists::<WorldStreaming>),
    );
}

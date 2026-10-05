//! The GPU buffer that holds every chunk layer's quad records.
//!
//! A chunk layer is not a mesh of its own. Its quads are 32-byte records
//! (`meshing::quads`) in one storage buffer bound on the block materials, and
//! the layer entity draws a shared proxy mesh with its range's start in
//! `MeshTag`. [`ChunkQuads`] hands out ranges, queues the bytes for the
//! render world, and keeps the proxy meshes.
//!
//! The buffer has no main-world copy. Writes are extracted with the frame
//! that carries the tags pointing at them and applied with
//! `RenderQueue::write_buffer`; growing resizes the buffer asset, which Bevy
//! copies on the GPU. The resource only exists with a renderer, so headless
//! apps keep meshing chunks in the packed vertex format.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::MainWorld;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::RenderDevice;
use bevy::render::renderer::RenderQueue;
use bevy::render::storage::GpuShaderBuffer;
use bevy::render::storage::ShaderBuffer;

use super::meshing::QUAD_WORDS;
use super::meshing::proxy_capacity;
use super::meshing::proxy_mesh;

/// Bytes in one record.
pub const RECORD_BYTES: usize = QUAD_WORDS * 4;
/// Records the buffer starts with: 8 MiB.
const INITIAL_RECORDS: u32 = 1 << 18;
/// Fewest records a growth adds: 16 MiB. Each growth copies the buffer on the
/// GPU, so small steps are not worth their copies.
const GROWTH_RECORDS: u32 = 1 << 19;
/// Frames the block materials are rebound for after the buffer is replaced.
/// A material prepared in the same frame as the new buffer may still have
/// seen the old one.
const REBIND_FRAMES: u8 = 2;

/// Free record ranges, merged with their neighbors and found best-fit first.
#[derive(Default)]
pub struct FreeRanges {
    by_start: BTreeMap<u32, u32>,
    by_length: BTreeSet<(u32, u32)>,
}

impl FreeRanges {
    /// The start of the smallest free range that holds `length` records.
    pub fn allocate(&mut self, length: u32) -> Option<u32> {
        let &(found, start) = self.by_length.range((length, 0)..).next()?;
        self.by_length.remove(&(found, start));
        self.by_start.remove(&start);
        if found > length {
            self.insert(start + length, found - length);
        }
        Some(start)
    }

    /// Return `length` records at `start`, joining adjacent free ranges.
    pub fn release(&mut self, mut start: u32, mut length: u32) {
        if length == 0 {
            return;
        }
        if let Some((&before, &before_length)) = self.by_start.range(..start).next_back()
            && before + before_length == start
        {
            self.remove(before, before_length);
            start = before;
            length += before_length;
        }
        if let Some(&after_length) = self.by_start.get(&(start + length)) {
            self.remove(start + length, after_length);
            length += after_length;
        }
        self.insert(start, length);
    }

    /// Records in all free ranges.
    pub fn free(&self) -> u64 {
        self.by_start
            .values()
            .map(|length| u64::from(*length))
            .sum()
    }

    fn insert(&mut self, start: u32, length: u32) {
        self.by_start.insert(start, length);
        self.by_length.insert((length, start));
    }

    fn remove(&mut self, start: u32, length: u32) {
        self.by_start.remove(&start);
        self.by_length.remove(&(length, start));
    }
}

/// One layer's records in the buffer, header included. Hand it back with
/// [`ChunkQuads::free`]; dropping it leaks the range until the world unloads.
#[derive(Debug)]
pub struct QuadRange {
    start: u32,
    records: u32,
}

impl QuadRange {
    /// The header record's index, which the layer's `MeshTag` carries.
    pub fn tag(&self) -> u32 {
        self.start
    }

    pub fn bytes(&self) -> usize {
        self.records as usize * RECORD_BYTES
    }
}

struct QuadWrite {
    start: u32,
    bytes: Vec<u8>,
}

#[derive(Resource)]
pub struct ChunkQuads {
    buffer: Handle<ShaderBuffer>,
    free: FreeRanges,
    /// Records the allocator hands out from.
    capacity: u32,
    /// Records the buffer asset was last sized for.
    sized: u32,
    /// Most records the device lets one storage binding hold.
    limit: u32,
    pending: Vec<QuadWrite>,
    proxies: HashMap<u32, Handle<Mesh>>,
    rebind_frames: u8,
}

impl ChunkQuads {
    fn new(buffers: &mut Assets<ShaderBuffer>) -> Self {
        let buffer = buffers.add(ShaderBuffer::with_size(
            INITIAL_RECORDS as usize * RECORD_BYTES,
            RenderAssetUsages::RENDER_WORLD,
        ));
        let mut free = FreeRanges::default();
        free.release(0, INITIAL_RECORDS);
        Self {
            buffer,
            free,
            capacity: INITIAL_RECORDS,
            sized: INITIAL_RECORDS,
            limit: INITIAL_RECORDS,
            pending: Vec::new(),
            proxies: HashMap::new(),
            rebind_frames: 0,
        }
    }

    /// The buffer every block material binds.
    pub fn buffer(&self) -> Handle<ShaderBuffer> {
        self.buffer.clone()
    }

    /// Store one layer's bytes (`meshing::layer_bytes`). `None` when the
    /// device cannot hold a buffer large enough.
    pub fn upload(&mut self, bytes: Vec<u8>) -> Option<QuadRange> {
        let records = u32::try_from(bytes.len() / RECORD_BYTES).ok()?;
        let start = loop {
            if let Some(start) = self.free.allocate(records) {
                break start;
            }
            if self.capacity >= self.limit {
                return None;
            }
            // A quarter at a time keeps the unused tail small next to the
            // records in use.
            let step = (self.capacity / 4).max(GROWTH_RECORDS).max(records);
            let grown = self.capacity.saturating_add(step).min(self.limit);
            self.free.release(self.capacity, grown - self.capacity);
            self.capacity = grown;
        };
        self.pending.push(QuadWrite { start, bytes });
        Some(QuadRange { start, records })
    }

    pub fn free(&mut self, range: QuadRange) {
        self.free.release(range.start, range.records);
    }

    /// The shared mesh a layer of `quads` quads draws.
    pub fn proxy(&mut self, meshes: &mut Assets<Mesh>, quads: u32) -> Handle<Mesh> {
        let capacity = proxy_capacity(quads);
        self.proxies
            .entry(capacity)
            .or_insert_with(|| meshes.add(proxy_mesh(capacity)))
            .clone()
    }

    /// Bytes of the buffer, and of the ranges layers hold in it.
    pub fn memory(&self) -> (usize, usize) {
        let reserved = self.capacity as usize * RECORD_BYTES;
        let free = usize::try_from(self.free.free()).unwrap_or(usize::MAX) * RECORD_BYTES;
        (reserved, reserved.saturating_sub(free))
    }

    /// Resize the buffer asset to the allocator's capacity. Returns whether
    /// the block materials must be rebound this frame.
    pub fn sync(&mut self, buffers: &mut Assets<ShaderBuffer>, device: &RenderDevice) -> bool {
        let limit = device.limits().max_storage_buffer_binding_size / RECORD_BYTES as u64;
        // The shader indexes words with a `u32`.
        self.limit = u32::try_from(limit)
            .unwrap_or(u32::MAX)
            .min(u32::MAX / QUAD_WORDS as u32);
        if self.capacity != self.sized
            && let Some(mut buffer) = buffers.get_mut(&self.buffer)
        {
            buffer.resize_in_place(self.capacity as u64 * RECORD_BYTES as u64);
            self.sized = self.capacity;
            self.rebind_frames = REBIND_FRAMES;
        }
        if self.rebind_frames == 0 {
            return false;
        }
        self.rebind_frames -= 1;
        true
    }
}

/// Writes taken from the main world, waiting for the GPU buffer.
#[derive(Resource, Default)]
struct QueuedQuadWrites {
    buffer: Option<AssetId<ShaderBuffer>>,
    writes: Vec<QuadWrite>,
}

fn extract_quad_writes(mut main_world: ResMut<MainWorld>, mut queued: ResMut<QueuedQuadWrites>) {
    let Some(mut quads) = main_world.get_resource_mut::<ChunkQuads>() else {
        return;
    };
    queued.buffer = Some(quads.buffer.id());
    queued.writes.append(&mut quads.pending);
}

/// Runs after asset preparation, so a resized buffer is already in place. A
/// write past the current buffer waits for the resize that covers it.
fn write_quads(
    mut queued: ResMut<QueuedQuadWrites>,
    buffers: Res<RenderAssets<GpuShaderBuffer>>,
    queue: Res<RenderQueue>,
) {
    let Some(gpu) = queued.buffer.and_then(|id| buffers.get(id)) else {
        return;
    };
    let size = gpu.buffer.size();
    queued.writes.retain(|write| {
        let offset = u64::from(write.start) * RECORD_BYTES as u64;
        if offset + write.bytes.len() as u64 > size {
            return true;
        }
        queue.write_buffer(&gpu.buffer, offset, &write.bytes);
        false
    });
}

pub(super) fn plugin(app: &mut App) {
    let Some(mut buffers) = app.world_mut().get_resource_mut::<Assets<ShaderBuffer>>() else {
        return;
    };
    let quads = ChunkQuads::new(&mut buffers);
    app.insert_resource(quads);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .init_resource::<QueuedQuadWrites>()
            .add_systems(ExtractSchedule, extract_quad_writes)
            .add_systems(Render, write_quads.in_set(RenderSystems::PrepareResources));
    }
}

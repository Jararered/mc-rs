//! Chunk regions: one draw per layer for 8×8 chunk columns.
//!
//! A section layer's quad records stay where streaming uploaded them in the
//! quad buffer (`rendering::chunk_quads`). Instead of one entity per section
//! layer, each region has one entity per layer, and that entity's `MeshTag`
//! names a draw list, also kept in the quad buffer:
//!
//! | record | words                                                          |
//! |--------|----------------------------------------------------------------|
//! | 0      | entry count, quads drawn, then zeros                           |
//! | 1..    | two entries per record: quad end, first record, section offset, 0 |
//!
//! Each entry is a run of one section layer's records. `quad end` counts the
//! quads drawn up to and including the run, so the vertex shader finds a
//! proxy quad's run by binary search, then reads its record and moves it by
//! the section's offset inside the region (`block_vertex_main.wesl`).
//!
//! Lists are rebuilt on the CPU for the camera: a section is listed only if
//! cave culling reached it and its layer's bounds touch the frustum, and only
//! the facing groups (`meshing::quads`) that can face the camera are listed.
//! Opaque layers list sections nearest first; water lists them farthest first.

use bevy::camera::primitives::Aabb;
use bevy::camera::primitives::Frustum;
use bevy::prelude::*;

use super::meshing::FACINGS;
use super::meshing::QUAD_WORDS;
use super::meshing::UNASSIGNED_FACING;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::SECTION_HEIGHT;

/// Set to `off` to draw each section layer as its own entity again.
pub const CHUNK_REGIONS_ENV: &str = "MC_CHUNK_REGIONS";

/// Chunk columns along each horizontal side of a region.
pub const REGION_CHUNKS: i32 = 8;
/// `u32` words in one draw-list entry.
const ENTRY_WORDS: usize = 4;
const ENTRIES_PER_RECORD: u32 = (QUAD_WORDS / ENTRY_WORDS) as u32;

/// Whether chunk layers are batched into regions, unless the environment
/// turns it off.
pub fn regions_enabled() -> bool {
    !std::env::var(CHUNK_REGIONS_ENV).is_ok_and(|value| value == "off")
}

/// The region a chunk belongs to.
pub fn region_of(position: ChunkPosition) -> IVec2 {
    IVec2::new(
        position.x.div_euclid(REGION_CHUNKS),
        position.z.div_euclid(REGION_CHUNKS),
    )
}

/// The world position of a region's corner, where its entities stand.
pub fn region_origin(region: IVec2) -> Vec3 {
    let size = (REGION_CHUNKS * CHUNK_SIZE as i32) as f32;
    Vec3::new(region.x as f32 * size, 0.0, region.y as f32 * size)
}

/// Bounds of everything a region can draw, around its origin. Section
/// geometry may reach `POSITION_MIN` blocks past its section, and leaves sway.
pub fn region_aabb() -> Aabb {
    let size = (REGION_CHUNKS * CHUNK_SIZE as i32) as f32;
    let margin = 9.0;
    Aabb::from_min_max(
        Vec3::splat(-margin),
        Vec3::new(size, crate::world::chunk::CHUNK_HEIGHT as f32, size) + margin,
    )
}

/// A section's place in its region: chunk x, section y, chunk z, three bits
/// each. The shader multiplies them back into blocks.
pub fn section_offset(position: ChunkPosition, section: usize) -> u32 {
    let x = position.x.rem_euclid(REGION_CHUNKS) as u32;
    let z = position.z.rem_euclid(REGION_CHUNKS) as u32;
    x | (section as u32) << 3 | z << 6
}

/// The world position of a section's corner, which its layer bounds are
/// relative to.
pub fn section_origin(position: ChunkPosition, section: usize) -> Vec3 {
    let (x, z) = position.world_origin();
    Vec3::new(x, (section * SECTION_HEIGHT) as f32, z)
}

/// The facing groups of a layer whose bounds are `min..max` that can face a
/// camera at `eye`, as a mask over `meshing::FACINGS`. A quad facing `+x`
/// lies at or past `min.x`, and is seen only from beyond its plane.
pub fn visible_facings(eye: Vec3, min: Vec3, max: Vec3) -> u8 {
    let mut mask = 1 << UNASSIGNED_FACING;
    // `geometry::FACE_*` order: up, down, east, west, south, north.
    for (facing, visible) in [
        eye.y > min.y,
        eye.y < max.y,
        eye.x > min.x,
        eye.x < max.x,
        eye.z > min.z,
        eye.z < max.z,
    ]
    .into_iter()
    .enumerate()
    {
        if visible {
            mask |= 1 << facing;
        }
    }
    mask
}

/// One section layer as streaming keeps it, for a region to list.
#[derive(Clone, Copy, Debug)]
pub struct SectionLayer {
    /// The layer's header record in the quad buffer.
    pub header: u32,
    /// Quads in each facing group, in record order.
    pub facings: [u32; FACINGS],
    /// Bounds relative to the section's corner.
    pub aabb: Aabb,
}

impl SectionLayer {
    /// Bounds in world space.
    pub fn world_bounds(&self, position: ChunkPosition, section: usize) -> (Vec3, Vec3) {
        let origin = section_origin(position, section);
        (
            Vec3::from(self.aabb.min()) + origin,
            Vec3::from(self.aabb.max()) + origin,
        )
    }

    /// Whether any of it is inside `frustum`.
    pub fn in_frustum(&self, frustum: &Frustum, position: ChunkPosition, section: usize) -> bool {
        let (min, max) = self.world_bounds(position, section);
        frustum.intersects_obb_identity(&Aabb::from_min_max(min, max))
    }
}

/// The runs one region layer draws this frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrawList {
    entries: Vec<[u32; ENTRY_WORDS]>,
    quads: u32,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.quads = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.quads == 0
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Quads the list draws.
    pub fn quads(&self) -> u32 {
        self.quads
    }

    /// List the facing groups of `layer` in `mask`, joining neighboring
    /// groups into one run.
    pub fn push(&mut self, layer: &SectionLayer, mask: u8, offset: u32) {
        let mut record = layer.header + 1;
        let mut open: Option<usize> = None;
        for (facing, &count) in layer.facings.iter().enumerate() {
            if count == 0 {
                continue;
            }
            if mask >> facing & 1 == 0 {
                open = None;
                record += count;
                continue;
            }
            self.quads += count;
            if let Some(entry) = open {
                self.entries[entry][0] = self.quads;
            } else {
                open = Some(self.entries.len());
                self.entries.push([self.quads, record, offset, 0]);
            }
            record += count;
        }
    }

    /// Records a list of `entries` entries needs, header included.
    pub fn records_for(entries: usize) -> u32 {
        1 + (entries as u32).div_ceil(ENTRIES_PER_RECORD)
    }

    /// The bytes to write at the list's range.
    pub fn bytes(&self) -> Vec<u8> {
        let words = Self::records_for(self.entries.len()) as usize * QUAD_WORDS;
        let mut bytes = Vec::with_capacity(words * 4);
        bytes.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.quads.to_le_bytes());
        bytes.resize(QUAD_WORDS * 4, 0);
        bytes.extend(
            self.entries
                .iter()
                .flatten()
                .flat_map(|word| word.to_le_bytes()),
        );
        bytes.resize(words * 4, 0);
        bytes
    }

    /// The record each drawn quad reads and its section offset, as the
    /// shader finds them. For tests.
    pub fn resolve(&self, quad: u32) -> Option<(u32, u32)> {
        if quad >= self.quads {
            return None;
        }
        let entry = self.entries.partition_point(|entry| entry[0] <= quad);
        let begin = entry
            .checked_sub(1)
            .map_or(0, |previous| self.entries[previous][0]);
        let [_, record, offset, _] = self.entries[entry];
        Some((record + quad - begin, offset))
    }
}

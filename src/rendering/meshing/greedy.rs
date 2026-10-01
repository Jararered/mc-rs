//! Binary-plane rectangle merge for full cube faces.
//!
//! `mesh_binary_plane` is adapted from `greedy_mesh_binary_plane` in
//! `refs/binary_greedy_mesher_demo` (MIT OR Apache-2.0). A row is a bitset of
//! visible faces that already share a tile, tint, and flat corner shading.
//! The scan grows those bits into rectangles. Face culling stays with
//! `neighbor_hides_face`, because fancy leaves and partial blocks are not a
//! solid-versus-air mask.

use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::SECTION_HEIGHT;

const _: () = assert!(CHUNK_SIZE <= 16 && SECTION_HEIGHT <= 16);

pub(crate) const PLANE: usize = CHUNK_SIZE;

/// A rectangle in one face plane. `row`/`bit` are the plane axes in blocks,
/// `w`/`h` how far the rectangle extends along each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GreedyQuad {
    pub row: u32,
    pub bit: u32,
    pub w: u32,
    pub h: u32,
}

pub(crate) fn mesh_binary_plane(mut rows: [u16; PLANE]) -> Vec<GreedyQuad> {
    let mut quads = Vec::new();
    let width = u16::try_from(PLANE).expect("plane width fits in a row");
    for row in 0..PLANE {
        let mut bit = 0u16;
        while bit < width {
            let skip = (rows[row] >> bit).trailing_zeros();
            let Ok(skip) = u16::try_from(skip) else {
                break;
            };
            bit = bit.saturating_add(skip);
            if bit >= width {
                break;
            }
            let height = (rows[row] >> bit).trailing_ones();
            // `1 << 16` does not fit in a row. A full row is every bit.
            let height_mask = u16::checked_shl(1, height).map_or(u16::MAX, |value| value - 1);
            let mask = height_mask << bit;
            let mut span = 1u16;
            while u16::try_from(row).expect("row") + span < width {
                let next = rows[row + usize::from(span)] >> bit;
                if next & height_mask != height_mask {
                    break;
                }
                rows[row + usize::from(span)] &= !mask;
                span += 1;
            }
            quads.push(GreedyQuad {
                row: u32::try_from(row).expect("row"),
                bit: u32::from(bit),
                w: u32::from(span),
                h: height,
            });
            let Ok(height) = u16::try_from(height) else {
                break;
            };
            bit = bit.saturating_add(height);
        }
    }
    quads
}

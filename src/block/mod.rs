//! Compact block values, Beta ids `0..=96`, and the per-id property table.
//!
//! The table is `data/blocks.ron` ([`definition`]): burn rate, explosion
//! resistance, solidity, which tool digs and harvests a block, and its
//! [`StepSound`](definition::StepSound). Facing and species are metadata,
//! not part of the id. Change a block's static properties in the RON file.

pub mod bed;
pub mod blocks;
pub mod definition;
pub mod direction;
pub mod fluids;
pub mod properties;

pub use blocks::Block;

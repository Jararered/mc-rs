//! Client rendering of world data, entity visuals, and shared item assets.

pub mod appearance;
pub mod chunk_quads;
pub mod clouds;
pub mod creatures;
pub mod dropped_items;
pub(crate) mod falling_block;
pub mod icons;
mod item_tiles;
pub mod meshing;
mod mobs;
pub mod particles;
mod plugin;
pub mod shadow;
pub mod sky;
pub mod textures;
pub mod weather;

pub use plugin::WorldRenderingPlugin;

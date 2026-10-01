//! Client rendering of world data, entity visuals, and shared item assets.

pub mod appearance;
pub mod clouds;
pub mod icons;
mod item_tiles;
pub mod meshing;
mod plugin;
pub mod sky;
pub mod textures;

pub use plugin::WorldRenderingPlugin;

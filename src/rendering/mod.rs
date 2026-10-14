//! Client rendering of world data, entity visuals, and shared item assets.

pub mod appearance;
pub mod chunk_quads;
pub mod chunk_regions;
pub mod clouds;
pub mod creatures;
pub mod dropped_items;
pub(crate) mod falling_block;
pub mod icons;
mod item_tiles;
pub mod meshing;
mod mobs;
pub mod occlusion;
pub mod particles;
mod plugin;
pub mod shadow;
pub mod sky;
pub mod textures;
pub mod weather;

pub use plugin::WorldRenderingPlugin;

/// A `load_internal_asset!` loader for the game's WESL shaders. Bevy names a
/// WESL module after its embedded path, so `src/rendering/weather.wesl` is
/// `game::rendering::weather` to an `import`.
pub(crate) fn wesl_shader(source: &'static str, path: impl AsRef<str>) -> bevy::shader::Shader {
    let path = path.as_ref().replace('\\', "/");
    let module = path
        .rsplit_once("src/")
        .map_or(path.as_str(), |(_, module)| module);
    bevy::shader::Shader::from_wesl(source, format!("embedded://game/{module}"))
}

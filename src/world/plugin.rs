//! Simulation plugin composition, independent of cameras and render assets.

use bevy::prelude::*;

use super::block_ticks::BlockTicksPlugin;
use super::chunk::WorldChunks;
use super::tick::WorldTick;

/// World simulation resources, block updates, and block entities.
/// Add `WorldRenderingPlugin` in a client to render and stream terrain.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldChunks>()
            .init_resource::<WorldTick>()
            .add_plugins(BlockTicksPlugin)
            .add_systems(First, super::tick::advance_world_tick)
            .add_systems(Update, super::furnace::tick_furnaces);
    }
}

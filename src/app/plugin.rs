use bevy::prelude::*;

use crate::{player::PlayerPlugin, world::plugin::WorldPlugin};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldPlugin, PlayerPlugin));
    }
}

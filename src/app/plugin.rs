use bevy::prelude::*;

use crate::{player::PlayerPlugin, ui::MenuPlugin, world::plugin::WorldPlugin};

use super::state::AppScreen;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppScreen>()
            .add_plugins((WorldPlugin, PlayerPlugin, MenuPlugin));
    }
}

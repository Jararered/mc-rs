use bevy::prelude::*;

use crate::entity::particles::BlockParticlePlugin;
use crate::physics::PhysicsPlugin;
use crate::player::PlayerPlugin;
use crate::ui::HudPlugin;
use crate::ui::MenuPlugin;
use crate::ui::UiCameraPlugin;
use crate::world::persistence::PersistencePlugin;
use crate::world::plugin::WorldPlugin;

use super::perf::PerfPlugin;
use super::screenshot::ScreenshotPlugin;
use super::settings::SettingsPlugin;
use super::state::AppScreen;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppScreen>().add_plugins((
            SettingsPlugin::default(),
            WorldPlugin,
            PersistencePlugin::default(),
            PlayerPlugin,
            BlockParticlePlugin,
            PhysicsPlugin,
            UiCameraPlugin,
            MenuPlugin,
            HudPlugin,
            ScreenshotPlugin,
            PerfPlugin,
        ));
    }
}

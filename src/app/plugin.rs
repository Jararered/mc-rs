use bevy::prelude::*;

use crate::entity::drops::items::DroppedItemPlugin;
use crate::entity::particles::block::BlockParticlePlugin;
use crate::entity::particles::registry::ParticleRegistryPlugin;
use crate::entity::shadow::plugin as entity_shadow_plugin;
use crate::physics::PhysicsPlugin;
use crate::player::PlayerPlugin;
use crate::random::parse_seed;
use crate::ui::ChatPlugin;
use crate::ui::HudPlugin;
use crate::ui::InventoryGuiPlugin;
use crate::ui::MenuPlugin;
use crate::ui::UiCameraPlugin;
use crate::world::persistence::PersistencePlugin;
use crate::world::plugin::WorldPlugin;

use super::diagnostics::DiagnosticsPlugin;
use super::frame_pacing::FramePacingPlugin;
use super::fullscreen::FullscreenPlugin;
use super::screenshot::ScreenshotPlugin;
use super::settings::SettingsPlugin;
use super::state::AppScreen;

/// Seed for a newly created world. `parse_seed` accepts a signed decimal long
/// or a text seed such as `glacier`.
const NEW_WORLD_SEED: &str = "gargamel";

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppScreen>()
            .add_plugins(ChatPlugin)
            .add_plugins((
                SettingsPlugin::default(),
                WorldPlugin,
                // Only consulted when no world exists yet; resuming a save keeps
                // the seed recorded in its own `level.json`.
                PersistencePlugin::default().with_seed(parse_seed(NEW_WORLD_SEED)),
                PlayerPlugin,
                BlockParticlePlugin,
                DroppedItemPlugin,
                entity_shadow_plugin,
                ParticleRegistryPlugin,
                PhysicsPlugin,
                UiCameraPlugin,
                MenuPlugin,
                HudPlugin,
                InventoryGuiPlugin,
                (ScreenshotPlugin, FullscreenPlugin),
                (DiagnosticsPlugin, FramePacingPlugin),
            ));
    }
}

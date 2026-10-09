use bevy::prelude::*;

use crate::chat::ChatPlugin;
use crate::entity::drops::items::DroppedItemPlugin;
use crate::physics::PhysicsPlugin;
use crate::player::PlayerPlugin;
use crate::rendering::dropped_items::DroppedItemRenderPlugin;
use crate::rendering::particles::block::BlockParticlePlugin;
use crate::rendering::particles::rain::RainParticlePlugin;
use crate::rendering::particles::registry::ParticleRegistryPlugin;
use crate::rendering::shadow::plugin as entity_shadow_plugin;
use crate::ui::ChatUiPlugin;
use crate::ui::HudPlugin;
use crate::ui::InventoryGuiPlugin;
use crate::ui::MenuPlugin;
use crate::ui::PauseMenuPlugin;
use crate::ui::PortalUiPlugin;
use crate::ui::SleepUiPlugin;
use crate::ui::UiCameraPlugin;
use crate::world::persistence::PersistencePlugin;
use crate::world::plugin::WorldPlugin;

use super::diagnostics::DiagnosticsPlugin;
use super::frame_pacing::FramePacingPlugin;
use super::fullscreen::FullscreenPlugin;
use super::screenshot::ScreenshotPlugin;
use super::session::SessionPlugin;
use super::settings::SettingsPlugin;
use super::state::AppScreen;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppScreen>()
            .add_plugins((ChatPlugin, ChatUiPlugin))
            .add_plugins((
                SettingsPlugin::default(),
                (
                    WorldPlugin,
                    crate::rendering::WorldRenderingPlugin,
                    crate::rendering::icons::ItemIconsPlugin,
                ),
                // Only consulted when no world exists yet; resuming a save keeps
                // the seed recorded in its own `level.json`.
                PersistencePlugin::default().deferred(),
                PlayerPlugin,
                (BlockParticlePlugin, RainParticlePlugin),
                (DroppedItemPlugin, DroppedItemRenderPlugin),
                entity_shadow_plugin,
                ParticleRegistryPlugin,
                PhysicsPlugin,
                UiCameraPlugin,
                (MenuPlugin, PauseMenuPlugin, SessionPlugin),
                (HudPlugin, PortalUiPlugin, SleepUiPlugin),
                InventoryGuiPlugin,
                (ScreenshotPlugin, FullscreenPlugin),
                (DiagnosticsPlugin, FramePacingPlugin),
            ));
    }
}

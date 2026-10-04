//! Menus, the HUD, inventory/chat screens, and stack overlays.
//!
//! [`UiCameraPlugin`] composites everything here after the world and the
//! first-person arm. Screens live in [`screens`], stack overlays in [`icons`].
//! Shared item appearances and icon assets belong to `crate::rendering`.

use bevy::prelude::*;

pub mod icons;
pub mod screens;
pub mod slider;

pub use screens::chat::ChatUiPlugin;
pub use screens::hud::HudPlugin;
pub use screens::inventory::InventoryGuiPlugin;
pub use screens::menu::MenuPlugin;

/// Composites all HUD and menu nodes after the world and first-person arm.
pub struct UiCameraPlugin;

impl Plugin for UiCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ui_camera);
    }
}

fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("UI camera"),
        Camera2d,
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        bevy::ui::IsDefaultUiCamera,
    ));
}

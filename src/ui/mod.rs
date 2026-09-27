//! Menus, the HUD, the inventory GUI, and item icons.
//!
//! [`UiCameraPlugin`] composites everything here after the world and the
//! first-person arm. Screens live in [`screens`], icon data in [`icons`].

use bevy::prelude::*;

pub mod icons;
pub mod screens;

pub use screens::chat::ChatPlugin;
pub use screens::hud::HudPlugin;
pub use screens::inventory::InventoryGuiPlugin;
pub use screens::menu::MenuPlugin;

pub(crate) use screens::chat::ChatState;
pub(crate) use screens::inventory::InventoryScreen;
pub(crate) use screens::inventory::WorkbenchUiSession;
pub(crate) use screens::inventory::close_crafting_interface;

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

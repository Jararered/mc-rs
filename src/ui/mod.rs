//! Menus, the HUD, inventory/chat screens, and stack overlays.
//!
//! [`UiCameraPlugin`] composites everything here after the world and the
//! first-person arm. Screens live in [`screens`], stack overlays in [`icons`].
//! Shared item appearances and icon assets belong to `crate::rendering`.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

pub mod icons;
pub mod screens;
pub mod slider;

pub use screens::chat::ChatUiPlugin;
pub use screens::hud::HudPlugin;
pub use screens::inventory::InventoryGuiPlugin;
pub use screens::menu::MenuPlugin;
pub use screens::pause::PauseMenuPlugin;
pub use screens::portal::PortalUiPlugin;
pub use screens::sleep::SleepUiPlugin;

/// Bevy 0.20 deprecates `Interaction` in favor of `picking::hover::Hovered`
/// and `ui::Pressed`, and still drives it. The screens name it through this
/// alias until they move to those.
#[expect(deprecated, reason = "the screens still use Bevy's focus system")]
pub type Interaction = bevy::ui::Interaction;

/// A clickable node. Bevy 0.20 no longer exports its own marker, which did
/// only this.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[require(Node, FocusPolicy::Block, Interaction)]
pub struct Button;

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

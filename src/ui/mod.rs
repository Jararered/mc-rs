//! Menus, the HUD, inventory/chat screens, and stack overlays.
//!
//! [`UiCameraPlugin`] composites everything here after the world and the
//! first-person arm. Screens live in [`screens`], stack overlays in [`icons`].
//! Shared item appearances and icon assets belong to `crate::rendering`.

use bevy::picking::events::PointerPress;
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui::Pressed;

use crate::world::sound::Sounds;

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

/// A clickable node. `Hovered` is kept by Bevy's picking, and [`ButtonPlugin`]
/// adds `Pressed` while the left button that went down on it is held, so a
/// screen acts on `Added<Pressed>`: on the press, as Beta's buttons do.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
#[require(Node, Hovered)]
pub struct Button;

/// Keeps `Pressed` on every [`Button`].
pub struct ButtonPlugin;

impl Plugin for ButtonPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(press_button)
            .add_systems(PostUpdate, release_buttons);
    }
}

/// A press on a button's text reaches the button by bubbling. It clicks, as
/// `GuiScreen.mouseClicked` does for every button.
fn press_button(
    press: On<PointerPress>,
    buttons: Query<(), With<Button>>,
    mut commands: Commands,
    sounds: Option<ResMut<Sounds>>,
) {
    if press.button == PointerButton::Primary && buttons.contains(press.entity) {
        commands.entity(press.entity).insert(Pressed);
        if let Some(mut sounds) = sounds {
            sounds.play_ui("random.click", 1.0, 1.0);
        }
    }
}

/// The release may land anywhere, or on no node at all, so this follows the
/// mouse button and not a pointer event. It runs after `Update` so a press
/// and release within one frame is still seen there.
fn release_buttons(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    pressed: Query<Entity, (With<Button>, With<Pressed>)>,
    mut commands: Commands,
) {
    if mouse.is_some_and(|mouse| mouse.pressed(MouseButton::Left)) {
        return;
    }
    for button in &pressed {
        commands.entity(button).remove::<Pressed>();
    }
}

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

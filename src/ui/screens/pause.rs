//! The pause menu opened with Escape while playing.
//!
//! It is a flag on [`PauseMenu`] inside [`AppScreen::Playing`], like the
//! inventory, over the same dimmed backdrop. Opening it asks persistence for a
//! save, so a pause doubles as a checkpoint.

use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use super::menu::MenuTextures;
use super::menu::button_rect;
use super::menu::menu_font;
use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::app::state::SettingsReturn;
use crate::chat::ChatFocus;
use crate::inventory::session::InventorySession;
use crate::world::persistence::WorldPersistence;

const BUTTON_WIDTH: f32 = 400.0;

pub struct PauseMenuPlugin;

impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseMenu>()
            .init_resource::<SettingsReturn>()
            .init_resource::<InventorySession>()
            .add_systems(OnExit(AppScreen::Playing), despawn_pause)
            .add_systems(
                Update,
                (toggle, sync_root, handle_buttons)
                    .chain()
                    // Before the inventory chain, so an Escape that closes the
                    // inventory is not also read as opening this menu.
                    .before(crate::rendering::icons::build)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy)]
enum PauseAction {
    Resume,
    Settings,
    QuitToTitle,
}

fn set_cursor(windows: &mut Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>, grab: bool) {
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    if grab && window.focused {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    } else {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Option<Res<ChatFocus>>,
    inventory: Res<InventorySession>,
    mut pause: ResMut<PauseMenu>,
    persistence: Option<ResMut<WorldPersistence>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if pause.open {
        pause.open = false;
        set_cursor(&mut windows, true);
        return;
    }
    if inventory.open || chat.is_some_and(|chat| chat.suppress_controls) {
        return;
    }
    pause.open = true;
    set_cursor(&mut windows, false);
    if let Some(mut persistence) = persistence {
        persistence.request_save();
    }
}

fn sync_root(
    mut commands: Commands,
    pause: Res<PauseMenu>,
    textures: Option<Res<MenuTextures>>,
    roots: Query<Entity, With<PauseRoot>>,
) {
    if !pause.open {
        for root in &roots {
            commands.entity(root).despawn();
        }
        return;
    }
    if !roots.is_empty() {
        return;
    }
    let Some(textures) = textures else {
        return;
    };
    commands
        .spawn((
            PauseRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(14),
                ..default()
            },
            // The inventory's backdrop.
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
        ))
        .with_children(|root| {
            for (title, action) in [
                ("Resume", PauseAction::Resume),
                ("Settings", PauseAction::Settings),
                ("Quit to title", PauseAction::QuitToTitle),
            ] {
                root.spawn((
                    Button,
                    action,
                    Node {
                        width: px(BUTTON_WIDTH),
                        height: px(40),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    ImageNode::new(textures.buttons.clone())
                        .with_rect(button_rect(false))
                        .with_mode(NodeImageMode::Sliced(TextureSlicer {
                            border: BorderRect::all(2.0),
                            max_corner_scale: 2.0,
                            ..default()
                        })),
                    BackgroundColor(Color::srgb(0.44, 0.44, 0.44)),
                ))
                .with_child((
                    Text::new(title),
                    menu_font(&textures, 16.0),
                    TextColor(Color::WHITE),
                    TextShadow::default(),
                ));
            }
        });
}

fn handle_buttons(
    mut buttons: Query<
        (
            &Interaction,
            &PauseAction,
            &mut ImageNode,
            &mut BackgroundColor,
        ),
        Changed<Interaction>,
    >,
    mut pause: ResMut<PauseMenu>,
    mut settings_return: ResMut<SettingsReturn>,
    mut next_screen: ResMut<NextState<AppScreen>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    for (interaction, action, mut image, mut background) in &mut buttons {
        image.rect = Some(button_rect(*interaction != Interaction::None));
        background.0 = if *interaction == Interaction::None {
            Color::srgb(0.44, 0.44, 0.44)
        } else {
            Color::srgb(0.57, 0.57, 0.72)
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PauseAction::Resume => {
                pause.open = false;
                set_cursor(&mut windows, true);
            }
            PauseAction::Settings => {
                settings_return.0 = AppScreen::Playing;
                next_screen.set(AppScreen::Settings);
            }
            PauseAction::QuitToTitle => {
                pause.open = false;
                settings_return.0 = AppScreen::Menu;
                next_screen.set(AppScreen::Menu);
            }
        }
        // One click acts once, even if several buttons somehow changed.
        break;
    }
}

/// The root cannot outlive `Playing`; `sync_root` rebuilds it on return if the
/// menu is still open, such as after visiting the settings screen.
fn despawn_pause(mut commands: Commands, roots: Query<Entity, With<PauseRoot>>) {
    for root in &roots {
        commands.entity(root).despawn();
    }
}

//! F11 toggles the game window between windowed and borderless fullscreen.
//!
//! The key works in every [`AppScreen`], like in the original game, so it can be
//! used from the menu as well as while playing.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use bevy::window::MonitorSelection;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowResized;
use bevy::window::WindowResolution;

/// Windowed size to restore when leaving fullscreen.
///
/// Going fullscreen resizes the window to the monitor, and the window backend
/// reports that size back as a resize. Without a saved resolution the window
/// would come back at full monitor size instead of its former windowed one.
#[derive(Resource, Default)]
pub struct WindowedResolution(Option<WindowResolution>);

pub struct FullscreenPlugin;

impl Plugin for FullscreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameSettings>()
            .init_resource::<WindowedResolution>()
            .add_systems(
                PostUpdate,
                toggle_fullscreen.before(bevy::render::camera::camera_system),
            );
    }
}

fn toggle_fullscreen(
    keys: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<GameSettings>,
    mut windowed: ResMut<WindowedResolution>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut resized: MessageWriter<WindowResized>,
) {
    if keys.just_pressed(KeyCode::F11) {
        settings.fullscreen = !settings.fullscreen;
    }
    let Ok((entity, mut window)) = windows.single_mut() else {
        return;
    };

    if settings.fullscreen == (window.mode != WindowMode::Windowed) {
        return;
    }

    if settings.fullscreen {
        windowed.0 = Some(window.resolution.clone());
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
    } else {
        window.mode = WindowMode::Windowed;
        if let Some(resolution) = windowed.0.take() {
            window.resolution = resolution;
            // Camera target sizes are refreshed by resize messages, not Window
            // change detection. Notify before PostUpdate's camera system so the
            // depth textures and the extracted window color target agree this
            // frame; the backend's resize notification can arrive later.
            resized.write(WindowResized {
                window: entity,
                width: window.width(),
                height: window.height(),
            });
        }
    }
}

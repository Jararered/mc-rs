//! Bound rendering work independently of the shared 20 Hz simulation clock.

use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::UpdateMode;
use bevy::winit::WinitSettings;

use super::settings::GameSettings;
use super::state::AppScreen;

pub struct FramePacingPlugin;

impl Plugin for FramePacingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WinitSettings>()
            .add_systems(Update, apply_frame_pacing);
    }
}

fn apply_frame_pacing(
    settings: Res<GameSettings>,
    screen: Option<Res<State<AppScreen>>>,
    mut pacing: ResMut<WinitSettings>,
    mut previous: Local<Option<(u32, bool)>>,
) {
    let playing = screen.is_none_or(|screen| *screen.get() == AppScreen::Playing);
    let desired = (settings.max_fps, playing);
    if *previous == Some(desired) {
        return;
    }
    *previous = Some(desired);
    pacing.focused_mode = if playing {
        if settings.max_fps == 0 {
            UpdateMode::Continuous
        } else {
            // Input is collected by winit and consumed on the next frame.
            // Reacting to each mouse event would defeat the frame cap.
            UpdateMode::Reactive {
                wait: Duration::from_secs_f64(1.0 / f64::from(settings.max_fps)),
                react_to_device_events: false,
                react_to_user_events: false,
                react_to_window_events: false,
            }
        }
    } else {
        UpdateMode::reactive_low_power(Duration::from_secs_f64(1.0 / 30.0))
    };
    // Keep gameplay ticking at 20 Hz in the background. Paused menus only
    // need occasional redraws and can still react immediately to input.
    pacing.unfocused_mode = UpdateMode::reactive_low_power(Duration::from_secs_f64(if playing {
        1.0 / 20.0
    } else {
        1.0 / 5.0
    }));
}

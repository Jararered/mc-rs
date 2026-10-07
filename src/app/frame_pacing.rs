//! Bound rendering work independently of the shared 20 Hz simulation clock.

use std::time::Duration;

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::winit::UpdateMode;
use bevy::winit::WINIT_WINDOWS;
use bevy::winit::WinitSettings;

use super::settings::GameSettings;
use super::state::AppScreen;

/// Share of the display's refresh rate a windowed game paces itself to. Just
/// under the display, so the swapchain never fills and blocks the frame.
const DISPLAY_PACING: f64 = 0.995;

pub struct FramePacingPlugin;

impl Plugin for FramePacingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WinitSettings>()
            .add_systems(Update, apply_frame_pacing);
    }
}

/// Time between gameplay frames, or `None` to let VSync alone pace them.
///
/// `display_millihertz` is the refresh rate to stay under, when the display
/// cannot be trusted to pace the game itself.
pub fn frame_interval(max_fps: u32, display_millihertz: Option<u32>) -> Option<Duration> {
    let selected = (max_fps != 0).then(|| f64::from(max_fps));
    let display = display_millihertz
        .filter(|millihertz| *millihertz != 0)
        .map(|millihertz| f64::from(millihertz) / 1000.0 * DISPLAY_PACING);
    let rate = match (selected, display) {
        (Some(selected), Some(display)) => selected.min(display),
        (selected, display) => selected.or(display)?,
    };
    Some(Duration::from_secs_f64(1.0 / rate))
}

/// The refresh rate of the display a windowed game is on.
///
/// A composited Linux window does not get an even frame from VSync: the
/// swapchain hands out two images back to back and then blocks for two
/// refreshes, so frames that are shown one refresh apart were simulated 1 ms
/// and 13 ms apart, which shows as judder in mouse look and movement. A
/// fullscreen window is paced evenly and is left to VSync.
fn windowed_refresh_millihertz(
    windows: &Query<(Entity, &Window), With<PrimaryWindow>>,
) -> Option<u32> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let (entity, window) = windows.single().ok()?;
    if window.mode != WindowMode::Windowed {
        return None;
    }
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        winit_windows
            .get_window(entity)?
            .current_monitor()?
            .refresh_rate_millihertz()
    })
}

fn apply_frame_pacing(
    settings: Res<GameSettings>,
    screen: Option<Res<State<AppScreen>>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut pacing: ResMut<WinitSettings>,
    mut previous: Local<Option<(u32, bool, Option<u32>)>>,
    // The winit windows live in a thread local of the main thread.
    _main_thread: NonSendMarker,
) {
    let playing = screen.is_none_or(|screen| *screen.get() == AppScreen::Playing);
    let display = playing
        .then(|| windowed_refresh_millihertz(&windows))
        .flatten();
    let desired = (settings.max_fps, playing, display);
    if *previous == Some(desired) {
        return;
    }
    *previous = Some(desired);
    pacing.focused_mode = if playing {
        match frame_interval(settings.max_fps, display) {
            None => UpdateMode::Continuous,
            // Input is collected by winit and consumed on the next frame.
            // Reacting to each mouse event would defeat the frame cap.
            Some(wait) => UpdateMode::Reactive {
                wait,
                react_to_device_events: false,
                react_to_user_events: false,
                react_to_window_events: false,
            },
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

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::window::MonitorSelection;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowPlugin;
use game::app::fullscreen::FullscreenPlugin;

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        WindowPlugin::default(),
    ))
    .init_resource::<ButtonInput<KeyCode>>()
    .add_plugins(FullscreenPlugin);
    app
}

fn primary_window(app: &mut App) -> Mut<'_, Window> {
    let mut windows = app
        .world_mut()
        .query_filtered::<&mut Window, With<PrimaryWindow>>();
    windows
        .iter_mut(app.world_mut())
        .next()
        .expect("primary window")
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    // `clear` leaves the key held, so a second press would not be a `just_press`.
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(key);
}

#[test]
fn f11_toggles_windowed_and_borderless_fullscreen() {
    let mut app = test_app();
    assert_eq!(primary_window(&mut app).mode, WindowMode::Windowed);

    press(&mut app, KeyCode::F11);
    assert_eq!(
        primary_window(&mut app).mode,
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    );

    press(&mut app, KeyCode::F11);
    assert_eq!(primary_window(&mut app).mode, WindowMode::Windowed);
}

#[test]
fn leaving_fullscreen_restores_the_windowed_resolution() {
    let mut app = test_app();
    let windowed = primary_window(&mut app).resolution.clone();

    press(&mut app, KeyCode::F11);
    // A fullscreen window matches the monitor, which the window backend reports
    // back as a resize.
    primary_window(&mut app)
        .resolution
        .set_physical_resolution(3840, 2160);

    press(&mut app, KeyCode::F11);
    let window = primary_window(&mut app);
    assert_eq!(window.mode, WindowMode::Windowed);
    assert_eq!(window.resolution, windowed);
}

#[test]
fn other_keys_leave_the_window_mode_alone() {
    let mut app = test_app();
    press(&mut app, KeyCode::F10);
    assert_eq!(primary_window(&mut app).mode, WindowMode::Windowed);
}

#![recursion_limit = "256"]

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::render::camera::camera_system;
use bevy::render::texture::ManualTextureViews;
use bevy::window::MonitorSelection;
use bevy::window::PrimaryWindow;
use bevy::window::WindowMode;
use bevy::window::WindowPlugin;
use bevy::window::WindowResized;
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

#[test]
fn restoring_windowed_size_refreshes_all_camera_targets_in_the_same_frame() {
    let mut app = test_app();
    app.init_asset::<Image>()
        .init_resource::<ManualTextureViews>()
        .add_systems(PostUpdate, camera_system);
    // The sky, world, arm, and UI all render to the primary window.
    let cameras: Vec<_> = (0..5)
        .map(|_| {
            app.world_mut()
                .spawn((Camera::default(), Projection::default()))
                .id()
        })
        .collect();
    app.update();
    let windowed_size = primary_window(&mut app).resolution.physical_size();
    press(&mut app, KeyCode::F11);
    let entity = {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>();
        query.single(app.world()).unwrap()
    };
    primary_window(&mut app)
        .resolution
        .set_physical_resolution(2560, 1440);
    app.world_mut().write_message(WindowResized {
        window: entity,
        width: 2560.0,
        height: 1440.0,
    });
    app.update();
    for &camera in &cameras {
        assert_eq!(
            app.world()
                .get::<Camera>(camera)
                .unwrap()
                .physical_target_size(),
            Some(UVec2::new(2560, 1440))
        );
    }

    // No backend resize event arrives until after this frame's camera update.
    press(&mut app, KeyCode::F11);
    for camera in cameras {
        assert_eq!(
            app.world()
                .get::<Camera>(camera)
                .unwrap()
                .physical_target_size(),
            Some(windowed_size)
        );
    }
}

#[test]
fn saved_fullscreen_and_menu_changes_share_the_f11_setting() {
    use game::app::settings::GameSettings;
    let mut app = test_app();
    let original = primary_window(&mut app).resolution.clone();
    app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
    app.update();
    assert_ne!(primary_window(&mut app).mode, WindowMode::Windowed);
    press(&mut app, KeyCode::F11);
    assert!(!app.world().resource::<GameSettings>().fullscreen);
    assert_eq!(primary_window(&mut app).resolution, original);
    app.world_mut().resource_mut::<GameSettings>().fullscreen = true;
    app.update();
    app.world_mut().resource_mut::<GameSettings>().fullscreen = false;
    app.update();
    assert_eq!(primary_window(&mut app).mode, WindowMode::Windowed);
}

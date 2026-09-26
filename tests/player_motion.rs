use bevy::asset::AssetPlugin;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;
use game::app::settings::GameSettings;
use game::app::state::AppScreen;
use game::entity::CollisionState;
use game::entity::Flying;
use game::entity::Velocity;
use game::player::FlySpeed;
use game::player::Player;
use game::player::PlayerCamera;
use game::player::PlayerPlugin;
use game::ui::HudPlugin;
use game::ui::InventoryGuiPlugin;
use game::world::chunk::WorldChunks;
use game::world::tick::WorldTick;
use std::time::Duration;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
        bevy::input::InputPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .init_asset::<Image>()
    .init_asset::<Font>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldChunks>()
    .init_resource::<WorldTick>()
    .add_plugins((PlayerPlugin, InventoryGuiPlugin, HudPlugin));
    app.world_mut().spawn((
        Window {
            focused: true,
            ..default()
        },
        CursorOptions {
            grab_mode: CursorGrabMode::Locked,
            visible: false,
            ..default()
        },
        PrimaryWindow,
    ));
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    let player = app
        .world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap();
    app.world_mut()
        .entity_mut(player)
        .insert((Flying, FlySpeed(2.0)));
    app
}

fn player_entity(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<Player>>()
        .single(app.world())
        .unwrap()
}

fn velocity(app: &mut App) -> Vec3 {
    app.world_mut()
        .query_filtered::<&Velocity, With<Player>>()
        .single(app.world())
        .unwrap()
        .0
}

fn fly_speed(app: &mut App) -> f32 {
    app.world_mut()
        .query_filtered::<&FlySpeed, With<Player>>()
        .single(app.world())
        .unwrap()
        .0
}

#[test]
fn diagonal_flight_normalizes_combined_axes_before_applying_speed() {
    let mut app = app();
    let player = player_entity(&mut app);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Transform>()
        .unwrap()
        .rotation = Quat::IDENTITY;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyD);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);

    app.update();

    let expected = Vec3::new(1.0, 1.0, -1.0).normalize() * 60.0;
    assert!(velocity(&mut app).abs_diff_eq(expected, 1.0e-5));
    assert!((velocity(&mut app).length() - 60.0).abs() < 1.0e-5);
}

#[test]
fn opposing_flight_inputs_cancel_to_zero_without_nan() {
    let mut app = app();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.press(KeyCode::KeyW);
    keys.press(KeyCode::KeyS);

    app.update();

    let motion = velocity(&mut app);
    assert_eq!(motion, Vec3::ZERO);
    assert!(motion.is_finite());
}

#[test]
fn saved_fly_speed_is_clamped_to_the_supported_multipliers() {
    let mut slow = FlySpeed(-10.0);
    slow.clamp_value();
    assert_eq!(slow.0, 1.0);

    let mut fast = FlySpeed(100.0);
    fast.clamp_value();
    assert_eq!(fast.0, 50.0);
}

#[test]
fn fly_speed_keys_apply_their_multiplicative_steps() {
    let mut faster = app();
    faster
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Equal);
    faster.world_mut().run_schedule(Update);
    assert!((fly_speed(&mut faster) - 2.5).abs() < f32::EPSILON);

    let mut slower = app();
    slower
        .world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Minus);
    slower.world_mut().run_schedule(Update);
    assert!(
        (fly_speed(&mut slower) - 1.6).abs() < f32::EPSILON,
        "expected a 1/1.25 multiplier, got {}",
        fly_speed(&mut slower)
    );
}

#[test]
fn camera_bob_tracks_grounded_horizontal_motion() {
    let mut app = app();
    let player = player_entity(&mut app);
    app.world_mut().entity_mut(player).remove::<Flying>();
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Transform>()
        .unwrap()
        .rotation = Quat::IDENTITY;
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Velocity>()
        .unwrap()
        .0 = Vec3::new(4.0, 0.0, 0.0);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<CollisionState>()
        .unwrap()
        .on_ground = true;

    app.update();

    let camera = app
        .world_mut()
        .query_filtered::<&Transform, With<PlayerCamera>>()
        .single(app.world())
        .unwrap();
    assert!(
        (camera.translation.x - -0.00736).abs() < 1.0e-4,
        "camera x offset was {}",
        camera.translation.x
    );
    assert!(
        (camera.translation.y - -0.03718).abs() < 1.0e-4,
        "camera y offset was {}",
        camera.translation.y
    );
}

#[test]
fn airborne_camera_pitch_tracks_vertical_speed() {
    let mut app = app();
    let player = player_entity(&mut app);
    app.world_mut().entity_mut(player).remove::<Flying>();
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Velocity>()
        .unwrap()
        .0 = Vec3::new(0.0, -10.0, 0.0);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<CollisionState>()
        .unwrap()
        .on_ground = false;

    app.update();

    let camera = app
        .world_mut()
        .query_filtered::<&Transform, With<PlayerCamera>>()
        .single(app.world())
        .unwrap();
    let target_pitch = (0.1_f32.atan() * 15.0) * 0.8;
    let expected_rotation = Quat::from_rotation_x(target_pitch.to_radians());
    assert!(camera.rotation.abs_diff_eq(expected_rotation, 1.0e-5));
}

#[test]
fn mouse_look_applies_sensitivity_and_clamps_pitch() {
    let mut app = app();
    let player = player_entity(&mut app);
    app.world_mut()
        .entity_mut(player)
        .get_mut::<Transform>()
        .unwrap()
        .rotation = Quat::IDENTITY;
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(100.0, -100.0);
    app.world_mut().run_schedule(Update);

    let transform = app.world().entity(player).get::<Transform>().unwrap();
    let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    assert!((yaw - -0.2).abs() < 1.0e-5);
    assert!((pitch - 0.2).abs() < 1.0e-5);

    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(0.0, -2_000.0);
    app.world_mut().run_schedule(Update);
    let transform = app.world().entity(player).get::<Transform>().unwrap();
    let (_, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    assert!((pitch - (std::f32::consts::FRAC_PI_2 - 0.01)).abs() < 1.0e-5);
}

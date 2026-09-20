use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_player, capture_mouse))
            .add_systems(Update, (update_mouse_capture, move_player).chain());
    }
}

#[derive(Component)]
pub struct Player;

const WALK_SPEED: f32 = 5.0;
const SPRINT_SPEED: f32 = 15.0;
const MOUSE_SENSITIVITY: f32 = 0.002;

fn spawn_player(mut commands: Commands) {
    commands.spawn((
        Name::new("Player"),
        Player,
        Camera3d::default(),
        Transform::from_xyz(8.0, 14.0, 27.0).looking_at(Vec3::new(8.0, 5.0, 8.0), Vec3::Y),
    ));
}

fn capture_mouse(mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    if let Ok((window, mut cursor)) = windows.single_mut()
        && window.focused
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn update_mouse_capture(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };

    if !window.focused || keys.just_pressed(KeyCode::Escape) {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if mouse_buttons.just_pressed(MouseButton::Left) {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    mut player: Query<&mut Transform, With<Player>>,
) {
    let Ok((window, cursor)) = windows.single() else {
        return;
    };
    if !window.focused || cursor.grab_mode != CursorGrabMode::Locked {
        return;
    }

    let Ok(mut transform) = player.single_mut() else {
        return;
    };

    let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
    yaw -= mouse_motion.delta.x * MOUSE_SENSITIVITY;
    pitch = (pitch - mouse_motion.delta.y * MOUSE_SENSITIVITY).clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );
    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);

    let mut direction = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        direction += *transform.forward();
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= *transform.forward();
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += *transform.right();
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= *transform.right();
    }
    if keys.pressed(KeyCode::KeyE) {
        direction += Vec3::Y;
    }
    if keys.pressed(KeyCode::KeyQ) {
        direction -= Vec3::Y;
    }

    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    transform.translation += direction.normalize_or_zero() * speed * time.delta_secs();
}

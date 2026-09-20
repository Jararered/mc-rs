use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::state::AppScreen;
use crate::inventory::Hotbar;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;

/// Full player health in half-hearts. Ten hearts on the HUD.
pub const MAX_PLAYER_HEALTH: u8 = 20;

const HOTBAR_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_player)
            .add_systems(OnEnter(AppScreen::Playing), capture_mouse)
            .add_systems(OnEnter(AppScreen::Menu), release_mouse)
            .add_systems(OnEnter(AppScreen::Settings), release_mouse)
            .add_systems(
                Update,
                (update_mouse_capture, move_player, select_hotbar)
                    .chain()
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Component)]
pub struct Player;

/// Current health in half-hearts. Each HUD heart is two points.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerHealth {
    pub current: u8,
}

impl Default for PlayerHealth {
    fn default() -> Self {
        Self {
            current: MAX_PLAYER_HEALTH,
        }
    }
}

/// How a single HUD heart should be filled from current health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartFill {
    Empty,
    Half,
    Full,
}

impl PlayerHealth {
    pub fn heart_fill(self, index: usize) -> HeartFill {
        let health = self.current.min(MAX_PLAYER_HEALTH);
        let start = (index as u8).saturating_mul(2);
        if health >= start + 2 {
            HeartFill::Full
        } else if health == start + 1 {
            HeartFill::Half
        } else {
            HeartFill::Empty
        }
    }
}

const WALK_SPEED: f32 = 5.0;
const SPRINT_SPEED: f32 = 15.0;
const MOUSE_SENSITIVITY: f32 = 0.002;

fn spawn_player(
    mut commands: Commands,
    chunks: Res<WorldChunks>,
    persistence: Option<Res<WorldPersistence>>,
) {
    let transform = persistence
        .as_ref()
        .and_then(|persistence| persistence.storage())
        .and_then(|storage| storage.load_player())
        .map(|player| player.to_transform())
        .unwrap_or_else(|| default_spawn_transform(&chunks));
    commands.spawn((
        Name::new("Player"),
        Player,
        PlayerHealth::default(),
        Hotbar::default(),
        Camera3d::default(),
        transform,
    ));
}

fn default_spawn_transform(chunks: &WorldChunks) -> Transform {
    let (high, center) = chunks
        .get(ChunkPos::ZERO)
        .map_or((80.0, 64.0), |generated| {
            (
                generated.heightmap.max() as f32,
                generated.heightmap.get(8, 8) as f32,
            )
        });
    Transform::from_xyz(8.0, high + 18.0, 40.0).looking_at(Vec3::new(8.0, center, 8.0), Vec3::Y)
}

fn capture_mouse(mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    if let Ok((window, mut cursor)) = windows.single_mut()
        && window.focused
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn release_mouse(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn update_mouse_capture(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut next_screen: ResMut<NextState<AppScreen>>,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Escape) {
        next_screen.set(AppScreen::Menu);
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if !window.focused {
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
    if keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::KeyE) {
        direction += Vec3::Y;
    }
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::KeyQ) {
        direction -= Vec3::Y;
    }

    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    transform.translation += direction.normalize_or_zero() * speed * time.delta_secs();
}

fn select_hotbar(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut hotbar: Query<&mut Hotbar, With<Player>>,
) {
    let Ok(mut hotbar) = hotbar.single_mut() else {
        return;
    };

    if scroll.delta.y != 0.0 {
        hotbar.scroll(if scroll.delta.y > 0.0 { 1 } else { -1 });
    }
    for (slot, key) in HOTBAR_KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            hotbar.select(slot);
        }
    }
}

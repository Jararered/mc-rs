use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Gravity;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::inventory::Hotbar;
mod arm;
mod held_mesh;
mod interaction;
mod mining;
mod overlay;

pub use arm::interpolated_swing;
pub use interaction::PLACED_BLOCK;
pub use interaction::break_block;
pub use interaction::place_block;
pub use interaction::place_selected_block;
pub use mining::MiningState;
pub use mining::destroy_stage;
pub use mining::hand_ticks_to_break;
pub use overlay::BlockFocus;
pub use overlay::OUTLINE_THICKNESS;
pub use overlay::destroy_overlay_mesh;
pub use overlay::punch_nearly_transparent_texels;
pub use overlay::selection_outline_mesh;

use crate::physics::PhysicsSet;
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
        overlay::overlay_plugin(app);
        arm::plugin(app);
        app.add_systems(PostStartup, spawn_player)
            .add_systems(OnEnter(AppScreen::Playing), capture_mouse)
            .add_systems(OnEnter(AppScreen::Menu), release_mouse)
            .add_systems(OnEnter(AppScreen::Settings), release_mouse)
            .add_systems(Update, apply_camera_fov)
            .add_systems(
                Update,
                (
                    look_player,
                    interaction::interact_blocks,
                    update_mouse_capture,
                    apply_player_input,
                    select_hotbar,
                )
                    .chain()
                    .in_set(PhysicsSet::ApplyInput)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                update_camera_bobbing
                    .after(PhysicsSet::Integrate)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Component)]
#[require(
    Transform,
    Velocity,
    CollisionState,
    Gravity,
    EntitySize = EntitySize::PLAYER,
    StepHeight = StepHeight::PLAYER
)]
pub struct Player;

/// The render camera is a child of the physics player so view bobbing does
/// not move the player's collision box or interaction origin.
#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component, Default)]
struct CameraBobbing {
    distance_walked: f32,
    camera_yaw: f32,
    camera_pitch: f32,
}

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

/// Horizontal movement speeds in blocks per second.
const WALK_SPEED: f32 = 4.317;
const SPRINT_SPEED: f32 = 5.612;
const SNEAK_SPEED: f32 = 1.295;
const SPRINT_JUMP_SPEED: f32 = 7.1;
const JUMP_SPEED: f32 = 8.4;
const MOUSE_SENSITIVITY: f32 = 0.002;

fn spawn_player(
    mut commands: Commands,
    chunks: Res<WorldChunks>,
    persistence: Option<Res<WorldPersistence>>,
    settings: Res<GameSettings>,
    arm_assets: Res<arm::ArmAssets>,
) {
    let saved = persistence
        .as_ref()
        .and_then(|persistence| persistence.storage())
        .and_then(|storage| storage.load_player());
    let transform = saved
        .as_ref()
        .map(|player| player.to_transform())
        .unwrap_or_else(|| default_spawn_transform(&chunks));
    let (hotbar, inventory) = saved
        .as_ref()
        .map(|player| player.to_inventory())
        .unwrap_or_default();
    commands
        .spawn((
            Name::new("Player"),
            Player,
            PlayerHealth::default(),
            hotbar,
            inventory,
            CameraBobbing::default(),
            transform,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    PlayerCamera,
                    Camera3d::default(),
                    Projection::from(PerspectiveProjection {
                        fov: settings.fov_radians(),
                        ..default()
                    }),
                    Transform::default(),
                ))
                .with_children(|camera| arm::spawn(camera, &arm_assets, settings.fov_radians()));
        });
}

fn apply_camera_fov(
    settings: Res<GameSettings>,
    mut cameras: Query<&mut Projection, With<PlayerCamera>>,
) {
    if !settings.is_changed() {
        return;
    }
    let fov = settings.fov_radians();
    for projection in &mut cameras {
        if let Projection::Perspective(perspective) = projection.into_inner() {
            perspective.fov = fov;
        }
    }
}

/// Reproduces the Beta view-bobbing transform from `EntityRenderer`: walking
/// distance drives the phase while smoothed horizontal and vertical motion
/// control the bob's amplitude.
fn update_camera_bobbing(
    time: Res<Time>,
    mut players: Query<(&Velocity, &CollisionState, &mut CameraBobbing, &Children)>,
    mut cameras: Query<&mut Transform, With<PlayerCamera>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (velocity, collision, mut bob, children) in &mut players {
        let horizontal_motion = velocity.0.xz().length() * dt;
        bob.distance_walked += horizontal_motion * 0.6;

        let target_yaw = if collision.on_ground {
            horizontal_motion.min(0.1)
        } else {
            0.0
        };
        let vertical_motion = velocity.0.y * dt;
        let target_pitch = if collision.on_ground {
            0.0
        } else {
            (-vertical_motion * 0.2).atan() * 15.0
        };
        bob.camera_yaw += (target_yaw - bob.camera_yaw) * 0.4;
        bob.camera_pitch += (target_pitch - bob.camera_pitch) * 0.8;

        for child in children {
            if let Ok(mut camera) = cameras.get_mut(*child) {
                *camera = Transform::from_matrix(camera_bob_pose(&bob));
            }
        }
    }
}

fn camera_bob_pose(bob: &CameraBobbing) -> Mat4 {
    let phase = -bob.distance_walked * std::f32::consts::PI;
    let lateral = phase.sin() * bob.camera_yaw * 0.5;
    let vertical = -(phase.cos() * bob.camera_yaw).abs();
    let roll = (phase.sin() * bob.camera_yaw * 3.0).to_radians();
    let pitch = ((phase - 0.2).cos() * bob.camera_yaw).abs() * 5.0 + bob.camera_pitch;
    Mat4::from_translation(Vec3::new(lateral, vertical, 0.0))
        * Mat4::from_rotation_z(roll)
        * Mat4::from_rotation_x(pitch.to_radians())
}

fn default_spawn_transform(chunks: &WorldChunks) -> Transform {
    let surface = chunks
        .get(ChunkPos::ZERO)
        .map_or(64.0, |generated| generated.heightmap.get(8, 8) as f32);
    let eye = surface + EntitySize::PLAYER.y_offset;
    Transform::from_xyz(8.5, eye, 8.5).looking_at(Vec3::new(8.5, eye, 16.5), Vec3::Y)
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
    inventory_screen: Res<crate::ui::InventoryScreen>,
) {
    if inventory_screen.open {
        return;
    }
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Escape) && cursor.grab_mode == CursorGrabMode::Locked {
        next_screen.set(AppScreen::Menu);
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if !window.focused {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if cursor.grab_mode != CursorGrabMode::Locked
        && mouse_buttons.just_pressed(MouseButton::Left)
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

fn look_player(
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
}

fn apply_player_input(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    mut player: Query<(&Transform, &mut Velocity, &CollisionState), With<Player>>,
) {
    let Ok((transform, mut velocity, collision)) = player.single_mut() else {
        return;
    };

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);

    let mut direction = Vec3::ZERO;
    if locked {
        let mut forward = *transform.forward();
        forward.y = 0.0;
        let forward = forward.normalize_or_zero();
        let mut right = *transform.right();
        right.y = 0.0;
        let right = right.normalize_or_zero();

        if keys.pressed(KeyCode::KeyW) {
            direction += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            direction -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            direction += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            direction -= right;
        }
    }

    let sneaking = locked && sneak_pressed(&keys);
    let sprinting = locked && keys.pressed(KeyCode::ShiftLeft) && !sneaking;
    let jumping = locked && keys.pressed(KeyCode::Space);
    let speed = if sneaking {
        SNEAK_SPEED
    } else if sprinting && jumping {
        SPRINT_JUMP_SPEED
    } else if sprinting {
        SPRINT_SPEED
    } else {
        WALK_SPEED
    };
    let horizontal = direction.normalize_or_zero() * speed;
    velocity.0.x = horizontal.x;
    velocity.0.z = horizontal.z;

    if locked && keys.pressed(KeyCode::Space) && collision.on_ground {
        velocity.0.y = JUMP_SPEED;
    }
}

#[cfg(target_os = "macos")]
fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
}

#[cfg(not(target_os = "macos"))]
fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

fn select_hotbar(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    inventory_screen: Option<Res<crate::ui::InventoryScreen>>,
    mut hotbar: Query<&mut Hotbar, With<Player>>,
) {
    let Ok(mut hotbar) = hotbar.single_mut() else {
        return;
    };

    if scroll.delta.y != 0.0 {
        hotbar.scroll(if scroll.delta.y > 0.0 { 1 } else { -1 });
    }
    // While the inventory is open, 1–9 move the hovered stack instead of
    // changing the selected slot.
    if inventory_screen.is_some_and(|screen| screen.open) {
        return;
    }
    for (slot, key) in HOTBAR_KEYS.iter().enumerate() {
        if keys.just_pressed(*key) {
            hotbar.select(slot);
        }
    }
}

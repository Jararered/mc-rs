//! Keyboard and mouse: mouse capture, look, movement input, flight, and the
//! hotbar keys.

use super::state::FlySpeed;
use super::state::GameMode;
use super::state::MAX_FLY_SPEED;
use super::state::MIN_FLY_SPEED;
use super::state::Player;
use super::state::PlayerMovementInput;
use crate::app::settings::GameSettings;
use crate::app::state::PauseMenu;
use crate::entity::CollisionState;
use crate::entity::Flying;
use crate::entity::Velocity;
use crate::inventory::Hotbar;
use crate::player::interaction::use_item::BowDraw;
use crate::player::interaction::use_item::DRAW_MOVEMENT_SCALE;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

pub(super) const HOTBAR_KEYS: [KeyCode; 9] = [
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

/// Horizontal movement speeds in blocks per second.
pub(super) const WALK_SPEED: f32 = 4.317;
pub(super) const SPRINT_SPEED: f32 = 5.612;
pub(crate) const SPRINT_ACCELERATION_MULTIPLIER: f32 = SPRINT_SPEED / WALK_SPEED;
pub(super) const MOUSE_SENSITIVITY: f32 = 0.002;

/// Flying base speed in blocks per second, as creative flight in current
/// Minecraft. Sprinting doubles the horizontal part.
pub(super) const FLY_SPEED: f32 = 10.92;
pub(super) const FLY_VERTICAL_SPEED: f32 = 7.5;
pub(super) const FLY_SPRINT_MULTIPLIER: f32 = 2.0;
/// Share of flying velocity kept each tick, horizontally and vertically.
pub(super) const FLY_HORIZONTAL_DRAG: f32 = 0.91;
pub(super) const FLY_VERTICAL_DRAG: f32 = 0.6;
pub(super) const FLY_SPEED_STEP: f32 = 1.25;
/// Longest gap between two presses of jump that toggles creative flight.
pub(super) const FLIGHT_DOUBLE_TAP_SECS: f32 = 0.35;

pub(super) fn chat_controls_active(
    chat: Option<Res<crate::chat::ChatFocus>>,
    pause: Option<Res<PauseMenu>>,
) -> bool {
    chat.is_none_or(|chat| !chat.suppress_controls) && pause.is_none_or(|pause| !pause.open)
}

pub(super) fn capture_mouse(
    pause: Res<PauseMenu>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    if pause.open {
        return;
    }
    if let Ok((window, mut cursor)) = windows.single_mut()
        && window.focused
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

pub(super) fn release_mouse(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

pub(super) fn update_mouse_capture(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    inventory_screen: Res<crate::inventory::session::InventorySession>,
    pause: Res<PauseMenu>,
) {
    // The pause menu owns the cursor; Escape is handled by its own toggle.
    if inventory_screen.open || pause.open {
        return;
    }
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };

    if !window.focused {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    } else if cursor.grab_mode != CursorGrabMode::Locked
        && mouse_buttons.just_pressed(MouseButton::Left)
    {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }
}

/// Creative flight starts and stops on F or a double tap of jump.
pub(super) fn toggle_flying(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut last_jump: Local<Option<f32>>,
    mut player: Query<(Entity, &GameMode, Has<Flying>, &mut FlySpeed, &mut Velocity), With<Player>>,
    mut commands: Commands,
) {
    let mut toggle = keys.just_pressed(KeyCode::KeyF);
    if keys.just_pressed(KeyCode::Space) {
        let now = time.elapsed_secs();
        if last_jump.is_some_and(|last| now - last <= FLIGHT_DOUBLE_TAP_SECS) {
            toggle = true;
            *last_jump = None;
        } else {
            *last_jump = Some(now);
        }
    }
    if !toggle {
        return;
    }
    let Ok((entity, mode, flying, mut fly_speed, mut velocity)) = player.single_mut() else {
        return;
    };
    if !mode.toggles_flight() {
        return;
    }
    if flying {
        commands.entity(entity).remove::<Flying>();
    } else {
        fly_speed.clamp_value();
        // Standing still carries gravity's pull, which would land the flight
        // on its first frame.
        velocity.0.y = velocity.0.y.max(0.0);
        commands.entity(entity).insert(Flying);
    }
}

/// Plus and minus change the flying speed. A spectator has no use for the
/// hotbar, so the scroll wheel changes it too.
pub(super) fn adjust_fly_speed(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut player: Query<(&GameMode, &mut FlySpeed), (With<Player>, With<Flying>)>,
) {
    let Ok((mode, mut fly_speed)) = player.single_mut() else {
        return;
    };
    let mut steps = 0;
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        steps += 1;
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        steps -= 1;
    }
    if *mode == GameMode::Spectator && scroll.delta.y != 0.0 {
        steps += if scroll.delta.y > 0.0 { 1 } else { -1 };
    }
    if steps != 0 {
        fly_speed.0 =
            (fly_speed.0 * FLY_SPEED_STEP.powi(steps)).clamp(MIN_FLY_SPEED, MAX_FLY_SPEED);
    }
}

pub(super) fn look_player(
    settings: Res<GameSettings>,
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
    yaw -= mouse_motion.delta.x * MOUSE_SENSITIVITY * settings.mouse_sensitivity;
    pitch = (pitch - mouse_motion.delta.y * MOUSE_SENSITIVITY * settings.mouse_sensitivity).clamp(
        -std::f32::consts::FRAC_PI_2 + 0.01,
        std::f32::consts::FRAC_PI_2 - 0.01,
    );
    transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}

pub(super) fn apply_player_input(
    chat: Option<Res<crate::chat::ChatFocus>>,
    pause: Option<Res<PauseMenu>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    mut player: Query<
        (
            &Transform,
            &mut Velocity,
            &CollisionState,
            Option<&Flying>,
            &FlySpeed,
            &mut PlayerMovementInput,
            Option<&BowDraw>,
        ),
        With<Player>,
    >,
) {
    let Ok((transform, mut velocity, _collision, flying, fly_speed, mut movement_input, draw)) =
        player.single_mut()
    else {
        return;
    };
    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked)
        && chat_controls_active(chat, pause);

    if flying.is_some() {
        // W/S follow the heading rather than the pitch; jump and sneak climb
        // and sink.
        let mut target = Vec3::ZERO;
        if locked {
            let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
            let heading = Quat::from_rotation_y(yaw);
            let horizontal = (heading
                * Vec3::new(
                    axis(keys.pressed(KeyCode::KeyD), keys.pressed(KeyCode::KeyA)),
                    0.0,
                    axis(keys.pressed(KeyCode::KeyS), keys.pressed(KeyCode::KeyW)),
                ))
            .normalize_or_zero();
            let sprint = if sprint_pressed(&keys) {
                FLY_SPRINT_MULTIPLIER
            } else {
                1.0
            };
            target = horizontal * FLY_SPEED * sprint;
            target.y =
                axis(keys.pressed(KeyCode::Space), sneak_pressed(&keys)) * FLY_VERTICAL_SPEED;
            target *= fly_speed.0;
        }
        // The drag is per tick; apply the same decay over this frame.
        let ticks = time.delta_secs() / crate::world::tick::TICK_SECONDS;
        let horizontal = 1.0 - FLY_HORIZONTAL_DRAG.powf(ticks);
        let vertical = 1.0 - FLY_VERTICAL_DRAG.powf(ticks);
        let current = velocity.0;
        velocity.0 += (target - current) * Vec3::new(horizontal, vertical, horizontal);
        *movement_input = PlayerMovementInput::default();
        return;
    }

    let sneaking = locked && sneak_pressed(&keys);
    let sprinting = locked && sprint_pressed(&keys) && !sneaking;
    movement_input.strafe = if locked {
        axis(keys.pressed(KeyCode::KeyA), keys.pressed(KeyCode::KeyD))
    } else {
        0.0
    };
    movement_input.forward = if locked {
        axis(keys.pressed(KeyCode::KeyW), keys.pressed(KeyCode::KeyS))
    } else {
        0.0
    };
    movement_input.sneaking = sneaking;
    movement_input.sprinting = sprinting;
    movement_input.jumping = locked && keys.pressed(KeyCode::Space);
    if sneaking {
        movement_input.strafe *= 0.3;
        movement_input.forward *= 0.3;
    }
    // Beta 1.8 `EntityPlayerSP.onLivingUpdate`: using an item slows the walk.
    if draw.is_some() {
        movement_input.strafe *= DRAW_MOVEMENT_SCALE;
        movement_input.forward *= DRAW_MOVEMENT_SCALE;
    }
}

pub(super) fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

#[cfg(target_os = "macos")]
pub(super) fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

#[cfg(target_os = "macos")]
pub(super) fn sprint_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn sprint_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

pub(super) fn select_hotbar(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    inventory_screen: Option<Res<crate::inventory::session::InventorySession>>,
    mut hotbar: Query<(&mut Hotbar, &GameMode), With<Player>>,
) {
    let Ok((mut hotbar, mode)) = hotbar.single_mut() else {
        return;
    };

    // A spectator's wheel sets the flying speed instead.
    if scroll.delta.y != 0.0 && *mode != GameMode::Spectator {
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

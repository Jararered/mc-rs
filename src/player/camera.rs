//! The first-person camera: field of view, view bobbing, and the hurt roll.

use super::interaction::use_item::BowDraw;
use super::state::LocalPlayer;
use super::state::PlayerInterpolation;
use crate::app::settings::GameSettings;
use crate::entity::CollisionState;
use crate::entity::Flying;
use crate::entity::Velocity;
use crate::entity::combat::HURT_TICKS;
use crate::entity::combat::PlayerCombat;
use crate::world::tick::WorldTick;
use bevy::prelude::*;

/// The render camera is a child of the physics player so view bobbing does
/// not move the player's collision box or interaction origin.
#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component, Default)]
pub(super) struct CameraBobbing {
    distance_walked: f32,
    camera_yaw: f32,
    camera_pitch: f32,
}

/// Where the player's eyes are drawn this frame: between the last tick's
/// position and this one's, as the camera is.
pub(crate) fn rendered_eye(
    transform: &Transform,
    interpolation: Option<&PlayerInterpolation>,
    partial: f32,
) -> Vec3 {
    interpolation.map_or(transform.translation, |interpolation| {
        interpolation
            .previous_position
            .lerp(transform.translation, partial.clamp(0.0, 1.0))
    })
}

/// How much a full bow draw narrows the view.
const DRAW_ZOOM: f32 = 0.15;

/// The view's scale for a bow drawn `ticks` ticks, after modern
/// `getFieldOfViewModifier`: it narrows with the square of the draw, to 15%
/// less after a second.
pub fn draw_fov_scale(ticks: f32) -> f32 {
    let drawn = (ticks / 20.0).clamp(0.0, 1.0);
    1.0 - drawn * drawn * DRAW_ZOOM
}

/// The view scale now and a tick ago. It closes half the gap to its target
/// each tick, so letting the string go eases the view back out.
pub(super) struct FovZoom {
    previous: f32,
    current: f32,
}

impl Default for FovZoom {
    fn default() -> Self {
        Self {
            previous: 1.0,
            current: 1.0,
        }
    }
}

pub(super) fn apply_camera_fov(
    settings: Res<GameSettings>,
    tick: Option<Res<WorldTick>>,
    draws: Query<&BowDraw, With<LocalPlayer>>,
    mut cameras: Query<&mut Projection, With<PlayerCamera>>,
    mut zoom: Local<FovZoom>,
) {
    let target = draws
        .single()
        .map_or(1.0, |draw| draw_fov_scale(draw.ticks as f32));
    let (ticks, partial) = tick
        .as_ref()
        .map_or((0, 1.0), |tick| (tick.ticks_this_frame(), tick.partial()));
    for _ in 0..ticks {
        zoom.previous = zoom.current;
        zoom.current += (target - zoom.current) * 0.5;
        if (target - zoom.current).abs() < 1e-4 {
            zoom.current = target;
        }
    }
    let scale = zoom.previous + (zoom.current - zoom.previous) * partial;
    let fov = settings.fov_radians() * scale;
    for mut projection in &mut cameras {
        // Written only when it differs, so a steady view changes nothing.
        if let Projection::Perspective(perspective) = projection.as_ref()
            && perspective.fov != fov
            && let Projection::Perspective(perspective) = projection.as_mut()
        {
            perspective.fov = fov;
        }
    }
}

/// Reproduces the Beta view-bobbing transform from `EntityRenderer`: walking
/// distance drives the phase while smoothed horizontal and vertical motion
/// control the bob's amplitude.
pub(super) fn update_camera_bobbing(
    settings: Res<GameSettings>,
    time: Res<Time>,
    tick: Res<WorldTick>,
    mut players: Query<
        (
            &Transform,
            &PlayerInterpolation,
            &Velocity,
            &CollisionState,
            &mut CameraBobbing,
            &Children,
            Option<&PlayerCombat>,
            Has<Flying>,
        ),
        (With<LocalPlayer>, Without<PlayerCamera>),
    >,
    mut cameras: Query<&mut Transform, With<PlayerCamera>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (transform, interpolation, velocity, collision, mut bob, children, combat, flying) in
        &mut players
    {
        let horizontal_motion = velocity.0.xz().length() * dt;
        bob.distance_walked += horizontal_motion * 0.6;

        let target_yaw = if collision.on_ground {
            horizontal_motion.min(0.1)
        } else {
            0.0
        };
        let vertical_motion = velocity.0.y * dt;
        let target_pitch = if collision.on_ground || flying {
            0.0
        } else {
            (-vertical_motion * 0.2).atan() * 15.0
        };
        bob.camera_yaw += (target_yaw - bob.camera_yaw) * 0.4;
        bob.camera_pitch += (target_pitch - bob.camera_pitch) * 0.8;
        let current = transform.translation;
        let interpolated = interpolation
            .previous_position
            .lerp(current, tick.partial().clamp(0.0, 1.0));
        let render_offset = transform.rotation.inverse() * (interpolated - current);
        let hurt = combat.map_or(Mat4::IDENTITY, |combat| hurt_pose(combat, tick.partial()));

        for child in children {
            if let Ok(mut camera) = cameras.get_mut(*child) {
                camera.set_if_neq(Transform::from_matrix(
                    Mat4::from_translation(render_offset)
                        * hurt
                        * if settings.view_bobbing {
                            camera_bob_pose(&bob)
                        } else {
                            Mat4::IDENTITY
                        },
                ));
            }
        }
    }
}

/// Peak roll of the hit camera effect, in degrees.
pub(super) const HURT_ROLL_DEGREES: f32 = 14.0;
/// Fraction of the hurt time spent easing into the peak.
pub(super) const HURT_ROLL_RISE: f32 = 0.2;

/// The roll for `elapsed` ticks of hurt time left (`HURT_TICKS` right after a
/// hit, 0 when it ends). Beta's `sin(p^4 * PI)` snaps to its peak within two
/// ticks, which looks jittery at frame rate; this eases in over the first
/// `HURT_ROLL_RISE` and out smoothly, with the same peak.
pub fn hurt_roll_degrees(elapsed: f32) -> f32 {
    if elapsed < 0.0 {
        return 0.0;
    }
    let age = (1.0 - elapsed / f32::from(HURT_TICKS)).clamp(0.0, 1.0);
    let envelope = if age < HURT_ROLL_RISE {
        let t = age / HURT_ROLL_RISE;
        t * t * (3.0 - 2.0 * t)
    } else {
        let t = (age - HURT_ROLL_RISE) / (1.0 - HURT_ROLL_RISE);
        (1.0 - t) * (1.0 - t)
    };
    envelope * HURT_ROLL_DEGREES
}

/// `EntityRenderer.hurtCameraEffect`: a hit rolls the view away from the side
/// it came from, easing back over the hurt time.
pub(super) fn hurt_pose(combat: &PlayerCombat, partial: f32) -> Mat4 {
    let elapsed = f32::from(combat.hurt_time) - partial;
    if elapsed < 0.0 {
        return Mat4::IDENTITY;
    }
    let roll = hurt_roll_degrees(elapsed);
    let side = combat.attacked_at_yaw.to_radians();
    Mat4::from_rotation_y(-side)
        * Mat4::from_rotation_z((-roll).to_radians())
        * Mat4::from_rotation_y(side)
}

pub(super) fn camera_bob_pose(bob: &CameraBobbing) -> Mat4 {
    let phase = -bob.distance_walked * std::f32::consts::PI;
    let lateral = phase.sin() * bob.camera_yaw * 0.5;
    let vertical = -(phase.cos() * bob.camera_yaw).abs();
    let roll = (phase.sin() * bob.camera_yaw * 3.0).to_radians();
    let pitch = ((phase - 0.2).cos() * bob.camera_yaw).abs() * 5.0 + bob.camera_pitch;
    Mat4::from_translation(Vec3::new(lateral, vertical, 0.0))
        * Mat4::from_rotation_z(roll)
        * Mat4::from_rotation_x(pitch.to_radians())
}

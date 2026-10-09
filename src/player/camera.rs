//! The first-person camera: field of view, view bobbing, and the hurt roll.

use super::state::Player;
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

pub(super) fn apply_camera_fov(
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
        (With<Player>, Without<PlayerCamera>),
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

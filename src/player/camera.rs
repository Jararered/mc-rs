//! The first-person camera: field of view, view bobbing, and the hurt roll.

use super::interaction::use_item::BowDraw;
use super::state::LocalPlayer;
use super::state::PlayerHealth;
use super::state::PlayerInterpolation;
use super::state::PlayerMovementInput;
use crate::app::settings::GameSettings;
use crate::entity::CollisionState;
use crate::entity::Flying;
use crate::entity::Velocity;
use crate::entity::combat::HURT_TICKS;
use crate::entity::combat::PlayerCombat;
use crate::entity::mount::Mounted;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;
use bevy::prelude::*;

/// The render camera is a child of the physics player so view bobbing does
/// not move the player's collision box or interaction origin.
#[derive(Component)]
pub struct PlayerCamera;

/// The local player's view state from Beta's `Entity` and `EntityPlayer`,
/// stepped once a world tick and drawn between the last two ticks.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct CameraBobbing {
    /// `distanceWalkedModified`: the bob's phase.
    pub distance_walked: f32,
    pub prev_distance_walked: f32,
    /// `cameraYaw`: the bob's size, from horizontal motion.
    pub camera_yaw: f32,
    pub prev_camera_yaw: f32,
    /// `cameraPitch`: the tilt of a fall or jump, in degrees.
    pub camera_pitch: f32,
    pub prev_camera_pitch: f32,
    /// `ySize`: how far the eyes sit below their standing height.
    pub y_size: f32,
    pub prev_y_size: f32,
}

/// `EntityPlayerSP.onLivingUpdate` holds `ySize` at least this high while
/// sneaking, before the move scales it.
const SNEAK_Y_SIZE: f32 = 0.2;
/// `Entity.moveEntity` scales `ySize` by this every tick.
const Y_SIZE_DECAY: f32 = 0.4;

impl CameraBobbing {
    /// One tick of the view. `moved` is the horizontal distance the tick's
    /// move covered and `motion` the velocity it left, both in blocks a tick
    /// (Beta's `motionX/Y/Z`).
    pub fn tick(
        &mut self,
        moved: f32,
        motion: Vec3,
        on_ground: bool,
        sneaking: bool,
        alive: bool,
        flying: bool,
    ) {
        self.prev_distance_walked = self.distance_walked;
        self.prev_camera_yaw = self.camera_yaw;
        self.prev_camera_pitch = self.camera_pitch;
        self.prev_y_size = self.y_size;

        if sneaking && self.y_size < SNEAK_Y_SIZE {
            self.y_size = SNEAK_Y_SIZE;
        }
        self.y_size *= Y_SIZE_DECAY;
        // `moveEntity` counts no walked distance for a sneak on the ground.
        if !(on_ground && sneaking) {
            self.distance_walked += moved * 0.6;
        }

        let target_yaw = if on_ground && alive {
            motion.xz().length().min(0.1)
        } else {
            0.0
        };
        let target_pitch = if on_ground || !alive || flying {
            0.0
        } else {
            (-motion.y * 0.2).atan() * 15.0
        };
        self.camera_yaw += (target_yaw - self.camera_yaw) * 0.4;
        self.camera_pitch += (target_pitch - self.camera_pitch) * 0.8;
    }

    /// How far below standing height the eyes are drawn.
    pub fn eye_drop(&self, partial: f32) -> f32 {
        self.prev_y_size + (self.y_size - self.prev_y_size) * partial
    }
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

/// Steps Beta's view state each world tick and poses the camera: the sneak
/// dip, the hurt roll, and `EntityRenderer.setupViewBobbing`.
pub(super) fn update_camera_bobbing(
    settings: Res<GameSettings>,
    tick: Res<WorldTick>,
    mut players: Query<
        (
            &Transform,
            &PlayerInterpolation,
            &Velocity,
            &CollisionState,
            &PlayerMovementInput,
            &PlayerHealth,
            &mut CameraBobbing,
            &Children,
            Option<&PlayerCombat>,
            Has<Flying>,
            Has<Mounted>,
        ),
        (With<LocalPlayer>, Without<PlayerCamera>),
    >,
    mut cameras: Query<&mut Transform, With<PlayerCamera>>,
) {
    let partial = tick.partial().clamp(0.0, 1.0);
    for (
        transform,
        interpolation,
        velocity,
        collision,
        input,
        health,
        mut bob,
        children,
        combat,
        flying,
        mounted,
    ) in &mut players
    {
        let current = transform.translation;
        // A rider is carried (`ridingEntity == null` in `moveEntity`).
        let moved = if mounted {
            0.0
        } else {
            (current - interpolation.previous_position).xz().length()
        };
        for _ in 0..tick.ticks_this_frame() {
            bob.tick(
                moved,
                velocity.0 * TICK_SECONDS,
                collision.on_ground,
                input.sneaking && !flying && !mounted,
                health.current > 0,
                flying,
            );
        }
        let interpolated = interpolation.previous_position.lerp(current, partial);
        let render_offset = transform.rotation.inverse() * (interpolated - current)
            - Vec3::Y * bob.eye_drop(partial);
        let hurt = combat.map_or(Mat4::IDENTITY, |combat| hurt_pose(combat, partial));

        for child in children {
            if let Ok(mut camera) = cameras.get_mut(*child) {
                camera.set_if_neq(Transform::from_matrix(
                    Mat4::from_translation(render_offset)
                        * hurt
                        * if settings.view_bobbing {
                            camera_bob_pose(&bob, partial)
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

/// `EntityRenderer.setupViewBobbing` at `partial` of the way to the next tick.
pub fn camera_bob_pose(bob: &CameraBobbing, partial: f32) -> Mat4 {
    let walked = bob.distance_walked + (bob.distance_walked - bob.prev_distance_walked) * partial;
    let yaw = bob.prev_camera_yaw + (bob.camera_yaw - bob.prev_camera_yaw) * partial;
    let fall = bob.prev_camera_pitch + (bob.camera_pitch - bob.prev_camera_pitch) * partial;
    let phase = -walked * std::f32::consts::PI;
    let lateral = phase.sin() * yaw * 0.5;
    let vertical = -(phase.cos() * yaw).abs();
    let roll = (phase.sin() * yaw * 3.0).to_radians();
    let pitch = ((phase - 0.2).cos() * yaw).abs() * 5.0 + fall;
    Mat4::from_translation(Vec3::new(lateral, vertical, 0.0))
        * Mat4::from_rotation_z(roll)
        * Mat4::from_rotation_x(pitch.to_radians())
}

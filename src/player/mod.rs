use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;
use serde::Deserialize;
use serde::Serialize;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::Gravity;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::combat::HURT_TICKS;
use crate::entity::combat::PlayerCombat;
use crate::inventory::Hotbar;
pub(crate) mod interaction;
pub(crate) mod model;
pub mod portal;
pub mod sleep;
mod survival;

pub use interaction::editing::PLACED_BLOCK;
pub use interaction::editing::break_block;
pub use interaction::editing::pick_up_fluid;
pub use interaction::editing::place_bed;
pub use interaction::editing::place_block;
pub use interaction::editing::place_door;
pub use interaction::editing::place_fluid;
pub use interaction::editing::place_selected_block;
pub use interaction::editing::place_selected_block_facing;
pub use interaction::editing::plant_seeds;
pub use interaction::editing::till_block;
pub use interaction::editing::till_with_selected_hoe;
pub use interaction::mining::MiningState;
pub use interaction::mining::destroy_stage;
pub use interaction::mining::hand_ticks_to_break;
pub use interaction::overlay::BlockFocus;
pub use interaction::overlay::destroy_overlay_mesh;
pub use interaction::overlay::double_crack_intensity;
pub use model::arm::interpolated_swing;
pub use survival::Bubble;
pub use survival::PlayerSurvival;
pub use survival::SurvivalPlugin;

use crate::physics::PhysicsSet;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::tick::WorldTick;

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
        interaction::overlay::overlay_plugin(app);
        model::arm::plugin(app);
        portal::plugin(app);
        app.init_resource::<PauseMenu>()
            .init_resource::<crate::inventory::session::InventorySession>()
            .init_resource::<crate::inventory::session::ActiveWorkbench>()
            .add_systems(
                PostStartup,
                spawn_player.run_if(crate::world::persistence::starts_with_world),
            )
            .add_systems(OnEnter(AppScreen::Playing), capture_mouse)
            .add_systems(OnEnter(AppScreen::Menu), release_mouse)
            .add_systems(OnEnter(AppScreen::Settings), release_mouse)
            .add_systems(OnEnter(AppScreen::WorldSelect), release_mouse)
            .add_systems(OnEnter(AppScreen::NewWorld), release_mouse)
            .add_systems(Update, apply_camera_fov)
            .add_systems(
                Update,
                (
                    look_player.run_if(chat_controls_active),
                    interaction::editing::interact_blocks.run_if(chat_controls_active),
                    update_mouse_capture.run_if(chat_controls_active),
                    toggle_flying.run_if(chat_controls_active),
                    adjust_fly_speed.run_if(chat_controls_active),
                    apply_player_input,
                    select_hotbar.run_if(chat_controls_active),
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
            )
            .add_plugins((survival::SurvivalPlugin, sleep::SleepPlugin));
    }
}

#[derive(Component)]
#[require(
    Transform,
    Velocity,
    CollisionState,
    Gravity,
    EntitySize = EntitySize::PLAYER,
    StepHeight = StepHeight::PLAYER,
    StepDistance,
    FlySpeed,
    GameMode,
    PlayerMovementInput,
    PlayerInterpolation,
    PlayerCombat,
    PlayerSurvival,
    portal::PortalTravel,
    sleep::PlayerSleep
)]
pub struct Player;

/// What the world lets the player do, set with `/gamemode`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
    /// Walks, falls, and takes damage.
    #[default]
    Survival,
    /// Takes no damage and may fly, still colliding with blocks.
    Creative,
    /// Takes no damage and always flies, passing through blocks.
    Spectator,
}

impl GameMode {
    pub const ALL: [Self; 3] = [Self::Survival, Self::Creative, Self::Spectator];

    pub fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Spectator => "spectator",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(name))
    }

    /// Hazards, mobs, and explosions can hurt the player.
    pub fn takes_damage(self) -> bool {
        self == Self::Survival
    }

    /// The player may switch [`Flying`] on and off.
    pub fn toggles_flight(self) -> bool {
        self == Self::Creative
    }

    /// Movement ignores blocks.
    pub fn noclip(self) -> bool {
        self == Self::Spectator
    }
}

/// Frame-sampled controls consumed by the tick-based player physics system.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct PlayerMovementInput {
    pub strafe: f32,
    pub forward: f32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub jumping: bool,
}

/// Previous fixed-tick position used to smooth the rendered first-person view.
#[derive(Component, Default, Clone, Copy, Debug)]
pub(crate) struct PlayerInterpolation {
    pub previous_position: Vec3,
}

/// How fast the player moves while flying. Multiplied by [`FLY_SPEED`].
#[derive(Component, Clone, Copy, Debug)]
pub struct FlySpeed(pub f32);

impl Default for FlySpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

impl FlySpeed {
    pub fn clamp_value(&mut self) {
        self.0 = self.0.clamp(MIN_FLY_SPEED, MAX_FLY_SPEED);
    }
}

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
pub(crate) const SPRINT_ACCELERATION_MULTIPLIER: f32 = SPRINT_SPEED / WALK_SPEED;
const MOUSE_SENSITIVITY: f32 = 0.002;

/// Flying base speed in blocks per second, as creative flight in current
/// Minecraft. Sprinting doubles the horizontal part.
const FLY_SPEED: f32 = 10.92;
const FLY_VERTICAL_SPEED: f32 = 7.5;
const FLY_SPRINT_MULTIPLIER: f32 = 2.0;
/// Share of flying velocity kept each tick, horizontally and vertically.
const FLY_HORIZONTAL_DRAG: f32 = 0.91;
const FLY_VERTICAL_DRAG: f32 = 0.6;
const MIN_FLY_SPEED: f32 = 0.25;
const MAX_FLY_SPEED: f32 = 50.0;
const FLY_SPEED_STEP: f32 = 1.25;
/// Longest gap between two presses of jump that toggles creative flight.
const FLIGHT_DOUBLE_TAP_SECS: f32 = 0.35;

pub(crate) fn spawn_player(
    mut commands: Commands,
    chunks: Res<WorldChunks>,
    persistence: Option<Res<WorldPersistence>>,
    settings: Res<GameSettings>,
    arm_assets: Res<model::arm::ArmAssets>,
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
    let game_mode = saved.as_ref().map(|p| p.game_mode).unwrap_or_default();
    let flying = match game_mode {
        GameMode::Survival => false,
        GameMode::Creative => saved.as_ref().is_some_and(|p| p.flying),
        GameMode::Spectator => true,
    };
    let fly_speed = saved.as_ref().map(|p| p.fly_speed).unwrap_or(1.0);
    let interpolation = PlayerInterpolation {
        previous_position: transform.translation,
    };
    let mut entity = commands.spawn((
        Name::new("Player"),
        Player,
        PlayerHealth {
            current: saved
                .as_ref()
                .map_or(MAX_PLAYER_HEALTH, |p| p.health.min(MAX_PLAYER_HEALTH)),
        },
        saved.as_ref().map_or_else(PlayerSurvival::default, |p| {
            PlayerSurvival::restored(p.air, p.fire, p.fall_distance)
        }),
        hotbar,
        inventory,
        CameraBobbing::default(),
        FlySpeed(fly_speed),
        game_mode,
        interpolation,
        transform,
        sleep::PlayerSleep::with_spawn(saved.as_ref().and_then(|p| p.spawn).map(IVec3::from_array)),
    ));
    if flying {
        entity.insert(Flying);
    }
    entity.with_children(|parent| {
        parent
            .spawn((
                PlayerCamera,
                Camera3d::default(),
                // Beta has no tonemap; the default one desaturates textures.
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                Projection::from(PerspectiveProjection {
                    fov: settings.fov_radians(),
                    ..default()
                }),
                bevy::camera::visibility::RenderLayers::from_layers(&[
                    0,
                    interaction::overlay::SELECTION_LAYER,
                ]),
                Transform::default(),
            ))
            .with_children(|camera| model::arm::spawn(camera, &arm_assets, settings.fov_radians()));
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

/// `EntityRenderer.hurtCameraEffect`: a hit rolls the view up to 14° away
/// from the side it came from, easing back over the hurt time.
fn hurt_pose(combat: &PlayerCombat, partial: f32) -> Mat4 {
    let elapsed = f32::from(combat.hurt_time) - partial;
    if elapsed < 0.0 {
        return Mat4::IDENTITY;
    }
    let progress = elapsed / f32::from(HURT_TICKS);
    let roll = (progress.powi(4) * std::f32::consts::PI).sin() * 14.0;
    let side = combat.attacked_at_yaw.to_radians();
    Mat4::from_rotation_y(-side)
        * Mat4::from_rotation_z((-roll).to_radians())
        * Mat4::from_rotation_y(side)
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

/// The world spawn point: on the surface of the origin chunk's middle column.
pub(crate) fn default_spawn_feet(chunks: &WorldChunks) -> Vec3 {
    let surface = chunks
        .get(ChunkPosition::ZERO)
        .map_or(64.0, |generated| generated.heightmap.get(8, 8) as f32);
    Vec3::new(8.5, surface, 8.5)
}

pub(crate) fn default_spawn_transform(chunks: &WorldChunks) -> Transform {
    let eye = default_spawn_feet(chunks).y + EntitySize::PLAYER.y_offset;
    Transform::from_xyz(8.5, eye, 8.5).looking_at(Vec3::new(8.5, eye, 16.5), Vec3::Y)
}

fn chat_controls_active(
    chat: Option<Res<crate::chat::ChatFocus>>,
    pause: Option<Res<PauseMenu>>,
) -> bool {
    chat.is_none_or(|chat| !chat.suppress_controls) && pause.is_none_or(|pause| !pause.open)
}

fn capture_mouse(
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

fn release_mouse(mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}

fn update_mouse_capture(
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
fn toggle_flying(
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
fn adjust_fly_speed(
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

fn look_player(
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

fn apply_player_input(
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
        ),
        With<Player>,
    >,
) {
    let Ok((transform, mut velocity, _collision, flying, fly_speed, mut movement_input)) =
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
}

fn axis(positive: bool, negative: bool) -> f32 {
    f32::from(u8::from(positive)) - f32::from(u8::from(negative))
}

#[cfg(target_os = "macos")]
fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

#[cfg(target_os = "macos")]
fn sprint_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
}

#[cfg(not(target_os = "macos"))]
fn sneak_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

#[cfg(not(target_os = "macos"))]
fn sprint_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

fn select_hotbar(
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

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
use crate::entity::Flying;
use crate::entity::Gravity;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::combat::HURT_TICKS;
use crate::entity::combat::PlayerCombat;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
mod interaction;
pub(crate) mod model;

pub use interaction::editing::PLACED_BLOCK;
pub use interaction::editing::break_block;
pub use interaction::editing::pick_up_fluid;
pub use interaction::editing::place_block;
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
        app.init_resource::<crate::inventory::session::InventorySession>()
            .init_resource::<crate::inventory::session::ActiveWorkbench>()
            .add_systems(PostStartup, spawn_player)
            .add_systems(OnEnter(AppScreen::Playing), capture_mouse)
            .add_systems(OnEnter(AppScreen::Menu), release_mouse)
            .add_systems(OnEnter(AppScreen::Settings), release_mouse)
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
            .add_systems(
                Update,
                tick_player_survival
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
    StepHeight = StepHeight::PLAYER,
    StepDistance,
    FlySpeed,
    PlayerMovementInput,
    PlayerInterpolation,
    PlayerCombat
)]
pub struct Player;

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

#[derive(Default)]
struct SurvivalState {
    death_ticks: u16,
    fire_ticks: u16,
}

fn tick_player_survival(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    mut player: Query<
        (
            &mut Transform,
            &mut PlayerHealth,
            &mut Velocity,
            &mut CollisionState,
            &mut Hotbar,
            &mut Inventory,
            &mut PlayerInterpolation,
        ),
        With<Player>,
    >,
    mut commands: Commands,
    mut rng: Local<crate::random::ItemRng>,
    mut survival: Local<SurvivalState>,
    mut persistence: Option<ResMut<WorldPersistence>>,
) {
    let Ok((
        mut transform,
        mut health,
        mut velocity,
        mut collision,
        mut hotbar,
        mut inventory,
        mut interpolation,
    )) = player.single_mut()
    else {
        return;
    };
    for step in 0..tick.ticks_this_frame() {
        if health.current == 0 {
            if survival.death_ticks == 0 {
                let cell = (transform.translation - Vec3::Y * EntitySize::PLAYER.y_offset)
                    .floor()
                    .as_ivec3();
                let Inventory {
                    main,
                    crafting,
                    armor,
                    carried,
                } = &mut *inventory;
                for slot in hotbar
                    .slots
                    .iter_mut()
                    .chain(main.iter_mut())
                    .chain(crafting.iter_mut())
                    .chain(armor.iter_mut())
                    .chain(std::iter::once(carried))
                {
                    if let Some(stack) = slot.take() {
                        crate::entity::drops::items::spawn_block_drop(
                            &mut commands,
                            &mut rng,
                            cell,
                            stack,
                        );
                    }
                }
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                }
                survival.death_ticks = 40;
                survival.fire_ticks = 0;
            } else {
                survival.death_ticks -= 1;
                if survival.death_ticks == 0 {
                    *transform = default_spawn_transform(&chunks);
                    interpolation.previous_position = transform.translation;
                    velocity.0 = Vec3::ZERO;
                    *collision = CollisionState::default();
                    health.current = MAX_PLAYER_HEALTH;
                }
            }
            continue;
        }
        let feet = transform.translation - Vec3::Y * EntitySize::PLAYER.y_offset;
        let (x, y, z) = (
            feet.x.floor() as i32,
            feet.y.floor() as i32,
            feet.z.floor() as i32,
        );
        let block = chunks
            .block_at(x, y, z)
            .unwrap_or(crate::block::id::Id::Air);
        if matches!(
            block,
            crate::block::id::Id::Water | crate::block::id::Id::FlowingWater
        ) {
            survival.fire_ticks = 0;
        } else if matches!(
            block,
            crate::block::id::Id::Fire
                | crate::block::id::Id::Lava
                | crate::block::id::Id::FlowingLava
        ) {
            survival.fire_ticks = if block == crate::block::id::Id::Fire {
                160
            } else {
                300
            };
        }
        if weather.as_ref().is_some_and(|w| w.is_raining())
            && (y..crate::world::chunk::CHUNK_HEIGHT as i32).all(|above| {
                !chunks
                    .block_at(x, above, z)
                    .is_some_and(crate::block::properties::is_opaque_cube)
            })
        {
            survival.fire_ticks = 0;
        }
        if survival.fire_ticks > 0 {
            survival.fire_ticks -= 1;
            if tick
                .world_time()
                .saturating_sub(u64::from(tick.ticks_this_frame() - step - 1))
                % 20
                == 0
            {
                health.current = health.current.saturating_sub(1);
            }
        }
    }
}

/// Horizontal movement speeds in blocks per second.
const WALK_SPEED: f32 = 4.317;
const SPRINT_SPEED: f32 = 5.612;
pub(crate) const SPRINT_ACCELERATION_MULTIPLIER: f32 = SPRINT_SPEED / WALK_SPEED;
const MOUSE_SENSITIVITY: f32 = 0.002;

/// Flying mode base speed in blocks per second.
const FLY_SPEED: f32 = 30.0;
const MIN_FLY_SPEED: f32 = 1.0;
const MAX_FLY_SPEED: f32 = 50.0;
const FLY_SPEED_STEP: f32 = 1.25;

fn spawn_player(
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
    let flying = saved.as_ref().map(|p| p.flying).unwrap_or(false);
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
        hotbar,
        inventory,
        CameraBobbing::default(),
        FlySpeed(fly_speed),
        interpolation,
        transform,
    ));
    if flying {
        entity.insert(Flying);
    }
    entity.with_children(|parent| {
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
        ),
        (With<Player>, Without<PlayerCamera>, Without<Flying>),
    >,
    mut cameras: Query<&mut Transform, With<PlayerCamera>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    for (transform, interpolation, velocity, collision, mut bob, children, combat) in &mut players {
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

fn default_spawn_transform(chunks: &WorldChunks) -> Transform {
    let surface = chunks
        .get(ChunkPosition::ZERO)
        .map_or(64.0, |generated| generated.heightmap.get(8, 8) as f32);
    let eye = surface + EntitySize::PLAYER.y_offset;
    Transform::from_xyz(8.5, eye, 8.5).looking_at(Vec3::new(8.5, eye, 16.5), Vec3::Y)
}

fn chat_controls_active(chat: Option<Res<crate::chat::ChatFocus>>) -> bool {
    chat.is_none_or(|chat| !chat.suppress_controls)
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
    inventory_screen: Res<crate::inventory::session::InventorySession>,
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

fn toggle_flying(
    keys: Res<ButtonInput<KeyCode>>,
    mut player: Query<(Entity, Option<&Flying>, &mut FlySpeed), With<Player>>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Ok((entity, flying, mut fly_speed)) = player.single_mut() else {
        return;
    };
    if flying.is_some() {
        commands.entity(entity).remove::<Flying>();
    } else {
        fly_speed.clamp_value();
        commands.entity(entity).insert(Flying);
    }
}

fn adjust_fly_speed(
    keys: Res<ButtonInput<KeyCode>>,
    mut player: Query<(&Flying, &mut FlySpeed), With<Player>>,
) {
    let Ok((_flying, mut fly_speed)) = player.single_mut() else {
        return;
    };
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        fly_speed.0 = (fly_speed.0 * FLY_SPEED_STEP).min(MAX_FLY_SPEED);
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        fly_speed.0 = (fly_speed.0 / FLY_SPEED_STEP).max(MIN_FLY_SPEED);
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
    keys: Res<ButtonInput<KeyCode>>,
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
        && chat_controls_active(chat);

    if flying.is_some() {
        // Flying mode: full 3D movement along the camera axes
        let mut direction = Vec3::ZERO;
        if locked {
            let forward = *transform.forward();
            let right = *transform.right();

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
            if keys.pressed(KeyCode::Space) {
                direction.y += 1.0;
            }
            if sneak_pressed(&keys) {
                direction.y -= 1.0;
            }
        }
        let speed = FLY_SPEED * fly_speed.0;
        velocity.0 = direction.normalize_or_zero() * speed;
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

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::app::state::PauseMenu;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::combat::tick_player_combat;
mod camera;
mod controls;
pub(crate) mod interaction;
pub(crate) mod model;
pub mod portal;
pub mod sleep;
mod state;
mod survival;

pub use camera::PlayerCamera;
pub use camera::draw_fov_scale;
pub use camera::hurt_roll_degrees;
pub(crate) use camera::rendered_eye;
pub(crate) use controls::SPRINT_ACCELERATION_MULTIPLIER;
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
pub use interaction::use_item::BowDraw;
pub use interaction::use_item::ItemUse;
pub use interaction::use_item::draw_power;
pub use model::arm::interpolated_swing;
pub use state::FlySpeed;
pub use state::GameMode;
pub use state::HeartFill;
pub use state::LocalPlayer;
pub use state::MAX_PLAYER_HEALTH;
pub use state::Player;
pub use state::PlayerHealth;
pub(crate) use state::PlayerInterpolation;
pub use state::PlayerMovementInput;
pub use state::PlayerName;
pub use survival::Bubble;
pub use survival::PlayerSurvival;
pub use survival::SurvivalPlugin;

use crate::physics::PhysicsSet;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::StoredPlayer;
use crate::world::persistence::WorldPersistence;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        interaction::overlay::overlay_plugin(app);
        model::arm::plugin(app);
        portal::plugin(app);
        interaction::use_item::plugin(app);
        app.init_resource::<PauseMenu>()
            .init_resource::<crate::inventory::session::InventorySession>()
            .init_resource::<crate::inventory::session::ActiveWorkbench>()
            .add_systems(
                PostStartup,
                spawn_player.run_if(crate::world::persistence::starts_with_world),
            )
            .add_systems(OnEnter(AppScreen::Playing), controls::capture_mouse)
            .add_systems(OnEnter(AppScreen::Menu), controls::release_mouse)
            .add_systems(OnEnter(AppScreen::Settings), controls::release_mouse)
            .add_systems(OnEnter(AppScreen::WorldSelect), controls::release_mouse)
            .add_systems(OnEnter(AppScreen::NewWorld), controls::release_mouse)
            .add_systems(Update, camera::apply_camera_fov)
            .add_systems(
                Update,
                (
                    controls::look_player.run_if(controls::chat_controls_active),
                    interaction::editing::interact_blocks.run_if(controls::chat_controls_active),
                    interaction::use_item::use_items,
                    interaction::use_item::draw_bow.run_if(controls::chat_controls_active),
                    controls::update_mouse_capture.run_if(controls::chat_controls_active),
                    controls::toggle_flying.run_if(controls::chat_controls_active),
                    controls::adjust_fly_speed.run_if(controls::chat_controls_active),
                    controls::apply_player_input,
                    controls::select_hotbar.run_if(controls::chat_controls_active),
                )
                    .chain()
                    .in_set(PhysicsSet::ApplyInput)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                camera::update_camera_bobbing
                    .after(PhysicsSet::Integrate)
                    // `hurt_pose` reads `hurt_time` with the new tick's
                    // `partial`; running first pairs them one tick apart and
                    // the roll steps back and forth on every tick.
                    .after(tick_player_combat)
                    // A rider's position is its vehicle's, written after the
                    // vehicle's tick; reading it first draws the view a tick
                    // behind on every frame that ticks.
                    .after(crate::entity::mount::snap_riders)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_plugins((survival::SurvivalPlugin, sleep::SleepPlugin));
    }
}

/// Spawn a player's simulated body from its saved record, or a new one at
/// `fallback`. This is every component the simulation reads; what a client
/// adds for its own player (camera, arm, [`LocalPlayer`]) is not here.
pub fn spawn_player_body(
    commands: &mut Commands,
    saved: Option<&StoredPlayer>,
    fallback: Transform,
) -> Entity {
    let transform = saved.map_or(fallback, StoredPlayer::to_transform);
    let (hotbar, inventory) = saved.map(StoredPlayer::to_inventory).unwrap_or_default();
    let game_mode = saved.map(|p| p.game_mode).unwrap_or_default();
    let flying = match game_mode {
        GameMode::Survival => false,
        GameMode::Creative => saved.is_some_and(|p| p.flying),
        GameMode::Spectator => true,
    };
    let fly_speed = saved.map_or(1.0, |p| p.fly_speed);
    let interpolation = PlayerInterpolation {
        previous_position: transform.translation,
    };
    let mut entity = commands.spawn((
        Name::new("Player"),
        Player,
        PlayerHealth {
            current: saved.map_or(MAX_PLAYER_HEALTH, |p| p.health.min(MAX_PLAYER_HEALTH)),
        },
        saved.map_or_else(PlayerSurvival::default, |p| {
            PlayerSurvival::restored(p.air, p.fire, p.fall_distance)
        }),
        hotbar,
        inventory,
        FlySpeed(fly_speed),
        game_mode,
        interpolation,
        transform,
        sleep::PlayerSleep::with_spawn(saved.and_then(|p| p.spawn).map(IVec3::from_array)),
    ));
    if flying {
        entity.insert(Flying);
    }
    entity.id()
}

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
    let body = spawn_player_body(
        &mut commands,
        saved.as_ref(),
        default_spawn_transform(&chunks),
    );
    let mut entity = commands.entity(body);
    entity.insert((LocalPlayer, camera::CameraBobbing::default()));
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

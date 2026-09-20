use bevy::asset::AssetPlugin;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::MeshPlugin;
use bevy::pbr::ScreenSpaceReflections;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use game::app::settings::DEFAULT_FOV;
use game::app::settings::GameSettings;
use game::app::settings::GraphicsQuality;
use game::app::settings::MAX_BRIGHTNESS;
use game::app::settings::MAX_FOV;
use game::app::settings::MAX_RENDER_DISTANCE;
use game::app::settings::MIN_BRIGHTNESS;
use game::app::settings::MIN_FOV;
use game::app::settings::MIN_RENDER_DISTANCE;
use game::app::state::AppScreen;
use game::player::Player;
use game::player::PlayerPlugin;
use game::world::chunk::WorldChunks;
use game::world::plugin::WorldPlugin;

#[test]
fn settings_controls_stay_within_their_ranges() {
    let mut settings = GameSettings::default();
    settings.change_render_distance(100);
    assert_eq!(settings.render_distance, MAX_RENDER_DISTANCE);
    settings.change_render_distance(-100);
    assert_eq!(settings.render_distance, MIN_RENDER_DISTANCE);

    settings.change_brightness(10_000.0);
    assert_eq!(settings.brightness, MAX_BRIGHTNESS);
    settings.change_brightness(-10_000.0);
    assert_eq!(settings.brightness, MIN_BRIGHTNESS);

    assert_eq!(settings.fov, DEFAULT_FOV);
    settings.change_fov(1_000.0);
    assert_eq!(settings.fov, MAX_FOV);
    settings.change_fov(-1_000.0);
    assert_eq!(settings.fov, MIN_FOV);

    assert_eq!(settings.graphics, GraphicsQuality::Fancy);
    assert!(settings.graphics.fancy_leaves());
    assert!(!settings.graphics.realistic_water());

    settings.cycle_graphics();
    assert_eq!(settings.graphics, GraphicsQuality::Ultra);
    assert!(settings.graphics.fancy_leaves());
    assert!(settings.graphics.realistic_water());

    settings.cycle_graphics();
    assert_eq!(settings.graphics, GraphicsQuality::Fast);
    assert!(!settings.graphics.fancy_leaves());
    assert!(!settings.graphics.realistic_water());

    settings.cycle_graphics();
    assert_eq!(settings.graphics, GraphicsQuality::Fancy);
}

#[test]
fn brightness_and_directional_toggle_update_bevy_lights() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins(WorldPlugin);
    app.update();

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.brightness = 500.0;
        settings.directional_lighting = false;
    }
    app.update();

    assert_eq!(
        app.world().resource::<GlobalAmbientLight>().brightness,
        500.0
    );
    let mut suns = app.world_mut().query::<&DirectionalLight>();
    let sun = suns.single(app.world()).unwrap();
    assert_eq!(sun.illuminance, 0.0);
    assert!(!sun.shadow_maps_enabled);
}

#[test]
fn fov_setting_updates_player_camera() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .init_asset::<Image>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldChunks>()
    .add_plugins(PlayerPlugin);
    app.update();

    assert!(
        (player_fov_radians(&mut app) - DEFAULT_FOV.to_radians()).abs() < f32::EPSILON,
        "spawned camera should use the default FOV"
    );

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.change_fov(20.0);
    }
    app.update();

    assert!((player_fov_radians(&mut app) - 90.0_f32.to_radians()).abs() < f32::EPSILON);
}

fn player_fov_radians(app: &mut App) -> f32 {
    let mut cameras = app
        .world_mut()
        .query_filtered::<&Projection, With<Player>>();
    match cameras.single(app.world()).unwrap() {
        Projection::Perspective(perspective) => perspective.fov,
        other => panic!("player camera should be perspective, got {other:?}"),
    }
}

#[test]
fn ultra_graphics_uses_ssr_water_without_changing_blend_on_fancy() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins(WorldPlugin);
    app.world_mut().spawn(Camera3d::default());
    app.update();

    let fancy_water = water_material(&app);
    assert_eq!(fancy_water.alpha_mode, AlphaMode::Blend);
    assert!((fancy_water.perceptual_roughness - 1.0).abs() < f32::EPSILON);
    assert_eq!(fancy_water.opaque_render_method, OpaqueRendererMethod::Auto);
    let mut ssr = app.world_mut().query::<&ScreenSpaceReflections>();
    assert!(ssr.iter(app.world()).next().is_none());

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.graphics = GraphicsQuality::Ultra;
    }
    app.update();

    let ultra_water = water_material(&app);
    assert_eq!(ultra_water.alpha_mode, AlphaMode::Opaque);
    assert!((ultra_water.perceptual_roughness - 0.09).abs() < f32::EPSILON);
    assert_eq!(
        ultra_water.opaque_render_method,
        OpaqueRendererMethod::Deferred
    );
    let mut ssr = app.world_mut().query::<&ScreenSpaceReflections>();
    assert!(ssr.single(app.world()).is_ok());

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.graphics = GraphicsQuality::Fancy;
    }
    app.update();

    let fancy_again = water_material(&app);
    assert_eq!(fancy_again.alpha_mode, AlphaMode::Blend);
    let mut ssr = app.world_mut().query::<&ScreenSpaceReflections>();
    assert!(ssr.iter(app.world()).next().is_none());
}

fn water_material(app: &App) -> StandardMaterial {
    app.world()
        .resource::<Assets<StandardMaterial>>()
        .iter()
        .map(|(_, material)| material.clone())
        .find(|material| material.cull_mode.is_none() && material.double_sided)
        .expect("water material should be double-sided with no cull")
}

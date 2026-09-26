use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::asset::AssetPlugin;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::MeshPlugin;
use bevy::pbr::ScreenSpaceReflections;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use game::app::settings::AMBIENT_ONLY_SCALE;
use game::app::settings::DEFAULT_FOV;
use game::app::settings::GameSettings;
use game::app::settings::GraphicsQuality;
use game::app::settings::MAX_BRIGHTNESS;
use game::app::settings::MAX_FOV;
use game::app::settings::MAX_RENDER_DISTANCE;
use game::app::settings::MIN_BRIGHTNESS;
use game::app::settings::MIN_FOV;
use game::app::settings::MIN_RENDER_DISTANCE;
use game::app::settings::SettingsPlugin;
use game::app::settings::load_settings;
use game::app::settings::save_settings;
use game::app::state::AppScreen;
use game::player::PlayerCamera;
use game::player::PlayerPlugin;
use game::ui::UiCameraPlugin;
use game::world::chunk::WorldChunks;
use game::world::plugin::WorldPlugin;
use game::world::textures::LeafCutoutMaterial;

fn temp_settings_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("game-settings-{label}-{unique}.json"))
}

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

    settings.brightness = MAX_BRIGHTNESS;
    settings.directional_lighting = true;
    assert_eq!(settings.ambient_light_brightness(), MAX_BRIGHTNESS);
    settings.directional_lighting = false;
    assert_eq!(
        settings.ambient_light_brightness(),
        MAX_BRIGHTNESS * AMBIENT_ONLY_SCALE
    );

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
        settings.old_lighting = false;
        settings.brightness = 500.0;
        settings.directional_lighting = false;
    }
    app.update();

    assert_eq!(
        app.world().resource::<GlobalAmbientLight>().brightness,
        500.0 * AMBIENT_ONLY_SCALE
    );
    let mut suns = app.world_mut().query::<&DirectionalLight>();
    let sun = suns.single(app.world()).unwrap();
    assert_eq!(sun.illuminance, 0.0);
    assert!(!sun.shadow_maps_enabled);

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.directional_lighting = true;
    }
    app.update();

    assert_eq!(
        app.world().resource::<GlobalAmbientLight>().brightness,
        500.0
    );
    let sun = suns.single(app.world()).unwrap();
    assert_eq!(sun.illuminance, 10_000.0);
    assert!(sun.shadow_maps_enabled);
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

    assert!((player_fov_radians(&mut app) - 100.0_f32.to_radians()).abs() < f32::EPSILON);
}

fn player_fov_radians(app: &mut App) -> f32 {
    let mut cameras = app
        .world_mut()
        .query_filtered::<&Projection, With<PlayerCamera>>();
    match cameras.single(app.world()).unwrap() {
        Projection::Perspective(perspective) => perspective.fov,
        other => panic!("player camera should be perspective, got {other:?}"),
    }
}

#[test]
fn first_person_arm_has_separate_camera_and_skin_mesh() {
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
    .add_plugins((PlayerPlugin, UiCameraPlugin));
    app.update();

    let mut cameras = app.world_mut().query::<(&Camera, &Projection)>();
    let camera_orders: Vec<_> = cameras
        .iter(app.world())
        .map(|(camera, _)| camera.order)
        .collect();
    assert_eq!(camera_orders.len(), 3);
    assert!(camera_orders.contains(&0));
    assert!(camera_orders.contains(&1));
    assert!(camera_orders.contains(&2));
    let mut ui_cameras = app
        .world_mut()
        .query_filtered::<&Camera, With<bevy::ui::IsDefaultUiCamera>>();
    let ui_camera = ui_cameras.single(app.world()).unwrap();
    assert_eq!(ui_camera.order, 2, "UI must render after the arm");

    let mut arms = app.world_mut().query::<(&Name, &Mesh3d, &Transform)>();
    let (_, arm_mesh, pose) = arms
        .iter(app.world())
        .find(|(name, _, _)| name.as_str() == "Right arm")
        .expect("first-person arm should spawn");
    let mesh = app
        .world()
        .resource::<Assets<Mesh>>()
        .get(&arm_mesh.0)
        .unwrap();
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
    let bevy::mesh::VertexAttributeValues::Float32x3(positions) = positions else {
        panic!("arm positions should be 3D floats");
    };
    assert_eq!(positions.len(), 24, "one textured cuboid with six faces");
    assert_eq!(mesh.count_vertices(), 24);
    let point = pose.transform_point(Vec3::from_array(positions[0]));
    assert!(point.is_finite());
    assert!(
        positions.iter().any(|position| {
            let point = pose.transform_point(Vec3::from_array(*position));
            point.z < -0.01 && point.x.abs() < -point.z && point.y.abs() < -point.z * 0.7
        }),
        "part of the resting arm should be inside the view"
    );
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

#[test]
fn fancy_leaves_mask_does_not_apply_to_solid_terrain() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins(WorldPlugin);
    app.update();

    let materials: Vec<_> = app
        .world()
        .resource::<Assets<StandardMaterial>>()
        .iter()
        .map(|(_, material)| material.clone())
        .collect();
    assert!(
        materials.iter().any(|material| {
            material.alpha_mode == AlphaMode::Opaque && material.cull_mode.is_some()
        }),
        "solid terrain should stay opaque so atlas edges are not discarded"
    );
    assert!(
        app.world()
            .resource::<Assets<LeafCutoutMaterial>>()
            .iter()
            .any(|(_, material)| matches!(material.base.alpha_mode, AlphaMode::Mask(_))),
        "fancy leaves should use a separate cutout material"
    );
}

fn water_material(app: &App) -> StandardMaterial {
    app.world()
        .resource::<Assets<StandardMaterial>>()
        .iter()
        .map(|(_, material)| material.clone())
        .find(|material| material.cull_mode.is_none() && material.double_sided)
        .expect("water material should be double-sided with no cull")
}

#[test]
fn missing_settings_file_uses_defaults() {
    let path = temp_settings_path("missing");
    assert_eq!(load_settings(&path), GameSettings::default());
}

#[test]
fn settings_round_trip_through_json() {
    let path = temp_settings_path("roundtrip");
    let settings = GameSettings {
        render_distance: 12,
        brightness: 450.0,
        fov: 90.0,
        old_lighting: true,
        smooth_lighting: true,
        directional_lighting: false,
        wiggle_leaves: false,
        graphics: GraphicsQuality::Ultra,
    };
    save_settings(&path, &settings).unwrap();
    assert_eq!(load_settings(&path), settings);
    let _ = fs::remove_file(path);
}

#[test]
fn settings_json_fills_in_missing_menu_fields() {
    let path = temp_settings_path("partial");
    fs::write(&path, r#"{ "render_distance": 16 }"#).unwrap();
    let loaded = load_settings(&path);
    assert_eq!(loaded.render_distance, 16);
    assert_eq!(loaded.brightness, GameSettings::default().brightness);
    assert_eq!(loaded.fov, DEFAULT_FOV);
    assert_eq!(loaded.old_lighting, true);
    assert_eq!(loaded.directional_lighting, true);
    assert_eq!(loaded.graphics, GraphicsQuality::Fancy);
    let _ = fs::remove_file(path);
}

#[test]
fn settings_json_clamps_out_of_range_values() {
    let path = temp_settings_path("clamp");
    fs::write(
        &path,
        r#"{
            "render_distance": 99,
            "brightness": -50.0,
            "fov": 180.0,
            "old_lighting": true,
            "directional_lighting": false,
            "graphics": "Fast"
        }"#,
    )
    .unwrap();
    let loaded = load_settings(&path);
    assert_eq!(loaded.render_distance, MAX_RENDER_DISTANCE);
    assert_eq!(loaded.brightness, MIN_BRIGHTNESS);
    assert_eq!(loaded.fov, MAX_FOV);
    assert!(loaded.old_lighting);
    assert!(!loaded.directional_lighting);
    assert_eq!(loaded.graphics, GraphicsQuality::Fast);
    let _ = fs::remove_file(path);
}

#[test]
fn unreadable_settings_json_falls_back_to_defaults() {
    let path = temp_settings_path("corrupt");
    fs::write(&path, "not json").unwrap();
    assert_eq!(load_settings(&path), GameSettings::default());
    let _ = fs::remove_file(path);
}

#[test]
fn settings_plugin_loads_and_saves_menu_changes() {
    let path = temp_settings_path("plugin");
    let initial = GameSettings {
        render_distance: 8,
        brightness: 200.0,
        fov: 55.0,
        old_lighting: true,
        smooth_lighting: true,
        directional_lighting: false,
        wiggle_leaves: true,
        graphics: GraphicsQuality::Fast,
    };
    save_settings(&path, &initial).unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(SettingsPlugin::new(path.clone()));
    app.update();
    assert_eq!(app.world().resource::<GameSettings>(), &initial);
    assert!(fs::read_to_string(&path).unwrap().contains("\"Fast\""));

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.change_render_distance(1);
        settings.change_fov(10.0);
        settings.cycle_graphics();
        settings.old_lighting = false;
    }
    app.update();

    let saved = load_settings(&path);
    assert_eq!(saved.render_distance, 9);
    assert_eq!(saved.graphics, GraphicsQuality::Fancy);
    assert!(!saved.old_lighting);
    assert_eq!(saved.brightness, 200.0);
    assert_eq!(saved.fov, 65.0);
    let _ = fs::remove_file(path);
}

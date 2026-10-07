use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::asset::AssetPlugin;
use bevy::camera::Exposure;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::MeshPlugin;
use bevy::pbr::ScreenSpaceReflections;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use game::app::settings::DEFAULT_CLOUD_HEIGHT;
use game::app::settings::DEFAULT_FOV;
use game::app::settings::Difficulty;
use game::app::settings::GameSettings;
use game::app::settings::GraphicsQuality;
use game::app::settings::MAX_CLOUD_HEIGHT;
use game::app::settings::MAX_FOV;
use game::app::settings::MAX_RENDER_DISTANCE;
use game::app::settings::MIN_CLOUD_HEIGHT;
use game::app::settings::MIN_FOV;
use game::app::settings::MIN_RENDER_DISTANCE;
use game::app::settings::SettingsPlugin;
use game::app::settings::load_settings;
use game::app::settings::save_settings;
use game::app::state::AppScreen;
use game::player::PlayerCamera;
use game::player::PlayerPlugin;
use game::rendering::meshing::WATER_ALPHA;
use game::rendering::textures::BlockMaterial;
use game::ui::UiCameraPlugin;
use game::world::chunk::WorldChunks;
use game::world::environment::celestial_angle;
use game::world::environment::skylight_subtracted;
use game::world::plugin::WorldPlugin;
use game::world::streaming::WorldStreaming;
use game::world::tick::WorldTick;

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

    assert_eq!(settings.fov, DEFAULT_FOV);
    settings.change_fov(1_000.0);
    assert_eq!(settings.fov, MAX_FOV);
    settings.change_fov(-1_000.0);
    assert_eq!(settings.fov, MIN_FOV);

    assert_eq!(settings.cloud_height, DEFAULT_CLOUD_HEIGHT);
    settings.change_cloud_height(8.0);
    assert_eq!(settings.cloud_height, DEFAULT_CLOUD_HEIGHT + 8.0);
    settings.change_cloud_height(1_000.0);
    assert_eq!(settings.cloud_height, MAX_CLOUD_HEIGHT);
    settings.change_cloud_height(-1_000.0);
    assert_eq!(settings.cloud_height, MIN_CLOUD_HEIGHT);

    assert_eq!(settings.graphics, GraphicsQuality::Fancy);
    assert!(settings.graphics.fancy_leaves());
    assert!(settings.graphics.entity_shadows());

    settings.cycle_graphics();
    assert_eq!(settings.graphics, GraphicsQuality::Fast);
    assert!(!settings.graphics.fancy_leaves());
    assert!(!settings.graphics.entity_shadows());

    settings.cycle_graphics();
    assert_eq!(settings.graphics, GraphicsQuality::Fancy);
}

#[test]
fn beta_lighting_keeps_ambient_zero_and_spawns_no_analytic_light() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app.update();

    app.update();

    assert_eq!(app.world().resource::<GlobalAmbientLight>().brightness, 0.0);
    let mut directional = app.world_mut().query::<&DirectionalLight>();
    assert!(directional.iter(app.world()).next().is_none());
    let mut points = app.world_mut().query::<&PointLight>();
    assert!(points.iter(app.world()).next().is_none());
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

    assert!(
        (player_fov_radians(&mut app) - (DEFAULT_FOV + 20.0).to_radians()).abs() < f32::EPSILON
    );
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
fn both_graphics_modes_keep_beta_water_and_disable_clusters() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    let camera = app.world_mut().spawn(Camera3d::default()).id();
    for graphics in [GraphicsQuality::Fancy, GraphicsQuality::Fast] {
        app.world_mut().resource_mut::<GameSettings>().graphics = graphics;
        app.update();
        app.update();
        let water = water_material(&app);
        assert_eq!(water.alpha_mode, AlphaMode::Blend);
        assert!(water.unlit);
        assert_eq!(water.opaque_render_method, OpaqueRendererMethod::Auto);
        assert!(matches!(
            app.world()
                .get::<bevy::light::cluster::ClusterConfig>(camera),
            Some(bevy::light::cluster::ClusterConfig::None)
        ));
        assert!(app.world().get::<ScreenSpaceReflections>(camera).is_none());
        assert!(app.world().get::<bevy::camera::Hdr>(camera).is_none());
    }
}

#[test]
fn anti_aliasing_and_shadow_map_sizes_follow_the_settings() {
    use bevy::light::DirectionalLightShadowMap;
    use bevy::light::PointLightShadowMap;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app.world_mut().spawn(Camera3d::default());
    app.world_mut().spawn(Camera2d);
    app.update();
    app.update();

    fn samples(app: &mut App) -> Vec<Msaa> {
        let mut cameras = app.world_mut().query_filtered::<&Msaa, With<Camera>>();
        cameras.iter(app.world()).copied().collect()
    }
    assert_eq!(samples(&mut app), [Msaa::Sample4; 2]);
    // Nothing is a directional or point light, so both maps stay unused.
    assert!(app.world().resource::<DirectionalLightShadowMap>().size < 64);
    assert!(app.world().resource::<PointLightShadowMap>().size < 64);

    app.world_mut().resource_mut::<GameSettings>().anti_aliasing = false;
    app.update();
    app.update();
    assert_eq!(samples(&mut app), [Msaa::Off; 2]);

    // A camera that appears later, such as a loaded world's, follows too.
    app.world_mut().spawn(Camera3d::default());
    app.update();
    app.update();
    assert_eq!(samples(&mut app), [Msaa::Off; 3]);
    let mut cameras = app
        .world_mut()
        .query_filtered::<&bevy::light::cluster::ClusterConfig, With<Camera3d>>();
    assert_eq!(cameras.iter(app.world()).count(), 2);
    assert!(
        cameras
            .iter(app.world())
            .all(|config| matches!(config, bevy::light::cluster::ClusterConfig::None))
    );

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.anti_aliasing = true;
    }
    app.update();
    app.update();
    assert_eq!(samples(&mut app), [Msaa::Sample4; 3]);
    assert!(app.world().resource::<DirectionalLightShadowMap>().size < 64);
    assert!(app.world().resource::<PointLightShadowMap>().size < 64);
}

#[test]
fn fancy_leaves_mask_does_not_apply_to_solid_terrain() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app.update();

    let materials = block_materials(&app);
    assert!(
        materials.iter().any(|material| {
            material.base.alpha_mode == AlphaMode::Opaque && material.base.cull_mode.is_some()
        }),
        "solid terrain should stay opaque so atlas edges are not discarded"
    );
    assert!(
        materials.iter().any(|material| {
            matches!(material.base.alpha_mode, AlphaMode::Mask(_))
                && material.extension.settings.wiggle_amplitude > 0.0
        }),
        "fancy leaves should use a separate, wiggling cutout material"
    );
    assert_eq!(
        materials
            .iter()
            .filter(|material| material.extension.settings.wiggle_amplitude > 0.0)
            .count(),
        1,
        "only leaves wiggle"
    );
}

#[test]
fn lighting_settings_and_dusk_update_block_uniforms_without_remeshing() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app.update();
    for material in block_materials(&app) {
        let lighting = material.extension.settings.lighting();
        assert!(lighting.smooth_lighting);
        assert_eq!(lighting.skylight_subtracted, 0);
    }

    {
        let mut settings = app.world_mut().resource_mut::<GameSettings>();
        settings.smooth_lighting = false;
    }
    let dusk = 13_700;
    app.world_mut()
        .resource_mut::<WorldTick>()
        .set_world_time(dusk);
    app.update();

    let expected = skylight_subtracted(celestial_angle(dusk, 0.0));
    assert!(expected > 0);
    for material in block_materials(&app) {
        let lighting = material.extension.settings.lighting();
        assert!(!lighting.smooth_lighting);
        assert!(material.base.unlit);
        assert_eq!(lighting.skylight_subtracted, expected);
    }
    // Lighting is a material uniform, so nothing was queued for rebuilding.
    let streaming = app.world().resource::<WorldStreaming>();
    assert_eq!(streaming.queued_remesh_positions().count(), 0);
}

fn block_materials(app: &App) -> Vec<BlockMaterial> {
    app.world()
        .resource::<Assets<BlockMaterial>>()
        .iter()
        .map(|(_, material)| material.clone())
        .collect()
}

/// Water is the only block material with a translucent base color.
fn water_material(app: &App) -> StandardMaterial {
    let water = block_materials(app)
        .into_iter()
        .map(|material| material.base)
        .find(|material| (material.base_color.alpha() - WATER_ALPHA).abs() < 1e-5)
        .expect("water material should carry the water alpha");
    assert!(water.cull_mode.is_none() && water.double_sided);
    water
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
        max_fps: 120,
        fov: 90.0,
        cloud_height: 192.0,

        smooth_lighting: true,
        wiggle_leaves: false,
        graphics: GraphicsQuality::Fancy,
        anti_aliasing: false,
        mouse_sensitivity: 1.5,
        view_bobbing: false,
        fullscreen: true,
        difficulty: Difficulty::Hard,
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
    assert_eq!(loaded.difficulty, Difficulty::Normal);
    assert_eq!(loaded.fov, DEFAULT_FOV);
    assert_eq!(loaded.cloud_height, DEFAULT_CLOUD_HEIGHT);
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
            "cloud_height": 9999.0,
            "old_lighting": true,
            "graphics": "Fast"
        }"#,
    )
    .unwrap();
    let loaded = load_settings(&path);
    assert_eq!(loaded.render_distance, MAX_RENDER_DISTANCE);
    assert_eq!(loaded.fov, MAX_FOV);
    assert_eq!(loaded.cloud_height, MAX_CLOUD_HEIGHT);
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
        max_fps: 90,
        fov: 55.0,
        cloud_height: 64.0,

        smooth_lighting: true,
        wiggle_leaves: true,
        graphics: GraphicsQuality::Fast,
        ..default()
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
        settings.change_cloud_height(8.0);
        settings.cycle_graphics();
    }
    app.update();
    // The write waits for the options to settle, so a dragged slider is one
    // write; quitting does not wait.
    assert_eq!(load_settings(&path), initial);
    app.world_mut().write_message(AppExit::Success);
    app.update();

    let saved = load_settings(&path);
    assert_eq!(saved.render_distance, 9);
    assert_eq!(saved.graphics, GraphicsQuality::Fancy);
    assert_eq!(saved.fov, 65.0);
    assert_eq!(saved.cloud_height, 72.0);
    let _ = fs::remove_file(path);
}

#[test]
fn atmosphere_does_not_mark_unchanged_camera_projections_changed() {
    #[derive(Resource, Default)]
    struct ProjectionChanges(usize);
    fn observe(query: Query<(), Changed<Projection>>, mut changes: ResMut<ProjectionChanges>) {
        changes.0 = query.iter().count();
    }
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin))
        .init_resource::<ProjectionChanges>()
        .add_systems(Last, observe);
    app.world_mut().spawn((
        PlayerCamera,
        Camera3d::default(),
        Projection::default(),
        Transform::default(),
    ));
    app.update();
    app.update();
    app.update();
    assert_eq!(app.world().resource::<ProjectionChanges>().0, 0);
    app.world_mut()
        .resource_mut::<GameSettings>()
        .render_distance = 8;
    app.update();
    assert!(app.world().resource::<ProjectionChanges>().0 > 0);
    app.update();
    assert_eq!(app.world().resource::<ProjectionChanges>().0, 0);
}

#[test]
fn frame_limit_defaults_round_trips_and_clamps() {
    let path = temp_settings_path("frame-limit");
    fs::write(&path, "{}").unwrap();
    assert_eq!(load_settings(&path).max_fps, 60);
    for (stored, expected) in [(0, 0), (1, 30), (120, 120), (1000, 240)] {
        fs::write(&path, format!("{{\"max_fps\":{stored}}}")).unwrap();
        let settings = load_settings(&path);
        assert_eq!(settings.max_fps, expected);
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).max_fps, expected);
    }
    let _ = fs::remove_file(path);
    let mut settings = GameSettings::default();
    for expected in [90, 120, 144, 240, 0, 30, 60] {
        settings.cycle_max_fps();
        assert_eq!(settings.max_fps, expected);
    }
}

#[test]
fn frame_pacing_changes_with_screen_and_selected_limit() {
    use bevy::winit::UpdateMode;
    use bevy::winit::WinitSettings;
    use game::app::frame_pacing::FramePacingPlugin;
    use std::time::Duration;

    fn wait(mode: &UpdateMode) -> Duration {
        match mode {
            UpdateMode::Reactive { wait, .. } => *wait,
            UpdateMode::Continuous => panic!("expected paced mode"),
        }
    }
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .init_state::<AppScreen>()
        .init_resource::<GameSettings>()
        .add_plugins(FramePacingPlugin);
    app.update();
    let pacing = app.world().resource::<WinitSettings>();
    assert_eq!(
        wait(&pacing.focused_mode),
        Duration::from_secs_f64(1.0 / 30.0)
    );
    assert_eq!(
        wait(&pacing.unfocused_mode),
        Duration::from_secs_f64(1.0 / 5.0)
    );
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    let pacing = app.world().resource::<WinitSettings>();
    assert_eq!(
        wait(&pacing.focused_mode),
        Duration::from_secs_f64(1.0 / 60.0)
    );
    assert_eq!(
        wait(&pacing.unfocused_mode),
        Duration::from_secs_f64(1.0 / 20.0)
    );
    assert!(matches!(
        pacing.focused_mode,
        UpdateMode::Reactive {
            react_to_device_events: false,
            react_to_user_events: false,
            react_to_window_events: false,
            ..
        }
    ));
    app.world_mut().resource_mut::<GameSettings>().max_fps = 120;
    app.update();
    assert_eq!(
        wait(&app.world().resource::<WinitSettings>().focused_mode),
        Duration::from_secs_f64(1.0 / 120.0)
    );
    app.world_mut().resource_mut::<GameSettings>().max_fps = 0;
    app.update();
    assert!(matches!(
        app.world().resource::<WinitSettings>().focused_mode,
        UpdateMode::Continuous
    ));
}

#[test]
fn opaque_menu_disables_world_cameras_and_playing_restores_them() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
    ))
    .init_state::<AppScreen>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    let camera = app
        .world_mut()
        .spawn((PlayerCamera, Camera3d::default(), Transform::default()))
        .id();
    // Chunks hide with the camera: a GPU-culled layer that stayed visible
    // would not be drawn again by the reactivated camera.
    let chunk_root = |app: &mut App, x| {
        app.world_mut()
            .spawn((
                game::world::chunk::ChunkPosition { x, z: 0 },
                Visibility::default(),
            ))
            .id()
    };
    let chunk = chunk_root(&mut app, 0);
    app.update();
    assert!(!app.world().get::<Camera>(camera).unwrap().is_active);
    assert_eq!(
        app.world().get::<Visibility>(chunk),
        Some(&Visibility::Hidden)
    );
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    assert!(app.world().get::<Camera>(camera).unwrap().is_active);
    assert_eq!(
        app.world().get::<Visibility>(chunk),
        Some(&Visibility::Inherited)
    );
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Settings);
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(chunk),
        Some(&Visibility::Hidden)
    );
    // A chunk that finishes behind the menu is hidden too.
    let late = chunk_root(&mut app, 1);
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(late),
        Some(&Visibility::Hidden)
    );
    let mut cameras = app.world_mut().query_filtered::<&Camera, With<Camera3d>>();
    assert!(cameras.iter(app.world()).all(|camera| !camera.is_active));
}

#[test]
fn unreadable_settings_json_is_kept_as_a_backup() {
    let path = temp_settings_path("corrupt-backup");
    fs::write(&path, "not json").unwrap();
    load_settings(&path);
    let backup = game::app::settings::backup_path(&path);
    assert_eq!(fs::read_to_string(&backup).unwrap(), "not json");
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(backup);
}

#[test]
fn one_unreadable_field_does_not_reset_the_other_settings() {
    let path = temp_settings_path("one-bad-field");
    fs::write(
        &path,
        r#"{"graphics":"Cinematic","fov":95,"render_distance":"far","view_bobbing":false,"from_a_newer_build":[1,2]}"#,
    )
    .unwrap();
    let loaded = load_settings(&path);
    assert_eq!(loaded.graphics, GameSettings::default().graphics);
    assert_eq!(
        loaded.render_distance,
        GameSettings::default().render_distance
    );
    assert_eq!(loaded.fov, 95.0);
    assert!(!loaded.view_bobbing);
    let _ = fs::remove_file(path);
}

#[test]
fn settings_plugin_saves_after_the_options_settle_and_keeps_the_players_difficulty() {
    use game::app::settings::ClientDifficulty;
    use game::app::settings::Difficulty;
    use game::app::settings::SAVE_DELAY_SECONDS;

    let path = temp_settings_path("debounce");
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(SettingsPlugin::new(path.clone()));
    app.update();

    // A loaded world's difficulty stands in for the player's own option.
    app.world_mut().resource_mut::<ClientDifficulty>().0 = Some(Difficulty::Easy);
    app.world_mut().resource_mut::<GameSettings>().difficulty = Difficulty::Peaceful;
    app.world_mut().resource_mut::<GameSettings>().fov = 100.0;
    app.update();
    assert!(!path.exists(), "the write waits for the options to settle");

    std::thread::sleep(std::time::Duration::from_secs_f32(SAVE_DELAY_SECONDS + 0.1));
    app.update();
    let saved = load_settings(&path);
    assert_eq!(saved.fov, 100.0);
    assert_eq!(saved.difficulty, Difficulty::Easy);
    let _ = fs::remove_file(path);
}

#[test]
fn player_options_default_clamp_and_load_from_older_files() {
    let mut settings = GameSettings::default();
    assert_eq!(settings.mouse_sensitivity, 1.0);
    assert!(settings.view_bobbing);
    assert!(!settings.fullscreen);
    settings.change_mouse_sensitivity(100.0);
    assert_eq!(settings.mouse_sensitivity, 3.0);
    settings.change_mouse_sensitivity(-100.0);
    assert_eq!(settings.mouse_sensitivity, 0.1);
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        settings.mouse_sensitivity = invalid;
        settings.clamp();
        assert_eq!(settings.mouse_sensitivity, 1.0);
    }
    let path = temp_settings_path("old-player-options");
    fs::write(&path, r#"{"fov":90}"#).unwrap();
    let loaded = load_settings(&path);
    assert_eq!(loaded.mouse_sensitivity, 1.0);
    assert!(loaded.view_bobbing);
    assert!(!loaded.fullscreen);
    fs::remove_file(path).unwrap();
}

fn settings_menu_app() -> App {
    settings_menu_app_with_assets(format!("{}/assets", env!("CARGO_MANIFEST_DIR")))
}

fn settings_menu_app_with_assets(file_path: String) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path,
            ..default()
        },
        bevy::image::ImagePlugin::default_nearest(),
        StatesPlugin,
        bevy::input::InputPlugin,
    ))
    .register_asset_loader(bevy::image::ImageLoader::new(
        bevy::image::CompressedImageFormats::NONE,
    ))
    .init_asset::<Font>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .add_message::<AppExit>()
    .add_plugins(game::ui::MenuPlugin);
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Settings);
    app.update();
    app
}

fn button_named(app: &mut App, title: &str) -> Entity {
    let mut texts = app.world_mut().query::<(&Text, &ChildOf)>();
    texts
        .iter(app.world())
        .find(|(text, parent)| {
            text.0 == title && app.world().get::<Button>(parent.parent()).is_some()
        })
        .unwrap_or_else(|| panic!("missing button {title}"))
        .1
        .parent()
}

fn click_menu_button(app: &mut App, title: &str) {
    let button = button_named(app, title);
    app.world_mut()
        .entity_mut(button)
        .insert(Interaction::Pressed);
    app.update();
    app.world_mut().entity_mut(button).insert(Interaction::None);
    app.update();
}

#[test]
fn main_menu_places_title_logo_halves_side_by_side() {
    let mut app = settings_menu_app();
    click_menu_button(&mut app, "Back");
    let logo_handle = app
        .world()
        .resource::<AssetServer>()
        .load::<Image>("title/mclogo.png");
    let mut logos = app.world_mut().query::<(&ImageNode, &Node, &ChildOf)>();
    let halves: Vec<_> = logos
        .iter(app.world())
        .filter(|(image, _, _)| image.image == logo_handle)
        .collect();
    assert_eq!(halves.len(), 2);
    assert_eq!(halves[0].0.rect, Some(Rect::new(0.0, 0.0, 155.0, 44.0)));
    assert_eq!(halves[1].0.rect, Some(Rect::new(0.0, 45.0, 119.0, 89.0)));
    assert_eq!(halves[0].1.width, px(310));
    assert_eq!(halves[1].1.width, px(238));
    assert_eq!(halves[0].1.height, px(88));
    assert_eq!(halves[1].1.height, px(88));
    assert_eq!(halves[0].2.parent(), halves[1].2.parent());
    assert_eq!(
        app.world()
            .get::<Node>(halves[0].2.parent())
            .unwrap()
            .flex_direction,
        FlexDirection::Row
    );
}

#[test]
fn main_menu_panorama_assembles_six_faces_and_stops_outside_menu() {
    use bevy::light::Skybox;
    use bevy::render::render_resource::TextureViewDimension;

    let mut app = settings_menu_app();
    click_menu_button(&mut app, "Back");
    let cubemap = (0..300)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
            app.world_mut()
                .query::<&Skybox>()
                .iter(app.world())
                .find_map(|skybox| skybox.image.clone())
        })
        .expect("the title panorama should load");
    let images = app.world().resource::<Assets<Image>>();
    let cube = images.get(&cubemap).unwrap();
    assert_eq!(cube.texture_descriptor.size.depth_or_array_layers, 6);
    assert_eq!(cube.width(), 256);
    assert_eq!(cube.height(), 256);
    assert_eq!(
        cube.texture_view_descriptor.as_ref().unwrap().dimension,
        Some(TextureViewDimension::Cube)
    );
    let bevy::image::ImageSampler::Descriptor(sampler) = &cube.sampler else {
        panic!("the panorama should use a linear sampler, unlike block textures");
    };
    assert_eq!(sampler.mag_filter, bevy::image::ImageFilterMode::Linear);
    assert_eq!(sampler.min_filter, bevy::image::ImageFilterMode::Linear);
    assert_eq!(sampler.lod_max_clamp, 0.0);
    let layer_size = 256 * 256 * 4;
    let bytes = cube.data.as_ref().unwrap();
    let mut changed = 0;
    let weights = [1_i32, 4, 6, 4, 1];
    for (layer, face) in [1, 3, 4, 5, 0, 2].into_iter().enumerate() {
        let handle = app
            .world()
            .resource::<AssetServer>()
            .load::<Image>(format!("title/bg/panorama{face}.png"));
        let source = images.get(&handle).unwrap().data.as_ref().unwrap();
        let layer_bytes = &bytes[layer * layer_size..(layer + 1) * layer_size];
        // Independently sample a 5x5 Gaussian from the original face. The
        // production blur rounds after each pass, so allow 1 level of error.
        for (x, y) in [(0_i32, 0_i32), (128, 128), (255, 255), (64, 100), (200, 10)] {
            for channel in 0..3 {
                let sample = |x: i32, y: i32| -> i32 {
                    let x = x.clamp(0, 255) as usize;
                    let y = y.clamp(0, 255) as usize;
                    let y = if face == 4 || face == 5 { 255 - y } else { y };
                    i32::from(source[(y * 256 + x) * 4 + channel])
                };
                let weighted: i32 = (-2..=2)
                    .flat_map(|dy| (-2..=2).map(move |dx| (dy, dx)))
                    .map(|(dy, dx)| {
                        weights[(dy + 2) as usize]
                            * weights[(dx + 2) as usize]
                            * sample(x + dx, y + dy)
                    })
                    .sum();
                let actual = i32::from(layer_bytes[(y as usize * 256 + x as usize) * 4 + channel]);
                assert!((actual - ((weighted + 128) / 256)).abs() <= 1);
                changed += usize::from(actual != sample(x, y));
            }
        }
    }
    assert!(changed > 0, "blur must soften some original pixels");
    assert!(
        app.world_mut()
            .query::<(&Skybox, &Camera, &bevy::camera::visibility::RenderLayers)>()
            .iter(app.world())
            .any(|(skybox, camera, layers)| skybox.image.is_some()
                && camera.is_active
                && skybox.brightness * Exposure::default().exposure() > 0.5
                && *layers == bevy::camera::visibility::RenderLayers::layer(4))
    );
    // The tiled fallback is replaced with a translucent tint once the panorama is ready.
    assert!(
        app.world_mut()
            .query::<(&Node, &BackgroundColor, Option<&ImageNode>)>()
            .iter(app.world())
            .any(|(node, color, image)| node.row_gap == px(14)
                && image.is_none()
                && color.0.alpha() < 1.0)
    );
    click_menu_button(&mut app, "Settings");
    assert!(
        app.world_mut()
            .query::<(&Skybox, &Camera)>()
            .iter(app.world())
            .all(|(_, camera)| !camera.is_active)
    );
    click_menu_button(&mut app, "Back");
    assert!(
        app.world_mut()
            .query::<(&Skybox, &Camera)>()
            .iter(app.world())
            .all(|(_, camera)| camera.is_active)
    );
    click_menu_button(&mut app, "Play");
    assert!(
        app.world_mut()
            .query::<(&Skybox, &Camera)>()
            .iter(app.world())
            .all(|(_, camera)| !camera.is_active)
    );
}

#[test]
fn missing_panorama_preserves_main_menu_background() {
    use bevy::light::Skybox;

    let mut app = settings_menu_app_with_assets(
        temp_settings_path("no-panorama")
            .to_string_lossy()
            .into_owned(),
    );
    click_menu_button(&mut app, "Back");
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(1));
        app.update();
    }
    assert!(
        app.world_mut()
            .query::<(&Skybox, &Camera)>()
            .iter(app.world())
            .all(|(skybox, camera)| skybox.image.is_none() && !camera.is_active)
    );
    assert!(
        app.world_mut()
            .query::<(&Node, &BackgroundColor)>()
            .iter(app.world())
            .any(|(node, color)| node.row_gap == px(14) && color.0.alpha() == 1.0)
    );
    button_named(&mut app, "Play");
}

#[test]
fn settings_tabs_buttons_and_live_labels() {
    let mut app = settings_menu_app();
    let mut texts = app.world_mut().query::<&Text>();
    assert!(
        texts
            .iter(app.world())
            .all(|text| !text.0.starts_with("Old lighting:")
                && !text.0.starts_with("Ambient brightness:"))
    );
    for title in [
        "Graphics: Fancy",
        "Smooth lighting: ON",
        "Wiggle leaves: ON",
        "Anti-aliasing: 4x",
        "Max FPS: 60",
        "Fullscreen: OFF",
        "View bobbing: ON",
    ] {
        button_named(&mut app, title);
    }
    let mut texts = app.world_mut().query::<(&Text, &ChildOf)>();
    let sensitivity_row = texts
        .iter(app.world())
        .find(|(text, _)| text.0 == "Mouse sensitivity: 100%")
        .unwrap()
        .1
        .parent();
    let controls_panel = app
        .world()
        .get::<ChildOf>(sensitivity_row)
        .unwrap()
        .parent();
    assert_eq!(
        app.world().get::<Node>(controls_panel).unwrap().display,
        Display::None
    );
    click_menu_button(&mut app, "Controls");
    assert_eq!(
        app.world().get::<Node>(controls_panel).unwrap().display,
        Display::Grid
    );
    click_menu_button(&mut app, "View bobbing: ON");
    assert!(!app.world().resource::<GameSettings>().view_bobbing);
    button_named(&mut app, "View bobbing: OFF");
    use game::ui::slider::Slider;
    use game::ui::slider::SliderDrag;
    let node = app.world().get::<Node>(sensitivity_row).unwrap();
    assert_eq!(node.width, percent(100));
    assert_eq!(node.height, px(40));
    assert_eq!(node.max_width, px(520));
    assert!(app.world().get::<ImageNode>(sensitivity_row).is_some());
    app.world_mut()
        .get_mut::<Slider>(sensitivity_row)
        .unwrap()
        .set_fraction(100.0 / 290.0);
    app.update();
    assert!((app.world().resource::<GameSettings>().mouse_sensitivity - 1.1).abs() < 0.0001);
    button_named(&mut app, "Mouse sensitivity: 110%");
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut().resource_mut::<SliderDrag>().0 = Some(sensitivity_row);
    click_menu_button(&mut app, "Video");
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    click_menu_button(&mut app, "Fullscreen: OFF");
    assert!(app.world().resource::<GameSettings>().fullscreen);
    assert_eq!(
        app.world().get::<Node>(controls_panel).unwrap().display,
        Display::None
    );
}

#[test]
fn settings_resize_scroll_and_tab_reset() {
    use bevy::input::mouse::MouseScrollUnit;
    use bevy::input::mouse::MouseWheel;
    use bevy::window::PrimaryWindow;
    let mut app = settings_menu_app();
    let window = app
        .world_mut()
        .spawn((
            PrimaryWindow,
            Window {
                resolution: (1280, 720).into(),
                ..default()
            },
        ))
        .id();
    app.update();
    let mut nodes = app.world_mut().query::<&Node>();
    assert_eq!(
        nodes
            .iter(app.world())
            .filter(|node| node.grid_template_columns == vec![RepeatedGridTrack::flex(2, 1.0)])
            .count(),
        3
    );
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .resolution
        .set(640.0, 360.0);
    app.update();
    assert_eq!(
        nodes
            .iter(app.world())
            .filter(|node| node.grid_template_columns == vec![RepeatedGridTrack::flex(1, 1.0)])
            .count(),
        3
    );
    let content = app
        .world_mut()
        .query::<(Entity, &Node)>()
        .iter(app.world())
        .find(|(_, node)| node.overflow.y == OverflowAxis::Scroll)
        .unwrap()
        .0;
    app.world_mut().entity_mut(content).insert(ComputedNode {
        size: Vec2::new(600.0, 180.0),
        content_size: Vec2::new(600.0, 540.0),
        inverse_scale_factor: 1.0,
        ..default()
    });
    app.world_mut().write_message(MouseWheel {
        unit: MouseScrollUnit::Line,
        phase: bevy::input::touch::TouchPhase::Moved,
        x: 0.0,
        y: -100.0,
        window,
    });
    app.update();
    assert_eq!(app.world().get::<ScrollPosition>(content).unwrap().y, 360.0);
    click_menu_button(&mut app, "Controls");
    assert_eq!(app.world().get::<ScrollPosition>(content).unwrap().y, 0.0);
    click_menu_button(&mut app, "Back");
    click_menu_button(&mut app, "Settings");
    // Reopening settings always starts on Video.
    let controls = button_named(&mut app, "View bobbing: ON");
    let panel = app.world().get::<ChildOf>(controls).unwrap().parent();
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );
}

#[test]
fn slider_thumb_tracks_value_and_leaving_settings_cancels_drag() {
    use game::ui::slider::Slider;
    use game::ui::slider::SliderDrag;
    use game::ui::slider::SliderThumb;
    let mut app = settings_menu_app();
    let slider = button_named(&mut app, "Render distance: 4 chunks");
    app.world_mut()
        .get_mut::<ComputedNode>(slider)
        .unwrap()
        .size = Vec2::new(400.0, 88.0);
    app.world_mut()
        .get_mut::<ComputedNode>(slider)
        .unwrap()
        .inverse_scale_factor = 0.5;
    app.world_mut()
        .get_mut::<Slider>(slider)
        .unwrap()
        .set_fraction(1.0);
    app.update();
    let thumb = app
        .world_mut()
        .query::<(Entity, &SliderThumb)>()
        .iter(app.world())
        .find(|(_, thumb)| thumb.0 == slider)
        .unwrap()
        .0;
    assert_eq!(app.world().get::<Node>(thumb).unwrap().left, px(184.0));
    let button = button_named(&mut app, "Graphics: Fancy");
    let image = app.world().get::<ImageNode>(thumb).unwrap();
    assert_eq!(
        image.image,
        app.world().get::<ImageNode>(button).unwrap().image
    );
    assert_eq!(image.rect, Some(Rect::new(0.0, 66.0, 200.0, 86.0)));
    app.world_mut()
        .get_mut::<Interaction>(slider)
        .unwrap()
        .clone_from(&Interaction::Hovered);
    app.update();
    assert_eq!(
        app.world().get::<ImageNode>(thumb).unwrap().rect,
        Some(Rect::new(0.0, 86.0, 200.0, 106.0))
    );
    button_named(&mut app, "Render distance: 32 chunks");
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut().resource_mut::<SliderDrag>().0 = Some(slider);
    click_menu_button(&mut app, "Back");
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    assert!(app.world().get_entity(slider).is_err());
}

#[test]
fn missing_reference_art_keeps_settings_labels_and_buttons_usable() {
    let mut app =
        settings_menu_app_with_assets(temp_settings_path("no-art").to_string_lossy().into_owned());
    let fullscreen = button_named(&mut app, "Fullscreen: OFF");
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(1));
        app.update();
        if app.world().get::<ImageNode>(fullscreen).is_none() {
            break;
        }
    }
    assert!(app.world().get::<ImageNode>(fullscreen).is_none());
    assert!(app.world().get::<BackgroundColor>(fullscreen).is_some());
    let text = app.world().get::<Children>(fullscreen).unwrap()[0];
    assert_eq!(
        app.world().get::<TextFont>(text).unwrap().font,
        TextFont::default().font
    );
    click_menu_button(&mut app, "Fullscreen: OFF");
    assert!(app.world().resource::<GameSettings>().fullscreen);
    button_named(&mut app, "Fullscreen: ON");
}

#[test]
fn slider_snaps_clamps_and_suppresses_unchanged_values() {
    use game::ui::slider::Slider;
    let mut slider = Slider::new(10.0, 300.0, 1.0, 100.0);
    assert!(!slider.set_fraction(slider.fraction()));
    assert!(slider.set_fraction(-1.0));
    assert_eq!(slider.value(), 10.0);
    assert!(!slider.set_fraction(f32::NAN));
    assert!(slider.set_fraction(2.0));
    assert_eq!(slider.value(), 300.0);
    slider.set_fraction(0.5);
    assert_eq!(slider.value(), 155.0);
    assert_eq!(slider.fraction(), 0.5);
}

#[test]
fn setting_sliders_cover_ranges_and_fps_vsync_endpoint() {
    use game::ui::screens::menu::SettingsSlider;
    let mut settings = GameSettings::default();
    for binding in [
        SettingsSlider::RenderDistance,
        SettingsSlider::Fov,
        SettingsSlider::CloudHeight,
        SettingsSlider::MouseSensitivity,
        SettingsSlider::MaxFps,
    ] {
        let mut slider = binding.slider(&settings);
        assert!(!binding.apply(&slider, &mut settings));
        slider.set_fraction(0.0);
        binding.apply(&slider, &mut settings);
        assert_eq!(binding.slider(&settings).fraction(), 0.0);
        slider.set_fraction(1.0);
        binding.apply(&slider, &mut settings);
        assert_eq!(binding.slider(&settings).fraction(), 1.0);
    }
    assert_eq!(settings.max_fps, 0);
    let mut slider = SettingsSlider::MaxFps.slider(&settings);
    slider.set_fraction(210.0 / 211.0);
    SettingsSlider::MaxFps.apply(&slider, &mut settings);
    assert_eq!(settings.max_fps, 240);
    slider.set_fraction(0.0);
    SettingsSlider::MaxFps.apply(&slider, &mut settings);
    assert_eq!(settings.max_fps, 30);
}

#[test]
fn slider_drag_respects_clipping_release_focus_and_cancellation() {
    use bevy::ui::RelativeCursorPosition;
    use bevy::window::PrimaryWindow;
    use game::ui::slider::Slider;
    use game::ui::slider::SliderDrag;
    use game::ui::slider::update_sliders;
    let mut app = App::new();
    app.init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<SliderDrag>()
        .add_systems(Update, update_sliders);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let entity = app
        .world_mut()
        .spawn((
            Slider::new(0.0, 100.0, 1.0, 0.0),
            Interaction::Pressed,
            RelativeCursorPosition {
                cursor_over: false,
                normalized: Some(Vec2::ZERO),
            },
            ComputedNode {
                size: Vec2::new(200.0, 44.0),
                inverse_scale_factor: 1.0,
                ..default()
            },
            BackgroundColor::default(),
        ))
        .id();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    app.world_mut()
        .get_mut::<RelativeCursorPosition>(entity)
        .unwrap()
        .cursor_over = true;
    app.update();
    assert_eq!(app.world().resource::<SliderDrag>().0, Some(entity));
    assert_eq!(app.world().get::<Slider>(entity).unwrap().value(), 50.0);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    *app.world_mut()
        .get_mut::<RelativeCursorPosition>(entity)
        .unwrap() = RelativeCursorPosition {
        cursor_over: false,
        normalized: Some(Vec2::new(2.0, 0.0)),
    };
    app.update();
    assert_eq!(app.world().get::<Slider>(entity).unwrap().value(), 100.0);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    app.world_mut().resource_mut::<SliderDrag>().0 = Some(entity);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .get_mut::<RelativeCursorPosition>(entity)
        .unwrap()
        .cursor_over = true;
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    app.update();
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    app.world_mut()
        .get_mut::<RelativeCursorPosition>(entity)
        .unwrap()
        .cursor_over = false;
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    app.world_mut().resource_mut::<SliderDrag>().0 = None;
    app.update();
    assert!(app.world().resource::<SliderDrag>().0.is_none());
    app.world_mut().resource_mut::<SliderDrag>().0 = Some(entity);
    app.world_mut().despawn(entity);
    app.update();
    assert!(app.world().resource::<SliderDrag>().0.is_none());
}

/// An app in `Playing` with the menu and pause plugins. Keyboard input is a
/// bare resource so a test can press Escape without the input plugin clearing
/// it first.
fn pause_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
            ..default()
        },
        bevy::image::ImagePlugin::default_nearest(),
        StatesPlugin,
    ))
    .register_asset_loader(bevy::image::ImageLoader::new(
        bevy::image::CompressedImageFormats::NONE,
    ))
    .init_asset::<Font>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<ButtonInput<KeyCode>>()
    .init_resource::<ButtonInput<MouseButton>>()
    .add_message::<bevy::input::mouse::MouseWheel>()
    .add_message::<AppExit>()
    .add_plugins((game::ui::MenuPlugin, game::ui::PauseMenuPlugin));
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    app
}

fn press_escape(app: &mut App) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release(KeyCode::Escape);
    keys.clear();
    app.update();
}

fn pause_open(app: &App) -> bool {
    app.world().resource::<game::app::state::PauseMenu>().open
}

fn screen(app: &App) -> AppScreen {
    *app.world().resource::<State<AppScreen>>().get()
}

#[test]
fn escape_opens_and_closes_the_pause_menu() {
    let mut app = pause_app();
    assert!(!pause_open(&app));

    press_escape(&mut app);
    assert!(pause_open(&app));
    assert_eq!(screen(&app), AppScreen::Playing);
    for title in ["Resume", "Settings", "Quit to title"] {
        button_named(&mut app, title);
    }

    press_escape(&mut app);
    assert!(!pause_open(&app));
    let mut roots = app.world_mut().query::<&Button>();
    assert_eq!(roots.iter(app.world()).count(), 0);
}

#[test]
fn escape_does_not_open_the_pause_menu_over_the_inventory_or_chat() {
    let mut app = pause_app();
    app.world_mut()
        .resource_mut::<game::inventory::session::InventorySession>()
        .open = true;
    press_escape(&mut app);
    assert!(!pause_open(&app));

    app.world_mut()
        .resource_mut::<game::inventory::session::InventorySession>()
        .open = false;
    app.insert_resource(game::chat::ChatFocus {
        open: true,
        suppress_controls: true,
    });
    press_escape(&mut app);
    assert!(!pause_open(&app));
}

#[test]
fn resume_closes_the_pause_menu() {
    let mut app = pause_app();
    press_escape(&mut app);
    click_menu_button(&mut app, "Resume");
    assert!(!pause_open(&app));
    assert_eq!(screen(&app), AppScreen::Playing);
}

#[test]
fn settings_from_the_pause_menu_returns_to_the_game_with_it_still_open() {
    let mut app = pause_app();
    press_escape(&mut app);
    click_menu_button(&mut app, "Settings");
    app.update();
    assert_eq!(screen(&app), AppScreen::Settings);

    click_menu_button(&mut app, "Back");
    app.update();
    assert_eq!(screen(&app), AppScreen::Playing);
    assert!(pause_open(&app));
    button_named(&mut app, "Resume");
}

#[test]
fn quit_to_title_leaves_the_game_and_closes_the_pause_menu() {
    let mut app = pause_app();
    press_escape(&mut app);
    click_menu_button(&mut app, "Quit to title");
    app.update();
    assert_eq!(screen(&app), AppScreen::Menu);
    assert!(!pause_open(&app));
    button_named(&mut app, "Play");
}

fn world_screen_app() -> App {
    let mut app = settings_menu_app();
    click_menu_button(&mut app, "Back");
    app
}

#[test]
fn play_opens_the_world_list_and_back_returns_to_the_title() {
    let mut app = world_screen_app();
    assert_eq!(screen(&app), AppScreen::Menu);

    click_menu_button(&mut app, "Play");
    app.update();
    assert_eq!(screen(&app), AppScreen::WorldSelect);
    button_named(&mut app, "Create New World");

    click_menu_button(&mut app, "Back");
    app.update();
    assert_eq!(screen(&app), AppScreen::Menu);
    button_named(&mut app, "Play");
}

#[test]
fn new_world_screen_goes_back_to_the_world_list() {
    let mut app = world_screen_app();
    click_menu_button(&mut app, "Play");
    app.update();
    click_menu_button(&mut app, "Create New World");
    app.update();
    assert_eq!(screen(&app), AppScreen::NewWorld);
    for title in ["Difficulty: Normal", "Back"] {
        button_named(&mut app, title);
    }

    click_menu_button(&mut app, "Back");
    app.update();
    assert_eq!(screen(&app), AppScreen::WorldSelect);
}

fn type_text(app: &mut App, key_code: KeyCode, text: Option<&str>) {
    app.world_mut()
        .write_message(bevy::input::keyboard::KeyboardInput {
            key_code,
            logical_key: bevy::input::keyboard::Key::Unidentified(
                bevy::input::keyboard::NativeKey::Unidentified,
            ),
            state: bevy::input::ButtonState::Pressed,
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    app.update();
}

#[test]
fn new_world_form_collects_name_seed_and_difficulty() {
    use game::app::session::WorldChoice;
    use game::app::session::WorldSession;
    use game::app::settings::Difficulty;

    let mut app = world_screen_app();
    click_menu_button(&mut app, "Play");
    app.update();
    click_menu_button(&mut app, "Create New World");
    app.update();

    for _ in 0.."New World".len() {
        type_text(&mut app, KeyCode::Backspace, None);
    }
    type_text(&mut app, KeyCode::KeyA, Some("A"));
    type_text(&mut app, KeyCode::KeyB, Some("B"));
    type_text(&mut app, KeyCode::Tab, None);
    type_text(&mut app, KeyCode::Digit4, Some("4"));
    type_text(&mut app, KeyCode::Digit2, Some("2"));
    app.update();
    button_named(&mut app, "World name: AB");
    button_named(&mut app, "Seed: 42_");

    click_menu_button(&mut app, "Difficulty: Normal");
    button_named(&mut app, "Difficulty: Hard");
    button_named(&mut app, "Save format: Native");
    click_menu_button(&mut app, "Save format: Native");
    button_named(&mut app, "Save format: Beta 1.7.3");
    click_menu_button(&mut app, "Create New World");

    let session = app.world().resource::<WorldSession>();
    match session.pending_choice() {
        Some(WorldChoice::New {
            name,
            seed,
            difficulty,
            format,
        }) => {
            assert_eq!(name, "AB");
            assert_eq!(*seed, 42);
            assert_eq!(*difficulty, Difficulty::Hard);
            assert_eq!(*format, game::world::persistence::SaveFormat::Original);
        }
        other => panic!("expected a new world request, got {other:?}"),
    }
}

#[test]
fn an_empty_name_and_seed_fall_back_to_a_default_name_and_random_seed() {
    use game::app::session::WorldChoice;
    use game::app::session::WorldSession;

    let mut app = world_screen_app();
    click_menu_button(&mut app, "Play");
    app.update();
    click_menu_button(&mut app, "Create New World");
    app.update();
    for _ in 0.."New World".len() {
        type_text(&mut app, KeyCode::Backspace, None);
    }
    click_menu_button(&mut app, "Create New World");

    let session = app.world().resource::<WorldSession>();
    let Some(WorldChoice::New { name, .. }) = session.pending_choice() else {
        panic!("expected a new world request");
    };
    assert_eq!(name, "New World");
}

fn right_click(app: &mut App, row: Entity) {
    app.world_mut().entity_mut(row).insert(Interaction::Hovered);
    app.world_mut()
        .write_message(bevy::input::mouse::MouseButtonInput {
            button: MouseButton::Right,
            state: bevy::input::ButtonState::Pressed,
            window: Entity::PLACEHOLDER,
        });
    app.world_mut()
        .write_message(bevy::input::mouse::MouseButtonInput {
            button: MouseButton::Right,
            state: bevy::input::ButtonState::Released,
            window: Entity::PLACEHOLDER,
        });
    // The screen rebuilds around the delete controls, replacing this row.
    app.update();
    app.update();
}

#[test]
fn right_clicking_a_world_offers_delete_with_a_confirmation() {
    use game::world::persistence::PersistencePlugin;
    use game::world::persistence::WorldStorage;

    let saves = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "delete-worlds-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let keep = WorldStorage::create(&saves, 1, "Keep Me").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let doomed = WorldStorage::create(&saves, 2, "Doomed").unwrap();

    let mut app = world_screen_app();
    app.add_plugins(PersistencePlugin::new(saves.clone()).deferred());
    click_menu_button(&mut app, "Play");
    app.update();
    let row = button_named(&mut app, "Doomed");

    // Without a right click there is nothing to delete.
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .all(|text| text.0 != "Delete World")
    );

    right_click(&mut app, row);
    button_named(&mut app, "Delete World");

    // Return backs out without deleting anything.
    click_menu_button(&mut app, "Delete World");
    app.update();
    button_named(&mut app, "Confirm");
    click_menu_button(&mut app, "Return");
    app.update();
    assert!(doomed.root().exists());
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .all(|text| text.0 != "Confirm" && text.0 != "Delete World")
    );

    let row = button_named(&mut app, "Doomed");
    right_click(&mut app, row);
    click_menu_button(&mut app, "Delete World");
    app.update();
    click_menu_button(&mut app, "Confirm");
    app.update();
    app.update();

    assert!(!doomed.root().exists(), "the confirmed world is removed");
    assert!(keep.root().exists(), "other worlds are untouched");
    let mut texts = app.world_mut().query::<&Text>();
    assert!(texts.iter(app.world()).all(|text| text.0 != "Doomed"));
    assert!(texts.iter(app.world()).any(|text| text.0 == "Keep Me"));
    let _ = std::fs::remove_dir_all(&saves);
}

#[test]
fn legacy_lighting_and_ultra_settings_migrate_without_losing_other_options() {
    let path = temp_settings_path("legacy-lighting");
    fs::write(&path, r#"{"old_lighting":false,"brightness":500,"graphics":"Ultra","smooth_lighting":false,"render_distance":12,"anti_aliasing":true}"#).unwrap();
    let settings = load_settings(&path);
    assert_eq!(settings.graphics, GraphicsQuality::Fancy);
    assert!(!settings.smooth_lighting);
    assert_eq!(settings.render_distance, 12);
    assert_eq!(settings.msaa(), Msaa::Sample4);
    save_settings(&path, &settings).unwrap();
    let saved = fs::read_to_string(&path).unwrap();
    assert!(!saved.contains("old_lighting"));
    assert!(!saved.contains("brightness"));
    assert!(!saved.contains("Ultra"));
    assert_eq!(load_settings(&path), settings);
    fs::remove_file(path).unwrap();
}

#[test]
fn beta_material_extensions_do_not_use_shadows_or_prepasses() {
    use bevy::pbr::Material;
    use game::rendering::creatures::CreatureMaterial;
    use game::rendering::textures::TintedMaterial;
    assert!(!BlockMaterial::enable_prepass());
    assert!(!BlockMaterial::enable_shadows());
    assert!(!CreatureMaterial::enable_prepass());
    assert!(!CreatureMaterial::enable_shadows());
    assert!(!TintedMaterial::enable_prepass());
    assert!(!TintedMaterial::enable_shadows());
}

#[test]
fn renderer_disables_gpu_light_clustering_without_changing_capabilities() {
    use bevy::light::cluster::GlobalClusterGpuSettings;
    use bevy::light::cluster::GlobalClusterSettings;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .insert_resource(GlobalClusterSettings {
            supports_storage_buffers: true,
            clustered_decals_are_usable: false,
            gpu_clustering: Some(GlobalClusterGpuSettings {
                initial_z_slice_list_capacity: 64,
                initial_index_list_capacity: 64,
            }),
            max_uniform_buffer_clusterable_objects: 256,
            view_cluster_bindings_max_indices: 16384,
        })
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    let camera = app.world_mut().spawn(Camera3d::default()).id();
    app.update();
    let config = app.world().resource::<GlobalClusterSettings>();
    assert!(config.gpu_clustering.is_none());
    assert!(config.supports_storage_buffers);
    assert_eq!(
        app.world()
            .get::<bevy::core_pipeline::tonemapping::Tonemapping>(camera),
        Some(&bevy::core_pipeline::tonemapping::Tonemapping::None)
    );
}

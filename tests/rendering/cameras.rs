use bevy::camera::CameraOutputMode;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::mesh::MeshPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use game::player::PlayerCamera;
use game::rendering::textures::BACKDROP_SORT;
use game::rendering::textures::SkyMaterial;
use game::world::plugin::WorldPlugin;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app
}

/// Whether a camera copies the frame to its target: `Some(true)` replacing
/// it, `Some(false)` skipping, `None` for Bevy's default blend.
fn writes(app: &App, camera: Entity) -> Option<bool> {
    match app.world().get::<Camera>(camera)?.output_mode {
        CameraOutputMode::Skip => Some(false),
        CameraOutputMode::Write {
            blend_state: Some(BlendState::REPLACE),
            ..
        } => Some(true),
        CameraOutputMode::Write { .. } => None,
    }
}

#[test]
fn only_the_last_camera_on_the_window_copies_the_frame_to_it() {
    let mut app = app();
    let first = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            Camera {
                order: -1,
                ..default()
            },
        ))
        .id();
    let world = app.world_mut().spawn(Camera3d::default()).id();
    let ui = app
        .world_mut()
        .spawn((
            Camera2d,
            Camera {
                order: 2,
                ..default()
            },
        ))
        .id();
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::default());
    let offscreen = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            Camera {
                order: 5,
                ..default()
            },
            RenderTarget::from(image),
        ))
        .id();
    app.update();
    assert_eq!(writes(&app, first), Some(false));
    assert_eq!(writes(&app, world), Some(false));
    assert_eq!(writes(&app, ui), Some(true));
    // A camera drawing into an image is not on the window.
    assert_eq!(writes(&app, offscreen), None);

    // With the last camera off, the one under it copies the frame.
    app.world_mut().get_mut::<Camera>(ui).unwrap().is_active = false;
    app.update();
    assert_eq!(writes(&app, world), Some(true));
    assert_eq!(writes(&app, ui), Some(false));
    assert_eq!(writes(&app, first), Some(false));
}

#[test]
fn the_world_camera_draws_the_sky_itself() {
    let mut app = app();
    let camera = app
        .world_mut()
        .spawn((PlayerCamera, Camera3d::default(), Transform::default()))
        .id();
    app.update();
    app.update();

    // No backdrop cameras: the world camera clears to the sky color.
    let mut cameras = app.world_mut().query_filtered::<Entity, With<Camera>>();
    assert_eq!(cameras.iter(app.world()).collect::<Vec<_>>(), [camera]);
    assert!(matches!(
        app.world().get::<Camera>(camera).unwrap().clear_color,
        ClearColorConfig::Custom(_)
    ));

    // The sunrise fan, sun, moon, and stars are on the world camera's
    // layer, and sort ahead of everything else blended.
    let mut bodies = app
        .world_mut()
        .query::<(&MeshMaterial3d<SkyMaterial>, Option<&RenderLayers>)>();
    let bodies: Vec<_> = bodies
        .iter(app.world())
        .map(|(material, layers)| (material.0.clone(), layers.is_some()))
        .collect();
    assert_eq!(bodies.len(), 4);
    let materials = app.world().resource::<Assets<SkyMaterial>>();
    for (material, layered) in bodies {
        assert!(!layered);
        assert_eq!(
            materials.get(&material).unwrap().base.depth_bias,
            BACKDROP_SORT
        );
    }
}

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use game::app::state::AppScreen;
use game::block::id::Id;
use game::chat::ChatFocus;
use game::rendering::textures::BlockMaterial;
use game::rendering::textures::MeshWireframe;
use game::rendering::textures::MeshWireframePlugin;
use game::rendering::textures::configure_mesh_wireframe;

/// Keeps the test material's strong handle alive. Dropping it makes asset
/// tracking remove the material before the toggle system can see it.
#[derive(Resource)]
struct KeptMaterial(Handle<BlockMaterial>);

fn test_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_state::<AppScreen>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(MeshWireframePlugin);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<BlockMaterial>>()
        .add(BlockMaterial::default());
    app.insert_resource(KeptMaterial(handle));
    app
}

fn press(app: &mut App, key: KeyCode) {
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset(key);
}

fn wireframe(app: &App) -> bool {
    let KeptMaterial(handle) = app.world().resource::<KeptMaterial>();
    app.world()
        .resource::<Assets<BlockMaterial>>()
        .get(handle)
        .is_some_and(|material| material.extension.wireframe)
}

fn enter_playing(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
}

#[test]
fn f4_toggles_block_meshes_between_filled_and_wireframe() {
    let mut app = test_app();
    assert!(!app.world().resource::<MeshWireframe>().enabled);
    assert!(!wireframe(&app));

    // The menu is the default screen. F4 there must leave the meshes filled.
    press(&mut app, KeyCode::F4);
    assert!(!wireframe(&app));

    enter_playing(&mut app);
    press(&mut app, KeyCode::F4);
    assert!(app.world().resource::<MeshWireframe>().enabled);
    assert!(wireframe(&app));

    press(&mut app, KeyCode::F4);
    assert!(!app.world().resource::<MeshWireframe>().enabled);
    assert!(!wireframe(&app));
}

#[test]
fn f4_does_not_toggle_wireframe_while_chat_has_focus() {
    let mut app = test_app();
    enter_playing(&mut app);
    app.insert_resource(ChatFocus::default());
    app.world_mut()
        .resource_mut::<ChatFocus>()
        .suppress_controls = true;

    press(&mut app, KeyCode::F4);
    assert!(!wireframe(&app));

    app.world_mut()
        .resource_mut::<ChatFocus>()
        .suppress_controls = false;
    press(&mut app, KeyCode::F4);
    assert!(wireframe(&app));
}

fn apply(
    app: &mut App,
    enabled: bool,
    block: Option<Id>,
    supported: bool,
) -> Result<String, &'static str> {
    app.world_mut()
        .resource_scope(|world, mut materials: Mut<Assets<BlockMaterial>>| {
            let mut mode = world.resource_mut::<MeshWireframe>();
            configure_mesh_wireframe(&mut mode, &mut materials, enabled, block, supported)
        })
}

#[test]
fn wireframe_commands_update_the_material_and_block_filter() {
    let mut app = test_app();
    assert_eq!(apply(&mut app, true, None, true).unwrap(), "Wireframe on");
    assert!(app.world().resource::<MeshWireframe>().enabled);
    assert_eq!(app.world().resource::<MeshWireframe>().block, None);
    assert!(wireframe(&app));

    assert_eq!(
        apply(&mut app, true, Some(Id::Water), true).unwrap(),
        "Wireframe showing Water (9)"
    );
    assert_eq!(
        app.world().resource::<MeshWireframe>().block,
        Some(Id::Water)
    );
    assert!(wireframe(&app));

    let filtered = *app.world().resource::<MeshWireframe>();
    assert!(apply(&mut app, false, None, false).is_err());
    assert_eq!(*app.world().resource::<MeshWireframe>(), filtered);
    assert!(wireframe(&app));

    assert_eq!(apply(&mut app, false, None, true).unwrap(), "Wireframe off");
    assert!(!app.world().resource::<MeshWireframe>().enabled);
    assert_eq!(app.world().resource::<MeshWireframe>().block, None);
    assert!(!wireframe(&app));
}

#[test]
fn f4_clears_a_block_filter_and_restores_filled_meshes() {
    let mut app = test_app();
    enter_playing(&mut app);
    apply(&mut app, true, Some(Id::Grass), true).unwrap();
    assert_eq!(
        app.world().resource::<MeshWireframe>().block,
        Some(Id::Grass)
    );

    press(&mut app, KeyCode::F4);
    assert!(!app.world().resource::<MeshWireframe>().enabled);
    assert_eq!(app.world().resource::<MeshWireframe>().block, None);
    assert!(!wireframe(&app));
}

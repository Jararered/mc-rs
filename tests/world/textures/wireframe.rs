use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use game::app::state::AppScreen;
use game::ui::screens::chat::ChatState;
use game::world::textures::BlockMaterial;
use game::world::textures::MeshWireframe;
use game::world::textures::MeshWireframePlugin;

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
    app.insert_resource(ChatState::default());
    app.world_mut()
        .resource_mut::<ChatState>()
        .suppress_controls = true;

    press(&mut app, KeyCode::F4);
    assert!(!wireframe(&app));

    app.world_mut()
        .resource_mut::<ChatState>()
        .suppress_controls = false;
    press(&mut app, KeyCode::F4);
    assert!(wireframe(&app));
}

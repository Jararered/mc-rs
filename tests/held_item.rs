use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;
use game::app::settings::GameSettings;
use game::app::state::AppScreen;
use game::inventory::Hotbar;
use game::item::ItemId;
use game::item::ItemStack;
use game::player::Player;
use game::player::PlayerPlugin;
use game::ui::HudPlugin;
use game::ui::InventoryGuiPlugin;
use game::world::chunk::WorldChunks;
use game::world::textures::atlas_tile_uvs;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        MeshPlugin,
        StatesPlugin,
        bevy::input::InputPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .init_asset::<Image>()
    .init_asset::<Font>()
    .init_state::<AppScreen>()
    .init_resource::<GameSettings>()
    .init_resource::<WorldChunks>()
    .add_plugins((PlayerPlugin, InventoryGuiPlugin, HudPlugin));
    app.update();
    app.world_mut()
        .resource_mut::<NextState<AppScreen>>()
        .set(AppScreen::Playing);
    app.update();
    app
}

fn visual(app: &mut App, name: &str) -> (Visibility, Handle<Mesh>, Transform) {
    let mut query = app
        .world_mut()
        .query::<(&Name, &Visibility, &Mesh3d, &Transform)>();
    query
        .iter(app.world())
        .find(|(n, _, _, _)| n.as_str() == name)
        .map(|(_, v, m, t)| (*v, m.0.clone(), *t))
        .unwrap()
}

fn select(app: &mut App, id: u16, data: u16) {
    let mut query = app
        .world_mut()
        .query_filtered::<&mut Hotbar, With<Player>>();
    let mut bar = query.single_mut(app.world_mut()).unwrap();
    bar.slots[0] = Some(ItemStack::with_data(ItemId(id), 1, data).unwrap());
}

fn vertices(app: &App, handle: &Handle<Mesh>) -> usize {
    app.world()
        .resource::<Assets<Mesh>>()
        .get(handle)
        .unwrap()
        .count_vertices()
}

#[test]
fn empty_arm_swaps_to_modeled_and_flat_blocks() {
    let mut app = app();
    assert_eq!(visual(&mut app, "Right arm").0, Visibility::Visible);
    assert_eq!(visual(&mut app, "Held stack").0, Visibility::Hidden);
    select(&mut app, 53, 0); // stairs: two boxes
    for _ in 0..7 {
        app.update();
    }
    assert_eq!(visual(&mut app, "Right arm").0, Visibility::Hidden);
    let (visible, mesh, _) = visual(&mut app, "Held stack");
    assert_eq!(visible, Visibility::Visible);
    assert_eq!(vertices(&app, &mesh), 48);
    select(&mut app, 50, 0); // torch: extruded terrain sprite
    for _ in 0..7 {
        app.update();
    }
    let (_, mesh, _) = visual(&mut app, "Held stack");
    assert_eq!(vertices(&app, &mesh), 264);
}

#[test]
fn item_sprite_has_beta_pixel_depth_and_subtype_uvs() {
    let mut app = app();
    select(&mut app, 351, 1); // dye subtype
    for _ in 0..7 {
        app.update();
    }
    let (_, handle, _) = visual(&mut app, "Held stack");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).unwrap();
    assert_eq!(mesh.count_vertices(), 264);
    let VertexAttributeValues::Float32x3(positions) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        positions.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min),
        -1.0 / 16.0
    );
    assert_eq!(
        positions
            .iter()
            .map(|p| p[2])
            .fold(f32::NEG_INFINITY, f32::max),
        0.0
    );
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    assert!(
        uvs.iter()
            .all(|uv| uv.iter().all(|n| (0.0..=1.0).contains(n)))
    );
    assert!(uvs[0][0] > 14.0 / 16.0 && uvs[0][1] > 1.0 / 16.0);
    assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
}

#[test]
fn held_block_uvs_use_the_padded_terrain_atlas() {
    let mut app = app();
    select(&mut app, 1, 0); // stone uses terrain tile (1, 0)
    for _ in 0..7 {
        app.update();
    }
    let (_, handle, _) = visual(&mut app, "Held stack");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).unwrap();
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(1, 0);
    assert!(
        uvs.iter()
            .all(|[u, v]| *u >= u0 && *u <= u1 && *v >= v0 && *v <= v1)
    );

    select(&mut app, 50, 0); // torch sprite uses terrain tile (0, 5)
    for _ in 0..7 {
        app.update();
    }
    let (_, handle, _) = visual(&mut app, "Held stack");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).unwrap();
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    let (u0, v0, u1, v1) = atlas_tile_uvs(0, 5);
    assert!(
        uvs.iter()
            .all(|[u, v]| *u >= u0 && *u <= u1 && *v >= v0 && *v <= v1)
    );
}

#[test]
fn count_and_durability_updates_keep_the_held_mesh_and_pose() {
    let mut app = app();
    select(&mut app, 346, 0); // fishing rod
    for _ in 0..7 {
        app.update();
    }
    let (_, mesh, before) = visual(&mut app, "Held stack");
    select(&mut app, 346, 1);
    app.update();
    let (_, after_mesh, after) = visual(&mut app, "Held stack");
    assert_eq!(mesh, after_mesh);
    assert!((before.translation.y - after.translation.y).abs() < 0.001);
}

#[test]
fn selection_lowers_old_visual_before_swapping_and_raising() {
    let mut app = app();
    select(&mut app, 1, 0);
    for _ in 0..7 {
        app.update();
    }
    let (_, old_mesh, rest) = visual(&mut app, "Held stack");
    select(&mut app, 4, 0);
    app.update();
    let (_, current_mesh, lowered) = visual(&mut app, "Held stack");
    assert_eq!(current_mesh, old_mesh);
    assert!(lowered.translation.y < rest.translation.y);
    app.update();
    assert_eq!(visual(&mut app, "Held stack").1, old_mesh);
    app.update();
    let (_, replacement, bottom) = visual(&mut app, "Held stack");
    assert_ne!(replacement, old_mesh);
    assert!(bottom.translation.y < lowered.translation.y);
    for _ in 0..4 {
        app.update();
    }
    let (_, final_mesh, raised) = visual(&mut app, "Held stack");
    assert_eq!(final_mesh, replacement);
    assert!(raised.translation.y > bottom.translation.y);
}

#[test]
fn click_swing_moves_the_arm_and_held_item() {
    let mut app = app();
    app.world_mut().spawn((
        Window::default(),
        CursorOptions {
            grab_mode: CursorGrabMode::Locked,
            ..default()
        },
        PrimaryWindow,
    ));
    let arm_rest = visual(&mut app, "Right arm").2;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.update();
    let arm_swing = visual(&mut app, "Right arm").2;
    assert_ne!(arm_rest, arm_swing);

    select(&mut app, 1, 0);
    for _ in 0..7 {
        app.update();
    }
    let held_before = visual(&mut app, "Held stack").2;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.update();
    app.update();
    let held_after = visual(&mut app, "Held stack").2;
    assert_ne!(held_before, held_after);
}

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::MinimalPlugins;
use bevy::prelude::*;
use game::player::Player;
use game::world::clouds::CLOUD_HEIGHT;
use game::world::clouds::cloud_color;
use game::world::clouds::cloud_scroll_blocks;
use game::world::clouds::fancy_cloud_mesh;
use game::world::clouds::fast_cloud_mesh;
use game::world::plugin::WorldPlugin;

fn positions(mesh: &bevy::mesh::Mesh) -> Vec<[f32; 3]> {
    let VertexAttributeValues::Float32x3(values) = mesh
        .attribute(bevy::mesh::Mesh::ATTRIBUTE_POSITION)
        .unwrap()
    else {
        panic!("cloud positions should be float triples");
    };
    values.clone()
}

#[test]
fn cloud_follow_runs_beside_the_player() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins(WorldPlugin);
    app.world_mut()
        .spawn((Player, Transform::from_xyz(10.0, 70.0, -4.0)));
    app.update();
    app.update();
    let mut clouds = app.world_mut().query::<(&Name, &Transform)>();
    let found = clouds.iter(app.world()).any(|(name, transform)| {
        name.as_str() == "Fancy clouds" && transform.translation.y > 100.0
    });
    assert!(
        found,
        "fancy clouds should follow the player without a query conflict"
    );
}

#[test]
fn cloud_color_tracks_daylight() {
    let noon = cloud_color(1.0);
    assert!((noon[0] - 1.0).abs() < 1e-5);
    assert!((noon[2] - 1.0).abs() < 1e-5);
    let night = cloud_color(0.0);
    assert!((night[0] - 0.1).abs() < 1e-5);
    assert!((night[1] - 0.1).abs() < 1e-5);
    assert!((night[2] - 0.15).abs() < 1e-5);
}

#[test]
fn cloud_scroll_advances_and_wraps() {
    assert!((cloud_scroll_blocks(0, 0.0)).abs() < 1e-5);
    assert!((cloud_scroll_blocks(1, 0.0) - 0.03).abs() < 1e-4);
    let wrapped = cloud_scroll_blocks(1_000_000, 0.0);
    assert!((0.0..2048.0).contains(&wrapped));
}

#[test]
fn fast_clouds_cover_a_flat_sheet_at_the_camera() {
    let points = positions(&fast_cloud_mesh());
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x + 256.0).abs() < 1e-3);
    assert!((max_x - 256.0).abs() < 1e-3);
    assert!(max_y.abs() < 1e-5);
    assert!((CLOUD_HEIGHT - 108.33).abs() < 1e-5);
}

#[test]
fn fancy_clouds_are_four_blocks_thick() {
    let points = positions(&fancy_cloud_mesh());
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    assert!(min_y.abs() < 1e-4);
    assert!((max_y - 4.0).abs() < 1e-3);
    assert!(points.len() > positions(&fast_cloud_mesh()).len());
}

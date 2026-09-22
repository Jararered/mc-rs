use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::MinimalPlugins;
use bevy::prelude::*;
use game::player::Player;
use game::world::clouds::CLOUD_HEIGHT;
use game::world::clouds::cloud_color;
use game::world::clouds::cloud_scroll_blocks;
use game::world::clouds::cloud_uv_offset;
use game::world::clouds::fancy_cloud_anchor;
use game::world::clouds::fancy_cloud_mesh;
use game::world::clouds::fast_cloud_mesh;
use game::world::plugin::WorldPlugin;

fn attribute(mesh: &bevy::mesh::Mesh, id: bevy::mesh::MeshVertexAttribute) -> Vec<[f32; 3]> {
    let VertexAttributeValues::Float32x3(values) = mesh.attribute(id.id).unwrap() else {
        panic!("cloud attribute should be float triples");
    };
    values.clone()
}

fn positions(mesh: &bevy::mesh::Mesh) -> Vec<[f32; 3]> {
    attribute(mesh, bevy::mesh::Mesh::ATTRIBUTE_POSITION)
}

fn uvs(mesh: &bevy::mesh::Mesh) -> Vec<[f32; 2]> {
    let VertexAttributeValues::Float32x2(values) =
        mesh.attribute(bevy::mesh::Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!("cloud uvs should be float pairs");
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

    let placed = cloud_places(&mut app);
    assert_eq!(placed.len(), 2, "one fast sheet and one fancy sheet");
    assert_cloud_anchor(&placed, 10.0, -4.0);

    {
        let mut players = app
            .world_mut()
            .query_filtered::<&mut Transform, With<Player>>();
        let mut player = players.single_mut(app.world_mut()).unwrap();
        player.translation.x = 400.0;
        player.translation.z = 48.0;
    }
    app.update();

    let placed = cloud_places(&mut app);
    assert_eq!(
        placed.len(),
        2,
        "walking a chunk must not spawn another sheet"
    );
    assert_cloud_anchor(&placed, 400.0, 48.0);

    let mut materials = app
        .world_mut()
        .query::<(&Name, &MeshMaterial3d<StandardMaterial>)>();
    let handles: Vec<_> = materials
        .iter(app.world())
        .filter_map(|(name, material)| {
            let name = name.as_str();
            (name == "Fast clouds" || name == "Fancy clouds")
                .then(|| (name.to_string(), material.0.clone()))
        })
        .collect();
    let assets = app.world().resource::<Assets<StandardMaterial>>();
    let fast_uv = cloud_uv_offset(400.0, 48.0, 0.0);
    let (_, fancy_uv) = fancy_cloud_anchor(400.0, 48.0, 0.0);
    for (name, handle) in handles {
        let material = assets.get(&handle).unwrap();
        assert!(
            matches!(material.alpha_mode, AlphaMode::Mask(cutoff) if (cutoff - 0.5).abs() < 1e-5),
            "{name} should cut out empty texels and stay opaque"
        );
        assert!((material.base_color.alpha() - 1.0).abs() < 1e-5);
        let expected = if name == "Fancy clouds" {
            fancy_uv
        } else {
            fast_uv
        };
        let uv = material.uv_transform.translation;
        assert!(
            (uv.x - expected.x).abs() < 1e-4 && (uv.y - expected.y).abs() < 1e-4,
            "{name} should stay world-locked, uv offset {uv:?}, expected {expected:?}"
        );
    }
}

fn assert_cloud_anchor(placed: &[(String, Vec3)], player_x: f32, player_z: f32) {
    let (fancy_place, _) = fancy_cloud_anchor(player_x, player_z, 0.0);
    for (name, translation) in placed {
        let expected = if name == "Fancy clouds" {
            fancy_place
        } else {
            Vec3::new(player_x, CLOUD_HEIGHT, player_z)
        };
        assert!(
            (translation.x - expected.x).abs() < 1e-3
                && (translation.y - expected.y).abs() < 1e-3
                && (translation.z - expected.z).abs() < 1e-3,
            "{name} should stay anchored, got {translation:?}, expected {expected:?}"
        );
    }
}

#[test]
fn cloud_pattern_stays_put_when_the_player_moves() {
    let before = cloud_uv_offset(10.0, -4.0, 0.0);
    let after = cloud_uv_offset(400.0, 48.0, 0.0);
    let du = (after.x - before.x).rem_euclid(1.0);
    let dv = (after.y - before.y).rem_euclid(1.0);
    assert!((du - 390.0 / 2048.0).abs() < 1e-5);
    assert!((dv - 52.0 / 2048.0).abs() < 1e-5);
}

fn cloud_places(app: &mut App) -> Vec<(String, Vec3)> {
    let mut clouds = app.world_mut().query::<(&Name, &Transform)>();
    clouds
        .iter(app.world())
        .filter_map(|(name, transform)| {
            let name = name.as_str();
            (name == "Fast clouds" || name == "Fancy clouds")
                .then(|| (name.to_string(), transform.translation))
        })
        .collect()
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
    let mesh = fast_cloud_mesh();
    let points = positions(&mesh);
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x + 1024.0).abs() < 1e-3);
    assert!((max_x - 1024.0).abs() < 1e-3);
    assert!(max_y.abs() < 1e-5);
    assert!((CLOUD_HEIGHT - 108.33).abs() < 1e-5);
    let coords = uvs(&mesh);
    let min_u = coords.iter().map(|uv| uv[0]).fold(f32::INFINITY, f32::min);
    let max_u = coords
        .iter()
        .map(|uv| uv[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(min_u.abs() < 1e-5);
    assert!((max_u - 1.0).abs() < 1e-5);
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
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    assert!((min_x + 192.0).abs() < 1e-2);
    assert!((max_x - 384.0).abs() < 1e-2);
    let has_side = points.iter().any(|point| {
        let column = (point[0] / 12.0).round();
        (point[0] - column * 12.0).abs() < 1e-2 && (column as i32).rem_euclid(8) != 0
    });
    assert!(has_side, "fancy clouds should include column sides");
    assert!(points.len() > positions(&fast_cloud_mesh()).len());
}

use bevy::asset::AssetPlugin;
use bevy::mesh::MeshPlugin;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::MinimalPlugins;
use bevy::prelude::*;
use game::app::settings::GameSettings;
use game::player::Player;
use game::rendering::clouds::CLOUD_HEIGHT;
use game::rendering::clouds::cloud_color;
use game::rendering::clouds::cloud_half_extent_blocks;
use game::rendering::clouds::cloud_render_y;
use game::rendering::clouds::cloud_scroll_blocks;
use game::rendering::clouds::fancy_cloud_anchor;
use game::rendering::clouds::fancy_cloud_cells;
use game::rendering::clouds::fancy_cloud_mesh;
use game::rendering::clouds::fast_cloud_anchor;
use game::rendering::clouds::fast_cloud_mesh;
use game::rendering::clouds::fast_cloud_uv;
use game::rendering::clouds::fast_cloud_uv_offset;
use game::rendering::textures::TintedMaterial;
use game::world::plugin::WorldPlugin;

/// Blocks a fancy cell spans: 8 texels at 12 blocks each.
const FANCY_CELL_BLOCKS: f32 = 96.0;

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

fn extent(mesh: &bevy::mesh::Mesh) -> (f32, f32, f32, f32) {
    let points = positions(mesh);
    let fold =
        |select: fn(&[f32; 3]) -> f32| points.iter().map(select).fold(f32::INFINITY, f32::min);
    let fold_max =
        |select: fn(&[f32; 3]) -> f32| points.iter().map(select).fold(f32::NEG_INFINITY, f32::max);
    (
        fold(|p| p[0]),
        fold_max(|p| p[0]),
        fold(|p| p[2]),
        fold_max(|p| p[2]),
    )
}

fn cloud_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), MeshPlugin))
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .add_plugins((WorldPlugin, game::rendering::WorldRenderingPlugin));
    app
}

#[test]
fn cloud_follow_runs_beside_the_player() {
    let mut app = cloud_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(10.0, 70.0, -4.0)));
    app.update();
    app.update();

    let placed = cloud_places(&mut app);
    assert_eq!(placed.len(), 2, "one fast sheet and one fancy sheet");
    assert_cloud_anchor(&placed, 10.0, -4.0, CLOUD_HEIGHT);

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
    assert_cloud_anchor(&placed, 400.0, 48.0, CLOUD_HEIGHT);

    let materials = app
        .world_mut()
        .query::<(&Name, &MeshMaterial3d<TintedMaterial>)>()
        .iter(app.world())
        .filter_map(|(name, material)| {
            let name = name.as_str();
            (name == "Fast clouds" || name == "Fancy clouds")
                .then(|| (name.to_string(), material.0.clone()))
        })
        .collect::<Vec<_>>();
    let assets = app.world().resource::<Assets<TintedMaterial>>();
    let (_, fancy_uv) = fancy_cloud_anchor(400.0, 48.0, 0.0, CLOUD_HEIGHT);
    for (name, handle) in materials {
        let material = &assets.get(&handle).unwrap().base;
        assert!(
            matches!(material.alpha_mode, AlphaMode::Mask(cutoff) if (cutoff - 0.5).abs() < 1e-5),
            "{name} should cut out empty texels and stay opaque"
        );
        assert!((material.base_color.alpha() - 1.0).abs() < 1e-5);
        // Only the visible sheet updates its material. A hidden sheet is
        // synchronized when graphics quality switches to it.
        if name == "Fast clouds" {
            assert_eq!(material.uv_transform.translation, Vec2::ZERO);
            continue;
        }
        let expected = fancy_uv;
        let uv = material.uv_transform.translation;
        assert!(
            (uv.x - expected.x).abs() < 1e-4 && (uv.y - expected.y).abs() < 1e-4,
            "{name} should stay world-locked, uv offset {uv:?}, expected {expected:?}"
        );
    }
}

fn assert_cloud_anchor(placed: &[(String, Vec3)], player_x: f32, player_z: f32, cloud_y: f32) {
    let (fancy_place, _) = fancy_cloud_anchor(player_x, player_z, 0.0, cloud_y);
    for (name, translation) in placed {
        let expected = if name == "Fancy clouds" {
            fancy_place
        } else {
            fast_cloud_anchor(player_x, player_z, cloud_y)
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
fn cloud_height_setting_moves_both_sheets() {
    let mut app = cloud_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(10.0, 70.0, -4.0)));
    app.update();

    let default = GameSettings::default().cloud_height;
    assert!((cloud_render_y(default) - CLOUD_HEIGHT).abs() < 1e-5);
    assert_cloud_anchor(&cloud_places(&mut app), 10.0, -4.0, CLOUD_HEIGHT);

    for cloud_height in [16.0_f32, 200.0, 256.0] {
        {
            let mut settings = app.world_mut().resource_mut::<GameSettings>();
            settings.cloud_height = cloud_height;
        }
        app.update();
        assert_cloud_anchor(
            &cloud_places(&mut app),
            10.0,
            -4.0,
            cloud_render_y(cloud_height),
        );
    }

    // The fancy columns keep their four-block thickness above the new height.
    let anchor = cloud_render_y(200.0);
    let points = positions(&fancy_cloud_mesh(1));
    let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((anchor + min_y - 200.33).abs() < 1e-3);
    assert!((anchor + max_y - 204.33).abs() < 1e-3);
}

#[test]
fn cloud_pattern_stays_put_when_the_player_moves() {
    // A fixed world point samples the same texel wherever the sheet sits.
    let point = (700.0, -300.0);
    let texel = |player_x, player_z, scroll| {
        let sheet = fast_cloud_anchor(player_x, player_z, CLOUD_HEIGHT);
        let offset = fast_cloud_uv_offset(player_x, player_z, scroll);
        let uv = fast_cloud_uv(sheet, offset, point.0, point.1);
        Vec2::new(uv.x.rem_euclid(1.0), uv.y.rem_euclid(1.0))
    };
    let before = texel(10.0, -4.0, 0.0);
    let after = texel(400.0, 48.0, 0.0);
    let far = texel(-9_000.0, 12_345.0, 0.0);
    assert!((after - before).length() < 1e-4);
    assert!((far - before).length() < 1e-4);

    // Drift moves the pattern by the scroll distance along X.
    let drifted = texel(10.0, -4.0, 30.0);
    assert!(((drifted.x - before.x).rem_euclid(1.0) - 30.0 / 2048.0).abs() < 1e-4);
    assert!((drifted.y - before.y).abs() < 1e-4);
}

#[test]
fn fast_cloud_sheet_is_centered_on_the_player() {
    // The sheet no longer slides a whole texture period away from the player,
    // so it can be exactly the size of the view.
    for (x, z) in [(0.0, 0.0), (1023.0, -1023.0), (-40_000.0, 77_777.0)] {
        let sheet = fast_cloud_anchor(x, z, CLOUD_HEIGHT);
        assert!((sheet.x - x).abs() < 1e-2);
        assert!((sheet.z - z).abs() < 1e-2);
        assert!((sheet.y - CLOUD_HEIGHT).abs() < 1e-5);
    }
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

fn cloud_mesh_handles(app: &mut App) -> (Handle<Mesh>, Handle<Mesh>) {
    let mut clouds = app.world_mut().query::<(&Name, &Mesh3d)>();
    let mut fast = None;
    let mut fancy = None;
    for (name, mesh) in clouds.iter(app.world()) {
        match name.as_str() {
            "Fast clouds" => fast = Some(mesh.0.clone()),
            "Fancy clouds" => fancy = Some(mesh.0.clone()),
            _ => {}
        }
    }
    (
        fast.expect("a fast sheet should be spawned"),
        fancy.expect("a fancy sheet should be spawned"),
    )
}

fn assert_fast_sheet(app: &App, handle: &Handle<Mesh>, render_chunks: i32) {
    let half = cloud_half_extent_blocks(render_chunks);
    let assets = app.world().resource::<Assets<Mesh>>();
    let mesh = assets.get(handle).unwrap();
    let (min_x, max_x, min_z, max_z) = extent(mesh);
    assert!(
        (min_x + half).abs() < 1e-3
            && (max_x - half).abs() < 1e-3
            && (min_z + half).abs() < 1e-3
            && (max_z - half).abs() < 1e-3,
        "the fast sheet should span {half} blocks either side, got {min_x}..{max_x}, {min_z}..{max_z}"
    );
    // Baked UVs are `local / 2048`; the material supplies the rest.
    let coords = uvs(mesh);
    for (uv, point) in coords.iter().zip(positions(mesh)) {
        assert!((uv[0] - point[0] / 2048.0).abs() < 1e-6);
        assert!((uv[1] - point[2] / 2048.0).abs() < 1e-6);
    }
}

fn assert_fancy_window(app: &App, handle: &Handle<Mesh>, render_chunks: i32) {
    let half = cloud_half_extent_blocks(render_chunks);
    let cells = fancy_cloud_cells(half);
    let assets = app.world().resource::<Assets<Mesh>>();
    let mesh = assets.get(handle).unwrap();
    let (min_x, max_x, min_z, max_z) = extent(mesh);
    assert!(
        (min_x + cells as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2
            && (max_x - (cells + 1) as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2
            && (min_z + cells as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2
            && (max_z - (cells + 1) as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2,
        "{cells} cells should span the window, got {min_x}..{max_x}, {min_z}..{max_z}"
    );
    assert!(
        cells as f32 * FANCY_CELL_BLOCKS - 12.0 >= half,
        "the snapped window should still reach the render distance"
    );
}

#[test]
fn cloud_geometry_follows_the_render_distance() {
    let mut app = cloud_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(10.0, 70.0, -4.0)));
    app.update();

    let (fast, fancy) = cloud_mesh_handles(&mut app);
    assert_fast_sheet(&app, &fast, 4);
    assert_fancy_window(&app, &fancy, 4);

    for render_chunks in [16, 8, 32] {
        {
            let mut settings = app.world_mut().resource_mut::<GameSettings>();
            settings.render_distance = render_chunks;
        }
        app.update();
        let (next_fast, next_fancy) = cloud_mesh_handles(&mut app);
        assert_ne!(
            fast.id(),
            next_fast.id(),
            "the fast sheet should be rebuilt at {render_chunks} chunks"
        );
        assert_ne!(
            fancy.id(),
            next_fancy.id(),
            "the fancy window should be rebuilt at {render_chunks} chunks"
        );
        assert_fast_sheet(&app, &next_fast, render_chunks);
        assert_fancy_window(&app, &next_fancy, render_chunks);
    }
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
    assert!(cloud_scroll_blocks(0, 0.0).abs() < 1e-5);
    assert!((cloud_scroll_blocks(1, 0.0) - 0.03).abs() < 1e-4);
    let wrapped = cloud_scroll_blocks(1_000_000, 0.0);
    assert!((0.0..2048.0).contains(&wrapped));
}

#[test]
fn fast_clouds_cover_the_render_distance() {
    for chunks in [4, 8, 16, 32] {
        let half = cloud_half_extent_blocks(chunks);
        assert!(
            (half - chunks as f32 * 16.0).abs() < 1e-5,
            "clouds should reach as far as the loaded world"
        );
        let mesh = fast_cloud_mesh(half);
        let (min_x, max_x, min_z, max_z) = extent(&mesh);
        assert!((min_x + half).abs() < 1e-3 && (max_x - half).abs() < 1e-3);
        assert!((min_z + half).abs() < 1e-3 && (max_z - half).abs() < 1e-3);
        let points = positions(&mesh);
        let max_y = points
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(max_y.abs() < 1e-5, "the fast sheet is flat");
        assert!((CLOUD_HEIGHT - 108.33).abs() < 1e-5);
        // Baked UVs are `local / 2048`; the material's offset completes them.
        // They stay far from a whole period, so the world-locked sample keeps
        // its precision at any render distance.
        for (uv, point) in uvs(&mesh).iter().zip(&points) {
            assert!((uv[0] - point[0] / 2048.0).abs() < 1e-6);
            assert!((uv[1] - point[2] / 2048.0).abs() < 1e-6);
        }
        assert!(half / 2048.0 <= 0.25, "baked uvs stay small");
    }
}

#[test]
fn fancy_clouds_cover_the_render_distance() {
    for chunks in [4, 8, 16, 32] {
        let half = cloud_half_extent_blocks(chunks);
        let cells = fancy_cloud_cells(half);
        assert!(cells >= 1);
        let mesh = fancy_cloud_mesh(cells);
        let points = positions(&mesh);
        let max_y = points
            .iter()
            .map(|p| p[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        assert!(min_y.abs() < 1e-4, "columns sit on the sheet");
        assert!((max_y - 4.0).abs() < 1e-3, "columns are four blocks thick");
        let (min_x, max_x, min_z, max_z) = extent(&mesh);
        assert!((min_x + cells as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2);
        assert!((max_x - (cells + 1) as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2);
        assert!((min_z + cells as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2);
        assert!((max_z - (cells + 1) as f32 * FANCY_CELL_BLOCKS).abs() < 1e-2);
        assert!(
            cells as f32 * FANCY_CELL_BLOCKS - 12.0 >= half,
            "{cells} cells should cover {half} blocks"
        );
        let has_side = points.iter().any(|point| {
            let column = (point[0] / 12.0).round();
            (point[0] - column * 12.0).abs() < 1e-2 && (column as i32).rem_euclid(8) != 0
        });
        assert!(has_side, "fancy clouds should include column sides");
        // One draw call, even at the longest view distance.
        assert!(
            points.len() < 24_000,
            "{cells} cells is {} vertices",
            points.len()
        );
    }
}

#[test]
fn hidden_cloud_material_stays_unchanged_and_switching_modes_synchronizes_it() {
    use game::app::settings::GraphicsQuality;
    use game::world::tick::WorldTick;
    let mut app = cloud_app();
    app.world_mut()
        .spawn((Player, Transform::from_xyz(400.0, 70.0, 48.0)));
    app.update();
    app.update();
    let mut query = app
        .world_mut()
        .query::<(&Name, &MeshMaterial3d<TintedMaterial>)>();
    let fast = query
        .iter(app.world())
        .find(|(name, _)| name.as_str() == "Fast clouds")
        .unwrap()
        .1
        .0
        .clone();
    let before = app
        .world()
        .resource::<Assets<TintedMaterial>>()
        .get(&fast)
        .unwrap()
        .base
        .uv_transform;
    app.world_mut().resource_mut::<WorldTick>().advance(0.2);
    app.update();
    assert_eq!(
        app.world()
            .resource::<Assets<TintedMaterial>>()
            .get(&fast)
            .unwrap()
            .base
            .uv_transform,
        before
    );
    app.world_mut().resource_mut::<GameSettings>().graphics = GraphicsQuality::Fast;
    app.update();
    let tick = app.world().resource::<WorldTick>();
    let scroll = cloud_scroll_blocks(tick.world_time(), tick.partial());
    let offset = app
        .world()
        .resource::<Assets<TintedMaterial>>()
        .get(&fast)
        .unwrap()
        .base
        .uv_transform
        .translation;
    assert_eq!(offset, fast_cloud_uv_offset(400.0, 48.0, scroll));
}

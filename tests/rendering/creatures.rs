//! Beta creature models: box geometry, skin UVs, and `RenderLiving` posing.

use bevy::mesh::Mesh;
use bevy::mesh::MeshTag;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use game::entity::mobs::MobKind;
use game::rendering::creatures::creature_tag;
use game::rendering::creatures::models;
use game::rendering::creatures::models::Layer;
use game::rendering::creatures::models::Part;
use game::rendering::creatures::models::PoseInput;

fn floats3(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> Vec<Vec3> {
    let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute(attribute.id) else {
        panic!("expected float triples");
    };
    values.iter().copied().map(Vec3::from_array).collect()
}

fn uvs(mesh: &Mesh) -> Vec<Vec2> {
    let Some(VertexAttributeValues::Float32x2(values)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0.id)
    else {
        panic!("expected float pairs");
    };
    values.iter().copied().map(Vec2::from_array).collect()
}

/// World-space corners of a posed part, relative to the feet.
fn posed_corners(part: &Part, input: &PoseInput, body_yaw: f32) -> Vec<Vec3> {
    let frame = models::model_transform(body_yaw, None);
    let pose = models::pose(part, input);
    floats3(&models::cuboid_mesh(&part.cuboid), Mesh::ATTRIBUTE_POSITION)
        .into_iter()
        .map(|corner| frame.transform_point(pose.transform_point(corner)))
        .collect()
}

fn bounds(kind: MobKind, layer: Layer, input: &PoseInput) -> (Vec3, Vec3) {
    models::model(kind)
        .iter()
        .filter(|part| part.layer == layer)
        .flat_map(|part| posed_corners(part, input, 0.0))
        .fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), point| (min.min(point), max.max(point)),
        )
}

fn center(part: &Part, input: &PoseInput, body_yaw: f32) -> Vec3 {
    let corners = posed_corners(part, input, body_yaw);
    corners.iter().sum::<Vec3>() / corners.len() as f32
}

#[test]
fn land_animals_stand_on_their_feet_at_beta_heights() {
    // The top of each model's highest box, from its Beta rotation point:
    // the pig's and sheep's heads, the cow's horns, the chicken's comb, and
    // the wolf's ears. `RenderLiving` lifts every model 1/128 off the ground.
    for (kind, top) in [
        (MobKind::Pig, 1.007_812_5),
        (MobKind::Cow, 1.632_812_5),
        (MobKind::Sheep, 1.382_812_5),
        (MobKind::Chicken, 0.945_312_5),
        (MobKind::Wolf, 0.976_562_5),
    ] {
        let (min, max) = bounds(kind, Layer::Base, &PoseInput::default());
        assert!(
            (min.y - 1.0 / 128.0).abs() < 1e-4,
            "{kind:?} feet at {}",
            min.y
        );
        assert!((max.y - top).abs() < 1e-4, "{kind:?} top at {}", max.y);
    }
}

#[test]
fn overlay_layers_wrap_the_base_model() {
    let rest = PoseInput::default();
    let (pig_min, pig_max) = bounds(MobKind::Pig, Layer::Base, &rest);
    let (saddle_min, saddle_max) = bounds(MobKind::Pig, Layer::Saddle, &rest);
    assert!(saddle_min.cmplt(pig_min).all() && saddle_max.cmpgt(pig_max).all());
    // The fleece hugs the body and upper legs, leaving the hooves bare.
    let (_, body_max) = bounds(MobKind::Sheep, Layer::Base, &rest);
    let (fleece_min, fleece_max) = bounds(MobKind::Sheep, Layer::Fleece, &rest);
    assert!(fleece_max.y > body_max.y);
    assert!(fleece_min.y > 0.3);
}

#[test]
fn heads_lead_in_the_direction_beta_yaw_faces() {
    let rest = PoseInput::default();
    for kind in [
        MobKind::Pig,
        MobKind::Cow,
        MobKind::Sheep,
        MobKind::Chicken,
        MobKind::Wolf,
    ] {
        let parts = models::model(kind);
        let body = if kind == MobKind::Chicken { 3 } else { 1 };
        let (head, body) = (&parts[0], &parts[body]);
        // Yaw 0 faces +Z.
        let ahead = center(head, &rest, 0.0) - center(body, &rest, 0.0);
        assert!(ahead.z > 0.15 && ahead.x.abs() < 0.1, "{kind:?}: {ahead}");
        // Yaw 90 faces -X.
        let ahead = center(head, &rest, 90.0) - center(body, &rest, 90.0);
        assert!(ahead.x < -0.15 && ahead.z.abs() < 0.1, "{kind:?}: {ahead}");
    }
}

#[test]
fn head_yaw_turns_the_head_but_not_the_body() {
    let parts = models::model(MobKind::Pig);
    let rest = PoseInput::default();
    let turned = PoseInput {
        head_yaw: 45.0,
        ..rest
    };
    assert_eq!(
        center(&parts[1], &rest, 0.0),
        center(&parts[1], &turned, 0.0)
    );
    let head = center(&parts[0], &turned, 0.0) - center(&parts[0], &rest, 0.0);
    // A positive Beta yaw turns toward -X.
    assert!(head.x < -0.05, "{head}");
}

#[test]
fn diagonal_legs_swing_together() {
    let parts = models::model(MobKind::Cow);
    let stride = PoseInput {
        limb_amount: 1.0,
        ..default()
    };
    let foot = |index: usize| {
        let rest = center(&parts[index], &PoseInput::default(), 0.0);
        center(&parts[index], &stride, 0.0).z - rest.z
    };
    // Legs 1 and 4 (back right, front left) move opposite 2 and 3.
    assert!(foot(2) * foot(3) < 0.0);
    assert!(foot(2) * foot(5) > 0.0);
    assert!(foot(3) * foot(4) > 0.0);
}

#[test]
fn sitting_wolves_lower_their_bodies() {
    let parts = models::model(MobKind::Wolf);
    let standing = PoseInput::default();
    let sitting = PoseInput {
        sitting: true,
        ..standing
    };
    assert!(center(&parts[1], &sitting, 0.0).y < center(&parts[1], &standing, 0.0).y);
    assert_eq!(
        center(&parts[0], &sitting, 0.0),
        center(&parts[0], &standing, 0.0)
    );
}

#[test]
fn boxes_sample_beta_skin_rectangles() {
    // The pig's 8×8×8 head starts at texel (0, 0).
    let head = models::model(MobKind::Pig)[0];
    let mesh = models::cuboid_mesh(&head.cuboid);
    let normals = floats3(&mesh, Mesh::ATTRIBUTE_NORMAL);
    let positions = floats3(&mesh, Mesh::ATTRIBUTE_POSITION);
    let uv = uvs(&mesh);
    let face = |normal: Vec3| -> Vec<usize> {
        (0..normals.len())
            .filter(|&i| normals[i].distance(normal) < 1e-5)
            .collect()
    };
    let rect = |vertices: &[usize]| {
        vertices.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(min, max), &i| (min.min(uv[i]), max.max(uv[i])),
        )
    };
    let inset = Vec2::new(0.1 / 64.0, 0.1 / 32.0);
    let expect = |vertices: &[usize], min: [f32; 2], max: [f32; 2]| {
        let (low, high) = rect(vertices);
        let size = Vec2::new(64.0, 32.0);
        assert!(
            low.distance(Vec2::from_array(min) / size + inset) < 1e-6,
            "{low}"
        );
        assert!(
            high.distance(Vec2::from_array(max) / size - inset) < 1e-6,
            "{high}"
        );
    };
    // Model space faces -Z forward and +Y down.
    let front = face(Vec3::NEG_Z);
    expect(&front, [8.0, 8.0], [16.0, 16.0]);
    expect(&face(Vec3::NEG_Y), [8.0, 0.0], [16.0, 8.0]);
    expect(&face(Vec3::Y), [16.0, 0.0], [24.0, 8.0]);
    expect(&face(Vec3::Z), [24.0, 8.0], [32.0, 16.0]);
    expect(&face(Vec3::X), [16.0, 8.0], [24.0, 16.0]);
    expect(&face(Vec3::NEG_X), [0.0, 8.0], [8.0, 16.0]);
    // Seen from the front, the face's right edge is its high x and high u,
    // so the skin is not mirrored.
    let right = front
        .iter()
        .copied()
        .max_by(|&a, &b| positions[a].x.total_cmp(&positions[b].x))
        .unwrap();
    assert!(uv[right].x > 0.2);
    assert_eq!(mesh.indices().unwrap().len(), 36);
}

#[test]
fn squid_tentacles_ring_the_body() {
    let parts = models::model(MobKind::Squid);
    assert_eq!(parts.len(), 9);
    for tentacle in &parts[1..] {
        let ring = Vec2::new(tentacle.pivot.x, tentacle.pivot.z);
        assert!((ring.length() - 5.0).abs() < 1e-4);
        assert_eq!(tentacle.pivot.y, 15.0);
    }
    // Upright, the top of the mantle sits 0.31 above `RenderSquid`'s turning
    // point, half a block above the feet.
    let frame = models::model_transform(0.0, Some((0.0, 0.0)));
    let rest = PoseInput::default();
    let mantle = models::pose(&parts[0], &rest);
    let top = frame.transform_point(mantle.transform_point(Vec3::new(0.0, -8.0, 0.0)));
    assert!((top.y - (0.5 + 0.307_812_5)).abs() < 1e-5, "{top}");
}

#[test]
fn an_untagged_creature_draws_white_at_full_brightness() {
    assert_eq!(creature_tag(Color::WHITE, 1.0), MeshTag::default());
    assert_ne!(creature_tag(Color::WHITE, 0.5), MeshTag::default());
}

#[test]
fn tamed_wolf_tails_droop_as_they_lose_health() {
    assert!(models::wolf_tail(true, false, 8) > models::wolf_tail(false, false, 8));
    assert!(models::wolf_tail(false, true, 20) > models::wolf_tail(false, true, 4));
}

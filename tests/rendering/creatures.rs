//! Beta creature models: box geometry, skin UVs, and `RenderLiving` posing.

use bevy::mesh::Mesh;
use bevy::mesh::MeshTag;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use game::entity::mobs::MobKind;
use game::rendering::creatures::creature_tag;
use game::rendering::creatures::models;
use game::rendering::creatures::models::Frame;
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
    let frame = models::model_transform(Frame::facing(body_yaw));
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
    let frame = models::model_transform(Frame {
        squid: Some((0.0, 0.0)),
        ..Frame::facing(0.0)
    });
    let rest = PoseInput::default();
    let mantle = models::pose(&parts[0], &rest);
    let top = frame.transform_point(mantle.transform_point(Vec3::new(0.0, -8.0, 0.0)));
    assert!((top.y - (0.5 + 0.307_812_5)).abs() < 1e-5, "{top}");
}

#[test]
fn an_untagged_creature_draws_white_at_full_brightness() {
    assert_eq!(creature_tag(1.0, false, 0.0, 1.0), MeshTag::default());
    assert_ne!(creature_tag(0.5, false, 0.0, 1.0), MeshTag::default());
    assert_ne!(creature_tag(1.0, true, 0.0, 1.0), MeshTag::default());
}

#[test]
fn tamed_wolf_tails_droop_as_they_lose_health() {
    assert!(models::wolf_tail(true, false, 8) > models::wolf_tail(false, false, 8));
    assert!(models::wolf_tail(false, true, 20) > models::wolf_tail(false, true, 4));
}

#[test]
fn hostile_mobs_stand_at_beta_heights() {
    // Zombies and skeletons top out at their headwear, half a pixel above
    // the head; the creeper at its head; the spider at its abdomen. Beta's
    // `ModelCreeper` sets its legs two pixels short, so it hovers. A
    // spider rests on the tips of its splayed legs.
    for (kind, bottom, top) in [
        (MobKind::Zombie, 0.007_812_5, 2.039_062_5),
        (MobKind::Skeleton, 0.007_812_5, 2.039_062_5),
        (MobKind::PigZombie, 0.007_812_5, 2.039_062_5),
        (MobKind::Creeper, 0.132_812_5, 1.757_812_5),
        (MobKind::Spider, 0.026_1, 0.820_312_5),
    ] {
        let (min, max) = bounds(kind, Layer::Base, &PoseInput::default());
        assert!(
            (min.y - bottom).abs() < 1e-3,
            "{kind:?} bottom at {}",
            min.y
        );
        assert!((max.y - top).abs() < 1e-3, "{kind:?} top at {}", max.y);
    }
}

#[test]
fn zombies_hold_their_arms_out_in_front() {
    let parts = models::model(MobKind::Zombie);
    let rest = PoseInput::default();
    let arm = center(&parts[2], &rest, 0.0);
    let body = center(&parts[1], &rest, 0.0);
    // Facing +Z, the arm reaches forward of the body at shoulder height.
    assert!(arm.z - body.z > 0.2, "{arm} vs {body}");
    assert!(arm.y > body.y);
}

#[test]
fn mirrored_limbs_read_the_skin_reversed() {
    let parts = models::model(MobKind::Zombie);
    let (right, left) = (parts[4], parts[5]);
    assert!(!right.cuboid.mirror && left.cuboid.mirror);
    let front_u = |part: Part| {
        let mesh = models::cuboid_mesh(&part.cuboid);
        let normals = floats3(&mesh, Mesh::ATTRIBUTE_NORMAL);
        let positions = floats3(&mesh, Mesh::ATTRIBUTE_POSITION);
        let uv = uvs(&mesh);
        let front: Vec<usize> = (0..normals.len())
            .filter(|&i| normals[i].distance(Vec3::NEG_Z) < 1e-5)
            .collect();
        assert_eq!(front.len(), 4, "a mirrored box keeps outward faces");
        let high_x = front
            .iter()
            .copied()
            .max_by(|&a, &b| positions[a].x.total_cmp(&positions[b].x))
            .unwrap();
        uv[high_x].x
    };
    // The same texels, laid the other way across the front face.
    assert!(front_u(left) < front_u(right));
}

#[test]
fn spiders_splay_eight_legs_and_glow_at_the_head() {
    let parts = models::model(MobKind::Spider);
    let legs = parts
        .iter()
        .filter(|part| matches!(part.role, models::Role::SpiderLeg(_)))
        .count();
    assert_eq!(legs, 8);
    let eyes: Vec<_> = parts
        .iter()
        .filter(|part| part.layer == Layer::Eyes)
        .collect();
    assert_eq!(eyes.len(), 1);
    assert_eq!(eyes[0].cuboid, parts[0].cuboid);
    // Left legs reach out to -X, right legs to +X.
    let rest = PoseInput::default();
    for part in &parts[3..11] {
        let models::Role::SpiderLeg(n) = part.role else {
            unreachable!()
        };
        let reach = center(part, &rest, 0.0).x;
        assert!(
            if n % 2 == 1 {
                reach < -0.4
            } else {
                reach > 0.4
            },
            "leg {n}: {reach}"
        );
    }
}

#[test]
fn ghasts_trail_nine_tentacles_of_seeded_lengths() {
    let parts = models::model(MobKind::Ghast);
    let lengths: Vec<u8> = parts[1..].iter().map(|part| part.cuboid.size[1]).collect();
    assert_eq!(lengths.len(), 9);
    assert!(lengths.iter().all(|length| (8..=14).contains(length)));
    // `new Random(1660)` decides them, so every ghast looks the same.
    assert_eq!(
        lengths,
        models::model(MobKind::Ghast)[1..]
            .iter()
            .map(|p| p.cuboid.size[1])
            .collect::<Vec<_>>()
    );
}

#[test]
fn slimes_and_creepers_carry_their_second_pass() {
    let slime = models::model(MobKind::Slime);
    assert_eq!(
        slime
            .iter()
            .filter(|part| part.layer == Layer::SlimeOuter)
            .count(),
        1
    );
    let creeper = models::model(MobKind::Creeper);
    let charge: Vec<_> = creeper
        .iter()
        .filter(|part| part.layer == Layer::Charge)
        .collect();
    assert_eq!(charge.len(), 6);
    assert!(charge.iter().all(|part| part.cuboid.inflate == 2.0));
}

#[test]
fn dying_mobs_tip_over_onto_their_side() {
    assert_eq!(models::death_tilt(0.0, 90.0), 0.0);
    assert!(models::death_tilt(5.0, 90.0) > 30.0);
    assert_eq!(models::death_tilt(20.0, 90.0), 90.0);
    let parts = models::model(MobKind::Pig);
    let input = PoseInput::default();
    let upright = models::model_transform(Frame::facing(0.0));
    let fallen = models::model_transform(Frame {
        death_tilt: 90.0,
        ..Frame::facing(0.0)
    });
    let head = |frame: Transform| {
        frame.transform_point(models::pose(&parts[0], &input).transform_point(Vec3::ZERO))
    };
    // Tipped over, the head drops toward the ground.
    assert!(head(fallen).y < head(upright).y - 0.3);
}

#[test]
fn held_items_sit_in_the_right_hand() {
    assert_eq!(
        models::held_item(MobKind::Skeleton),
        Some(game::item::ItemId::Bow)
    );
    assert_eq!(
        models::held_item(MobKind::PigZombie),
        Some(game::item::ItemId::GoldSword)
    );
    assert_eq!(models::held_item(MobKind::Zombie), None);
    // The sprite's center ends up near the end of the arm, which hangs to
    // +12 pixels from the shoulder.
    for full_3d in [false, true] {
        let held = models::held_item_transform(full_3d);
        let center = held.transform_point(Vec3::new(0.5, 0.5, -1.0 / 32.0));
        assert!(center.length() < 16.0 && center.y > 4.0, "{center}");
    }
}

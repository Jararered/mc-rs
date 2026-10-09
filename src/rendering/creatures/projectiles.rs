//! `RenderArrow`, `RenderFireball`, `RenderSnowball` and `RenderFish`.
//!
//! An arrow is a crossed pair of shaft quads and a fletching cross from
//! `item/arrows.png`, turned to its flight and quivering after it strikes. A
//! fireball is the snowball icon from `gui/items.png`, twice its size and
//! always facing the camera. A thrown snowball or egg is its own icon at half
//! size, and the bobber is a tile of `particles.png` with a black line of
//! sixteen segments sagging back to the angler's hand. The line ends at the
//! first-person hand; `RenderFish` also sways that end with the arm's swing,
//! which is not copied.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use super::CreatureAssets;
use super::CreatureMaterial;
use super::creature_tag;
use super::entity_brightness;
use crate::entity::PreviousTick;
use crate::entity::fishing::Bobber;
use crate::entity::projectiles::Arrow;
use crate::entity::projectiles::Fireball;
use crate::entity::thrown::Thrown;
use crate::entity::thrown::ThrownKind;
use crate::item::Item;
use crate::player::PlayerCamera;
use crate::rendering::appearance::item_tile;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;

/// The mesh drawn for an arrow or fireball, as a child of its body.
#[derive(Component)]
pub(super) struct ProjectileModel;

#[derive(Resource)]
pub(super) struct ProjectileMeshes {
    arrow: Handle<Mesh>,
    fireball: Handle<Mesh>,
    snowball: Handle<Mesh>,
    egg: Handle<Mesh>,
    bobber: Handle<Mesh>,
    line: Handle<Mesh>,
}

/// One of the straight pieces of a fishing line, a child of its bobber.
#[derive(Component)]
pub(super) struct LineSegment(u8);

/// `RenderFish` draws the line as a strip of this many pieces.
const LINE_SEGMENTS: u8 = 16;
/// The line is one pixel wide in Beta; this is its width in blocks.
const LINE_WIDTH: f32 = 0.01;
/// Where the line meets the rod in first person, in camera space:
/// `RenderFish`'s `(-0.5, 0.03, 0.8)`, which is left-handed with +Z ahead.
const ROD_TIP: Vec3 = Vec3::new(0.5, 0.03, -0.8);

struct Quads {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u16>,
}

impl Quads {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
        }
    }

    fn quad(&mut self, transform: Mat4, corners: [([f32; 3], [f32; 2]); 4], normal: Vec3) {
        let base = self.positions.len() as u16;
        let normal = transform.transform_vector3(normal).normalize_or_zero();
        for (position, uv) in corners {
            self.positions.push(
                transform
                    .transform_point3(Vec3::from_array(position))
                    .to_array(),
            );
            self.normals.push(normal.to_array());
            self.uvs.push(uv);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn finish(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U16(self.indices))
    }
}

/// `RenderArrow`'s geometry in the arrow's frame: +X points along its flight.
pub fn arrow_mesh() -> Mesh {
    let mut quads = Quads::new();
    let frame = Mat4::from_rotation_x(45f32.to_radians())
        * Mat4::from_scale(Vec3::splat(0.05625))
        * Mat4::from_translation(Vec3::new(-4.0, 0.0, 0.0));
    let (fletch_top, fletch_bottom) = (5.0 / 32.0, 10.0 / 32.0);
    quads.quad(
        frame,
        [
            ([-7.0, -2.0, -2.0], [0.0, fletch_top]),
            ([-7.0, -2.0, 2.0], [0.156_25, fletch_top]),
            ([-7.0, 2.0, 2.0], [0.156_25, fletch_bottom]),
            ([-7.0, 2.0, -2.0], [0.0, fletch_bottom]),
        ],
        Vec3::X,
    );
    quads.quad(
        frame,
        [
            ([-7.0, 2.0, -2.0], [0.0, fletch_top]),
            ([-7.0, 2.0, 2.0], [0.156_25, fletch_top]),
            ([-7.0, -2.0, 2.0], [0.156_25, fletch_bottom]),
            ([-7.0, -2.0, -2.0], [0.0, fletch_bottom]),
        ],
        Vec3::NEG_X,
    );
    for turn in 1..=4 {
        let shaft = frame * Mat4::from_rotation_x((90.0 * turn as f32).to_radians());
        quads.quad(
            shaft,
            [
                ([-8.0, -2.0, 0.0], [0.0, 0.0]),
                ([8.0, -2.0, 0.0], [0.5, 0.0]),
                ([8.0, 2.0, 0.0], [0.5, fletch_top]),
                ([-8.0, 2.0, 0.0], [0.0, fletch_top]),
            ],
            Vec3::Z,
        );
    }
    quads.finish()
}

/// A camera-facing quad one unit square before `scale`, its bottom edge
/// `drop` below the body's position, showing the texture rectangle
/// `(u0, v0)..(u1, v1)`.
fn billboard_mesh(scale: f32, drop: f32, [u0, v0, u1, v1]: [f32; 4]) -> Mesh {
    let mut quads = Quads::new();
    quads.quad(
        Mat4::from_scale(Vec3::splat(scale)),
        [
            ([-0.5, -drop, 0.0], [u0, v1]),
            ([0.5, -drop, 0.0], [u1, v1]),
            ([0.5, 1.0 - drop, 0.0], [u1, v0]),
            ([-0.5, 1.0 - drop, 0.0], [u0, v0]),
        ],
        Vec3::Y,
    );
    quads.finish()
}

/// The rectangle of tile `tile` in the 16×16 grid of `gui/items.png`.
fn icon_rect(tile: u8) -> [f32; 4] {
    let (u0, v0) = (f32::from(tile % 16) / 16.0, f32::from(tile / 16) / 16.0);
    [u0, v0, u0 + 1.0 / 16.0, v0 + 1.0 / 16.0]
}

/// `RenderFireball`'s quad, twice the icon's size, centered half a block up.
pub fn fireball_mesh(tile: u8) -> Mesh {
    billboard_mesh(2.0, 0.25, icon_rect(tile))
}

/// `RenderSnowball`'s quad for a thrown snowball or egg: the item's icon at
/// half size.
pub fn thrown_mesh(tile: u8) -> Mesh {
    billboard_mesh(0.5, 0.25, icon_rect(tile))
}

/// `RenderFish`'s bobber: the 8-pixel tile at column 1, row 2 of the 128-pixel
/// `particles.png`, half a block across and centered on the bobber.
pub fn bobber_mesh() -> Mesh {
    billboard_mesh(
        0.5,
        0.5,
        [8.0 / 128.0, 16.0 / 128.0, 16.0 / 128.0, 24.0 / 128.0],
    )
}

/// The points of `RenderFish`'s line from a bobber at `bobber` to the rod at
/// `tip`: straight in plan, sagging as `(t² + t) / 2` in height.
pub fn fishing_line_points(bobber: Vec3, tip: Vec3) -> [Vec3; LINE_SEGMENTS as usize + 1] {
    let delta = tip - (bobber + Vec3::Y * 0.25);
    std::array::from_fn(|index| {
        let t = index as f32 / f32::from(LINE_SEGMENTS);
        bobber + Vec3::new(delta.x * t, delta.y * (t * t + t) * 0.5 + 0.25, delta.z * t)
    })
}

pub(super) fn add_projectile_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cached: Local<Option<ProjectileMeshes>>,
    arrows: Query<Entity, Added<Arrow>>,
    fireballs: Query<Entity, Added<Fireball>>,
    thrown: Query<(Entity, &Thrown), Added<Thrown>>,
    bobbers: Query<Entity, Added<Bobber>>,
) {
    let icon = |item: Item| item_tile(item.as_u16(), 0).unwrap_or(0);
    let cached = cached.get_or_insert_with(|| ProjectileMeshes {
        arrow: meshes.add(arrow_mesh()),
        fireball: meshes.add(fireball_mesh(icon(Item::Snowball))),
        snowball: meshes.add(thrown_mesh(icon(Item::Snowball))),
        egg: meshes.add(thrown_mesh(icon(Item::Egg))),
        bobber: meshes.add(bobber_mesh()),
        line: meshes.add(Cuboid::new(LINE_WIDTH, LINE_WIDTH, 1.0)),
    });
    let mut attach = |entity: Entity, mesh: &Handle<Mesh>, skin: &str| {
        if let Some(material) = assets.skins.get(skin) {
            commands.spawn((
                ProjectileModel,
                Mesh3d(mesh.clone()),
                MeshMaterial3d::<CreatureMaterial>(material.clone()),
                MeshTag::default(),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
            ));
        }
    };
    for entity in &arrows {
        attach(entity, &cached.arrow, "item/arrows.png");
    }
    for entity in &fireballs {
        attach(entity, &cached.fireball, "gui/items.png");
    }
    for (entity, thrown) in &thrown {
        let mesh = match thrown.kind {
            ThrownKind::Snowball => &cached.snowball,
            ThrownKind::Egg => &cached.egg,
        };
        attach(entity, mesh, "gui/items.png");
    }
    let lines: Vec<Entity> = bobbers
        .iter()
        .inspect(|&entity| attach(entity, &cached.bobber, "particles.png"))
        .collect();
    for entity in lines {
        let Some(material) = assets.skins.get("particles.png") else {
            continue;
        };
        for segment in 0..LINE_SEGMENTS {
            commands.spawn((
                LineSegment(segment),
                Mesh3d(cached.line.clone()),
                MeshMaterial3d::<CreatureMaterial>(material.clone()),
                // Drawn black, as Beta's untextured line is.
                creature_tag(0.0, false, 0.0, 1.0),
                Transform::default(),
                Visibility::Hidden,
                ChildOf(entity),
            ));
        }
    }
}

/// Stretch each bobber's line segments from it to the angler's hand.
pub(super) fn pose_fishing_lines(
    tick: Res<WorldTick>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    bobbers: Query<(&Transform, &PreviousTick, &Children), With<Bobber>>,
    mut segments: Query<
        (&LineSegment, &mut Transform, &mut Visibility),
        (Without<Bobber>, Without<ProjectileModel>),
    >,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let tip = camera.transform_point(ROD_TIP);
    for (body, previous, children) in &bobbers {
        let shown = previous.0.lerp(body.translation, tick.partial());
        let points = fishing_line_points(shown, tip);
        for child in children.iter() {
            let Ok((segment, mut pose, mut visibility)) = segments.get_mut(child) else {
                continue;
            };
            let (from, to) = (points[segment.0 as usize], points[segment.0 as usize + 1]);
            let span = to - from;
            let length = span.length();
            if length < 1e-4 {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
            visibility.set_if_neq(Visibility::Inherited);
            pose.set_if_neq(
                Transform::from_translation((from + to) * 0.5 - body.translation)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, span / length))
                    .with_scale(Vec3::new(1.0, 1.0, length)),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_projectiles(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    environment: crate::world::dimension::Environment,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    bodies: Query<
        (&Transform, &PreviousTick, Option<&Arrow>, &Children),
        Or<(With<Arrow>, With<Fireball>, With<Thrown>, With<Bobber>)>,
    >,
    mut models: Query<
        (&mut Transform, &mut MeshTag),
        (With<ProjectileModel>, Without<PreviousTick>),
    >,
) {
    let partial = tick.partial();
    let lerp = |from: f32, to: f32| from + (to - from) * partial;
    let subtracted = environment.skylight_subtracted(partial);
    let ambient = environment.ambient_light();
    let facing = camera
        .single()
        .map_or(Quat::IDENTITY, |camera| camera.rotation());
    for (body, previous, arrow, children) in &bodies {
        let rotation = if let Some(arrow) = arrow {
            let shake = f32::from(arrow.shake) - partial;
            let quiver = if shake > 0.0 {
                -(shake * 3.0).sin() * shake
            } else {
                0.0
            };
            Quat::from_rotation_y((lerp(arrow.prev_yaw, arrow.yaw) - 90.0).to_radians())
                * Quat::from_rotation_z(lerp(arrow.prev_pitch, arrow.pitch).to_radians())
                * Quat::from_rotation_z(quiver.to_radians())
        } else {
            facing
        };
        let slide = previous.0.lerp(body.translation, partial) - body.translation;
        let brightness = entity_brightness(
            &chunks,
            light.as_deref(),
            body.translation,
            0.5,
            subtracted,
            ambient,
        );
        for child in children.iter() {
            if let Ok((mut pose, mut tag)) = models.get_mut(child) {
                pose.set_if_neq(Transform::from_translation(slide).with_rotation(rotation));
                tag.set_if_neq(creature_tag(brightness, false, 0.0, 1.0));
            }
        }
    }
}

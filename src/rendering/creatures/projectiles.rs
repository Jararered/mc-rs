//! `RenderArrow` and `RenderFireball`.
//!
//! An arrow is a crossed pair of shaft quads and a fletching cross from
//! `item/arrows.png`, turned to its flight and quivering after it strikes. A
//! fireball is the snowball icon from `gui/items.png`, twice its size and
//! always facing the camera.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use super::CreatureAssets;
use super::CreatureMaterial;
use super::creature_tag;
use super::entity_brightness;
use crate::app::settings::GameSettings;
use crate::entity::PreviousTick;
use crate::entity::projectiles::Arrow;
use crate::entity::projectiles::Fireball;
use crate::item::ItemId;
use crate::player::PlayerCamera;
use crate::rendering::appearance::item_tile;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::environment::skylight_subtracted;
use crate::world::lighting::LightCache;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;

/// The mesh drawn for an arrow or fireball, as a child of its body.
#[derive(Component)]
pub(super) struct ProjectileModel;

#[derive(Resource)]
pub(super) struct ProjectileMeshes {
    arrow: Handle<Mesh>,
    fireball: Handle<Mesh>,
}

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

/// `RenderFireball`'s quad, twice the icon's size, centered half a block up.
pub fn fireball_mesh(tile: u8) -> Mesh {
    let mut quads = Quads::new();
    let (u0, v0) = (f32::from(tile % 16) / 16.0, f32::from(tile / 16) / 16.0);
    let (u1, v1) = (u0 + 1.0 / 16.0, v0 + 1.0 / 16.0);
    quads.quad(
        Mat4::from_scale(Vec3::splat(2.0)),
        [
            ([-0.5, -0.25, 0.0], [u0, v1]),
            ([0.5, -0.25, 0.0], [u1, v1]),
            ([0.5, 0.75, 0.0], [u1, v0]),
            ([-0.5, 0.75, 0.0], [u0, v0]),
        ],
        Vec3::Y,
    );
    quads.finish()
}

pub(super) fn add_projectile_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cached: Local<Option<ProjectileMeshes>>,
    arrows: Query<Entity, Added<Arrow>>,
    fireballs: Query<Entity, Added<Fireball>>,
) {
    let cached = cached.get_or_insert_with(|| ProjectileMeshes {
        arrow: meshes.add(arrow_mesh()),
        fireball: meshes.add(fireball_mesh(
            item_tile(ItemId::Snowball.as_u16(), 0).unwrap_or(0),
        )),
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
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pose_projectiles(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    weather: Option<Res<WorldWeather>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    bodies: Query<
        (
            &Transform,
            &PreviousTick,
            Option<&Arrow>,
            Option<&Fireball>,
            &Children,
        ),
        Or<(With<Arrow>, With<Fireball>)>,
    >,
    mut models: Query<
        (&mut Transform, &mut MeshTag),
        (With<ProjectileModel>, Without<PreviousTick>),
    >,
) {
    let partial = tick.partial();
    let lerp = |from: f32, to: f32| from + (to - from) * partial;
    let subtracted = skylight_subtracted(celestial_angle(tick.world_time(), partial))
        .saturating_add(
            weather
                .as_ref()
                .map_or(0, |weather| weather.skylight_penalty()),
        )
        .min(15);
    let facing = camera
        .single()
        .map_or(Quat::IDENTITY, |camera| camera.rotation());
    for (body, previous, arrow, fireball, children) in &bodies {
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
        } else if fireball.is_some() {
            facing
        } else {
            continue;
        };
        let slide = previous.0.lerp(body.translation, partial) - body.translation;
        let brightness = if settings.old_lighting {
            entity_brightness(&chunks, light.as_deref(), body.translation, 0.5, subtracted)
        } else {
            1.0
        };
        for child in children.iter() {
            if let Ok((mut pose, mut tag)) = models.get_mut(child) {
                pose.set_if_neq(Transform::from_translation(slide).with_rotation(rotation));
                tag.set_if_neq(creature_tag(brightness, false, 0.0, 1.0));
            }
        }
    }
}

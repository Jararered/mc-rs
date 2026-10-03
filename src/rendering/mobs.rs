//! Placeholder cuboid models for hostile mobs, and projectile and primed TNT
//! meshes. Meshes and materials are shared between instances. Creatures use
//! Beta's own models in [`super::creatures`].
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use std::collections::HashMap;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobKind;
use crate::entity::mobs::MobProjectile;
use crate::entity::mobs::PrimedTnt;
use crate::world::tick::WorldTick;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct CuboidKey {
    dims: [u8; 3],
    uv: [u8; 2],
}

#[derive(Resource)]
struct MobAssets {
    meshes: HashMap<CuboidKey, Handle<Mesh>>,
    materials: HashMap<&'static str, Handle<StandardMaterial>>,
    arrow_mesh: Handle<Mesh>,
    fireball_mesh: Handle<Mesh>,
    tnt_mesh: Handle<Mesh>,
    arrow: Handle<StandardMaterial>,
    fireball: Handle<StandardMaterial>,
    tnt: Handle<StandardMaterial>,
}

#[derive(Component)]
struct MobPart {
    owner: Entity,
    index: usize,
    pivot: Vec3,
    swing: f32,
    eyes: bool,
    charged: bool,
}

#[derive(Clone, Copy)]
struct Part {
    dims: [u8; 3],
    uv: [u8; 2],
    at: Vec3,
    swing: f32,
    eyes: bool,
    charged: bool,
}

impl Part {
    fn new(dims: [u8; 3], uv: [u8; 2], at: Vec3, swing: f32) -> Self {
        Self {
            dims,
            uv,
            at,
            swing,
            eyes: false,
            charged: false,
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_asset::<StandardMaterial>()
        .add_systems(Startup, prepare_mob_assets)
        .add_systems(
            Update,
            (add_mob_models, add_projectile_models, animate_mob_models)
                .chain()
                .run_if(in_state(AppScreen::Playing)),
        );
}

fn prepare_mob_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Res<AssetServer>,
    settings: Res<GameSettings>,
) {
    let mut lookup = HashMap::new();
    for kind in MobKind::ALL.into_iter().filter(|kind| kind.hostile()) {
        let sample = Mob::new(kind, 1);
        for texture in [
            kind.texture(&sample),
            "mob/ghast_fire.png",
            "mob/spider_eyes.png",
        ] {
            if lookup.contains_key(texture) {
                continue;
            }
            let available = std::path::Path::new("assets").join(texture).exists();
            lookup.insert(
                texture,
                materials.add(StandardMaterial {
                    base_color: if available {
                        Color::WHITE
                    } else {
                        fallback_color(kind)
                    },
                    base_color_texture: available.then(|| server.load(texture.to_owned())),
                    unlit: settings.old_lighting || texture.ends_with("spider_eyes.png"),
                    alpha_mode: if texture.ends_with("slime.png")
                        || texture.ends_with("spider_eyes.png")
                    {
                        AlphaMode::Blend
                    } else {
                        AlphaMode::Mask(0.5)
                    },
                    double_sided: true,
                    cull_mode: None,
                    perceptual_roughness: 1.0,
                    ..default()
                }),
            );
        }
    }
    lookup.insert(
        "creeper_charge",
        materials.add(StandardMaterial {
            base_color: Color::srgba(0.35, 0.65, 1.0, 0.45),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            ..default()
        }),
    );
    // Populate shared meshes once per distinct face layout, not per entity.
    let mut built = HashMap::new();
    for kind in MobKind::ALL {
        for part in parts(kind) {
            let key = CuboidKey {
                dims: part.dims,
                uv: part.uv,
            };
            built.entry(key).or_insert_with(|| meshes.add(cuboid(key)));
        }
    }
    commands.insert_resource(MobAssets {
        meshes: built,
        materials: lookup,
        arrow_mesh: meshes.add(Cuboid::new(0.06, 0.06, 0.6)),
        fireball_mesh: meshes.add(Sphere::new(0.25)),
        tnt_mesh: meshes.add(Cuboid::new(0.98, 0.98, 0.98)),
        arrow: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(156, 125, 82),
            ..default()
        }),
        fireball: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(255, 98, 18),
            emissive: LinearRgba::new(5.0, 1.0, 0.1, 1.0),
            ..default()
        }),
        tnt: materials.add(StandardMaterial {
            base_color: Color::srgb_u8(190, 53, 42),
            ..default()
        }),
    });
}

fn add_projectile_models(
    mut commands: Commands,
    assets: Res<MobAssets>,
    projectiles: Query<(Entity, &MobProjectile), Added<MobProjectile>>,
    tnt: Query<Entity, Added<PrimedTnt>>,
) {
    for (entity, projectile) in &projectiles {
        let (mesh, material) = if projectile.fireball {
            (&assets.fireball_mesh, &assets.fireball)
        } else {
            (&assets.arrow_mesh, &assets.arrow)
        };
        commands
            .entity(entity)
            .insert((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
    }
    for entity in &tnt {
        commands.entity(entity).insert((
            Mesh3d(assets.tnt_mesh.clone()),
            MeshMaterial3d(assets.tnt.clone()),
        ));
    }
}

fn fallback_color(kind: MobKind) -> Color {
    match kind {
        MobKind::Creeper => Color::srgb_u8(81, 169, 60),
        MobKind::Slime => Color::srgb_u8(98, 194, 88),
        MobKind::Zombie | MobKind::PigZombie => Color::srgb_u8(86, 130, 76),
        MobKind::Spider => Color::srgb_u8(52, 45, 40),
        _ => Color::srgb_u8(209, 209, 206),
    }
}

fn parts(kind: MobKind) -> Vec<Part> {
    let p = Part::new;
    match kind {
        MobKind::Spider => {
            let mut result = vec![
                p([10, 8, 12], [0, 12], Vec3::new(0., 0.47, 0.22), 0.),
                p([8, 8, 8], [32, 4], Vec3::new(0., 0.52, -0.43), 0.),
                p([12, 12, 12], [0, 0], Vec3::new(0., 0.48, 0.7), 0.),
            ];
            let mut eyes = p([8, 8, 8], [32, 4], Vec3::new(0., 0.52, -0.43), 0.);
            eyes.eyes = true;
            result.push(eyes);
            for side in [-1., 1.] {
                for row in 0..4 {
                    result.push(p(
                        [12, 2, 2],
                        [18, 0],
                        Vec3::new(side * 0.78, 0.43, (row as f32 - 1.5) * 0.33),
                        side,
                    ));
                }
            }
            result
        }
        MobKind::Ghast => {
            let mut result = vec![p([16, 16, 16], [0, 0], Vec3::new(0., 2.0, 0.), 0.)];
            for i in 0..9 {
                result.push(p(
                    [2, 10 + i as u8 % 5, 2],
                    [0, 0],
                    Vec3::new((i % 3) as f32 - 1., 0.5, (i / 3) as f32 - 1.),
                    1.,
                ));
            }
            result
        }
        MobKind::Slime => vec![p([16, 16, 16], [0, 0], Vec3::new(0., 0.5, 0.), 0.)],
        MobKind::Zombie | MobKind::Skeleton | MobKind::PigZombie | MobKind::Creeper => {
            let mut result = vec![
                p([8, 8, 8], [0, 0], Vec3::new(0., 1.55, 0.), 0.),
                p([8, 12, 4], [16, 16], Vec3::new(0., 1.08, 0.), 0.),
            ];
            if kind != MobKind::Creeper {
                for x in [-0.38, 0.38] {
                    result.push(p(
                        [4, 12, 4],
                        [40, 16],
                        Vec3::new(x, 1.05, 0.),
                        if x < 0. { -1. } else { 1. },
                    ));
                }
            }
            for x in [-0.15, 0.15] {
                result.push(p(
                    if kind == MobKind::Creeper {
                        [4, 6, 4]
                    } else {
                        [4, 12, 4]
                    },
                    [0, 16],
                    Vec3::new(x, if kind == MobKind::Creeper { 0.19 } else { 0.38 }, 0.),
                    if x < 0. { 1. } else { -1. },
                ));
            }
            if kind == MobKind::Creeper {
                for (dims, uv, at) in [
                    ([9, 9, 9], [0, 0], Vec3::new(0., 1.55, 0.)),
                    ([9, 13, 5], [16, 16], Vec3::new(0., 1.08, 0.)),
                ] {
                    let mut charge = p(dims, uv, at, 0.);
                    charge.charged = true;
                    result.push(charge);
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

fn add_mob_models(
    mut commands: Commands,
    assets: Res<MobAssets>,
    mobs: Query<(Entity, &Mob), (Added<Mob>, Without<Living>)>,
) {
    for (entity, mob) in &mobs {
        let parts = parts(mob.kind);
        let texture = mob.kind.texture(mob);
        let Some(material) = assets.materials.get(texture) else {
            continue;
        };
        commands.entity(entity).with_children(|parent| {
            for (index, part) in parts.iter().enumerate() {
                let key = CuboidKey {
                    dims: part.dims,
                    uv: part.uv,
                };
                let selected = if part.eyes {
                    assets
                        .materials
                        .get("mob/spider_eyes.png")
                        .unwrap_or(material)
                } else if part.charged {
                    assets.materials.get("creeper_charge").unwrap_or(material)
                } else {
                    material
                };
                parent.spawn((
                    Mesh3d(assets.meshes[&key].clone()),
                    MeshMaterial3d(selected.clone()),
                    MobPart {
                        owner: entity,
                        index,
                        pivot: part.at,
                        swing: part.swing,
                        eyes: part.eyes,
                        charged: part.charged,
                    },
                    Transform::from_translation(part.at),
                    if part.charged && !mob.charged {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    },
                ));
            }
        });
    }
}

fn animate_mob_models(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    mobs: Query<(&Mob, &Transform, &Velocity, &PreviousTick), Without<MobPart>>,
    mut parts: Query<
        (
            &MobPart,
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<Mob>,
    >,
    assets: Res<MobAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if settings.is_changed() {
        for (name, handle) in &assets.materials {
            if *name != "creeper_charge"
                && *name != "mob/spider_eyes.png"
                && let Some(mut material) = materials.get_mut(handle)
            {
                material.unlit = settings.old_lighting;
            }
        }
    }
    for (part, mut pose, mut visibility, mut material) in &mut parts {
        let Ok((mob, root, velocity, previous)) = mobs.get(part.owner) else {
            continue;
        };
        if part.charged {
            *visibility = if !mob.charged {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        if !part.eyes && !part.charged {
            if let Some(new) = assets.materials.get(mob.kind.texture(mob)) {
                if material.0 != *new {
                    material.0 = new.clone();
                }
            }
        }
        let walk = (root.translation - previous.0).length() * 12.
            + mob.age as f32 * 0.35
            + tick.partial() * 0.35;
        let moving = velocity.0.length().min(4.) / 4.;
        pose.translation = part.pivot;
        pose.rotation = Quat::from_rotation_x(walk.cos() * part.swing * moving * 0.55);
        if mob.kind == MobKind::Slime && part.index == 0 {
            pose.scale = Vec3::splat(f32::from(mob.variant.max(1)) * 0.6);
        } else if mob.kind == MobKind::Ghast {
            pose.scale = Vec3::splat(4.0);
        }
    }
}

/// ModelRenderer's six unfolded box faces on the 64x32 Beta skin sheet.
fn cuboid(key: CuboidKey) -> Mesh {
    let [w, h, d] = key.dims.map(f32::from);
    let [u, v] = key.uv.map(f32::from);
    let (x, y, z) = (w / 32., h / 32., d / 32.);
    let faces: [([[f32; 3]; 4], [f32; 3], [f32; 4]); 6] = [
        (
            [[-x, -y, z], [x, -y, z], [x, y, z], [-x, y, z]],
            [0., 0., 1.],
            [u + d, v + d, u + d + w, v + d + h],
        ),
        (
            [[x, -y, -z], [-x, -y, -z], [-x, y, -z], [x, y, -z]],
            [0., 0., -1.],
            [u + d + w + d, v + d, u + d + w + d + w, v + d + h],
        ),
        (
            [[x, -y, z], [x, -y, -z], [x, y, -z], [x, y, z]],
            [1., 0., 0.],
            [u + d + w, v + d, u + d + w + d, v + d + h],
        ),
        (
            [[-x, -y, -z], [-x, -y, z], [-x, y, z], [-x, y, -z]],
            [-1., 0., 0.],
            [u, v + d, u + d, v + d + h],
        ),
        (
            [[-x, y, z], [x, y, z], [x, y, -z], [-x, y, -z]],
            [0., 1., 0.],
            [u + d, v, u + d + w, v + d],
        ),
        (
            [[-x, -y, -z], [x, -y, -z], [x, -y, z], [-x, -y, z]],
            [0., -1., 0.],
            [u + d + w, v, u + d + w + w, v + d],
        ),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uv = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (corners, normal, rect) in faces {
        let base = positions.len() as u32;
        positions.extend(corners);
        normals.extend([normal; 4]);
        let [l, t, r, b] = rect;
        uv.extend([
            [l / 64., b / 32.],
            [r / 64., b / 32.],
            [r / 64., t / 32.],
            [l / 64., t / 32.],
        ]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_indices(Indices::U32(indices))
}

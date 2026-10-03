//! Articulated cuboid mob models. Meshes and materials are shared between instances.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use std::collections::HashMap;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
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
    wool: Vec<Handle<StandardMaterial>>,
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
    fur: bool,
    saddle: bool,
    eyes: bool,
    charged: bool,
}

#[derive(Clone, Copy)]
struct Part {
    dims: [u8; 3],
    uv: [u8; 2],
    at: Vec3,
    swing: f32,
    fur: bool,
    saddle: bool,
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
            fur: false,
            saddle: false,
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
    for kind in MobKind::ALL {
        let sample = Mob::new(kind, 1);
        for texture in [
            kind.texture(&sample),
            "mob/wolf_angry.png",
            "mob/wolf_tame.png",
            "mob/ghast_fire.png",
            "mob/sheep_fur.png",
            "mob/saddle.png",
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
    // The old wool texture is white. EntitySheep.fleeceColorTable tints its layer.
    const COLORS: [[u8; 3]; 16] = [
        [255, 255, 255],
        [243, 178, 51],
        [229, 127, 217],
        [153, 178, 242],
        [229, 229, 51],
        [127, 204, 25],
        [242, 178, 204],
        [76, 76, 76],
        [153, 153, 153],
        [76, 153, 178],
        [178, 102, 229],
        [51, 102, 204],
        [127, 102, 76],
        [102, 127, 51],
        [204, 76, 76],
        [25, 25, 25],
    ];
    let wool = COLORS
        .into_iter()
        .map(|[r, g, b]| {
            materials.add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                base_color_texture: std::path::Path::new("assets/mob/sheep_fur.png")
                    .exists()
                    .then(|| server.load("mob/sheep_fur.png")),
                unlit: settings.old_lighting,
                alpha_mode: AlphaMode::Mask(0.5),
                double_sided: true,
                ..default()
            })
        })
        .collect();
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
        wool,
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
        MobKind::Wolf => Color::srgb_u8(159, 152, 136),
        MobKind::Pig => Color::srgb_u8(237, 167, 175),
        MobKind::Chicken => Color::srgb_u8(236, 230, 216),
        MobKind::Ghast | MobKind::Sheep | MobKind::Skeleton => Color::srgb_u8(209, 209, 206),
        _ => Color::srgb_u8(124, 93, 74),
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
        MobKind::Squid => {
            let mut result = vec![p([12, 16, 12], [0, 0], Vec3::new(0., 0.85, 0.), 0.)];
            for i in 0..8 {
                let angle = i as f32 * std::f32::consts::TAU / 8.;
                result.push(p(
                    [2, 18, 2],
                    [48, 0],
                    Vec3::new(angle.cos() * 0.32, -0.08, angle.sin() * 0.32),
                    1.,
                ));
            }
            result
        }
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
        _ => {
            let (head, body, leg, height) = match kind {
                MobKind::Chicken => ([4, 6, 3], [6, 8, 6], [2, 4, 2], 0.55),
                MobKind::Wolf => ([6, 6, 4], [6, 9, 6], [2, 8, 2], 0.85),
                MobKind::Sheep => ([6, 6, 8], [8, 16, 6], [4, 6, 4], 1.3),
                MobKind::Cow => ([8, 8, 6], [10, 16, 8], [4, 12, 4], 1.3),
                _ => ([8, 8, 8], [10, 16, 8], [4, 6, 4], 0.9),
            };
            let mut result = vec![
                p(head, [0, 0], Vec3::new(0., height - 0.27, -0.48), 0.),
                p(body, [28, 8], Vec3::new(0., height - 0.55, 0.12), 0.),
            ];
            for x in [-0.31, 0.31] {
                for z in [-0.31, 0.35] {
                    result.push(p(
                        leg,
                        [0, 16],
                        Vec3::new(x, 0.22, z),
                        if x * z > 0. { 1. } else { -1. },
                    ));
                }
            }
            if kind == MobKind::Sheep {
                let mut fur = p(
                    [12, 18, 10],
                    [28, 8],
                    Vec3::new(0., height - 0.55, 0.12),
                    0.,
                );
                fur.fur = true;
                result.push(fur);
                let mut head = p([8, 8, 10], [0, 0], Vec3::new(0., height - 0.27, -0.48), 0.);
                head.fur = true;
                result.push(head);
                for x in [-0.31, 0.31] {
                    for z in [-0.31, 0.35] {
                        let mut leg = p([6, 8, 6], [0, 16], Vec3::new(x, 0.22, z), 0.);
                        leg.fur = true;
                        result.push(leg);
                    }
                }
            }
            if kind == MobKind::Pig {
                let mut saddle = p(body, [28, 8], Vec3::new(0., height - 0.55, 0.12), 0.);
                saddle.saddle = true;
                result.push(saddle);
            }
            result
        }
    }
}

fn add_mob_models(
    mut commands: Commands,
    assets: Res<MobAssets>,
    mobs: Query<(Entity, &Mob), Added<Mob>>,
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
                let selected = if part.fur {
                    &assets.wool[mob.variant.min(15) as usize]
                } else if part.saddle {
                    assets.materials.get("mob/saddle.png").unwrap_or(material)
                } else if part.eyes {
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
                        fur: part.fur,
                        saddle: part.saddle,
                        eyes: part.eyes,
                        charged: part.charged,
                    },
                    Transform::from_translation(part.at),
                    if part.fur && mob.sheared
                        || part.saddle && !mob.saddled
                        || part.charged && !mob.charged
                    {
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
        for material in &assets.wool {
            if let Some(mut material) = materials.get_mut(material) {
                material.unlit = settings.old_lighting;
            }
        }
    }
    for (part, mut pose, mut visibility, mut material) in &mut parts {
        let Ok((mob, root, velocity, previous)) = mobs.get(part.owner) else {
            continue;
        };
        if part.fur || part.saddle || part.charged {
            *visibility = if part.fur && mob.sheared
                || part.saddle && !mob.saddled
                || part.charged && !mob.charged
            {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        if !part.fur && !part.saddle && !part.eyes && !part.charged {
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

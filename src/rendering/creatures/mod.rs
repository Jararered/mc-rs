//! Creature models posed each frame like `RenderLiving`.
//!
//! A creature's root entity stays at its simulated feet. Its one model child
//! carries this frame's slide between ticks and `RenderLiving`'s turn, flip,
//! and lift; that child's children are the model's boxes, re-posed from the
//! creature's [`Living`] state. Meshes and materials are shared between
//! creatures, and per-creature brightness rides in each box's `MeshTag`.

pub mod models;
mod shading;

use std::collections::HashMap;

use bevy::camera::visibility::VisibilitySystems;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::creature::Living;
use crate::entity::creature::Swim;
use crate::entity::creature::Wings;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobKind;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::environment::skylight_subtracted;
use crate::world::lighting::LightCache;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::combined_light;
use crate::world::lighting::light_level_at;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;
use models::Cuboid;
use models::Layer;
use models::Part;
use models::PoseInput;
pub use shading::CreatureMaterial;
pub use shading::CreatureShading;
pub use shading::creature_tag;

const CREATURES: [MobKind; 6] = [
    MobKind::Pig,
    MobKind::Cow,
    MobKind::Sheep,
    MobKind::Chicken,
    MobKind::Squid,
    MobKind::Wolf,
];

/// Skins with a flat color to stand in when the reference texture is absent.
const SKINS: [(&str, [u8; 3]); 10] = [
    ("mob/pig.png", [237, 167, 175]),
    ("mob/saddle.png", [112, 72, 40]),
    ("mob/cow.png", [124, 93, 74]),
    ("mob/sheep.png", [214, 196, 180]),
    ("mob/sheep_fur.png", [235, 235, 235]),
    ("mob/chicken.png", [236, 230, 216]),
    ("mob/squid.png", [52, 70, 95]),
    ("mob/wolf.png", [159, 152, 136]),
    ("mob/wolf_tame.png", [159, 152, 136]),
    ("mob/wolf_angry.png", [159, 152, 136]),
];

/// `EntitySheep.fleeceColorTable`, by wool color.
const FLEECE: [[f32; 3]; 16] = [
    [1.0, 1.0, 1.0],
    [0.95, 0.7, 0.2],
    [0.9, 0.5, 0.85],
    [0.6, 0.7, 0.95],
    [0.9, 0.9, 0.2],
    [0.5, 0.8, 0.1],
    [0.95, 0.7, 0.8],
    [0.3, 0.3, 0.3],
    [0.6, 0.6, 0.6],
    [0.3, 0.6, 0.7],
    [0.7, 0.4, 0.9],
    [0.2, 0.4, 0.8],
    [0.5, 0.4, 0.3],
    [0.4, 0.5, 0.2],
    [0.8, 0.3, 0.3],
    [0.1, 0.1, 0.1],
];

#[derive(Resource)]
struct CreatureAssets {
    models: HashMap<MobKind, Vec<(Part, Handle<Mesh>)>>,
    materials: HashMap<&'static str, Handle<CreatureMaterial>>,
}

/// The posed frame between a creature's feet and its boxes.
#[derive(Component)]
struct CreatureModel {
    owner: Entity,
}

/// One box of a creature's model, by its index in [`models::model`].
#[derive(Component)]
struct CreaturePart(usize);

pub(super) fn plugin(app: &mut App) {
    shading::plugin(app);
    app.add_systems(Startup, prepare_creature_assets)
        .add_systems(
            PostUpdate,
            (add_creature_models, pose_creatures, apply_creature_lighting)
                .chain()
                .before(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate)
                .run_if(in_state(AppScreen::Playing)),
        );
}

fn prepare_creature_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CreatureMaterial>>,
    server: Res<AssetServer>,
    settings: Res<GameSettings>,
) {
    // Legs and mirrored parts share boxes, so build each distinct box once.
    let mut built: Vec<(Cuboid, Handle<Mesh>)> = Vec::new();
    let mut mesh_for = |cuboid: Cuboid| {
        if let Some((_, mesh)) = built.iter().find(|(known, _)| *known == cuboid) {
            return mesh.clone();
        }
        let mesh = meshes.add(models::cuboid_mesh(&cuboid));
        built.push((cuboid, mesh.clone()));
        mesh
    };
    let models = CREATURES
        .into_iter()
        .map(|kind| {
            let parts = models::model(kind)
                .into_iter()
                .map(|part| (part, mesh_for(part.cuboid)))
                .collect();
            (kind, parts)
        })
        .collect();
    let materials = SKINS
        .into_iter()
        .map(|(texture, [r, g, b])| {
            let available = std::path::Path::new("assets").join(texture).exists();
            let material = CreatureMaterial {
                base: StandardMaterial {
                    base_color: if available {
                        Color::WHITE
                    } else {
                        Color::srgb_u8(r, g, b)
                    },
                    base_color_texture: available.then(|| server.load(texture)),
                    unlit: settings.old_lighting,
                    // `RenderLiving` alpha-tests at 0.1 and draws both sides.
                    alpha_mode: AlphaMode::Mask(0.1),
                    cull_mode: None,
                    perceptual_roughness: 1.0,
                    // The brightness is applied by a forward fragment shader.
                    opaque_render_method: OpaqueRendererMethod::Forward,
                    ..default()
                },
                extension: CreatureShading {},
            };
            (texture, materials.add(material))
        })
        .collect();
    commands.insert_resource(CreatureAssets { models, materials });
}

fn layer_texture(mob: &Mob, layer: Layer) -> &'static str {
    match layer {
        Layer::Base => mob.kind.texture(mob),
        Layer::Saddle => "mob/saddle.png",
        Layer::Fleece => "mob/sheep_fur.png",
    }
}

/// `RenderPig.renderSaddledPig` and `RenderSheep.setWoolColorAndRender`.
fn layer_visible(mob: &Mob, layer: Layer) -> bool {
    match layer {
        Layer::Base => true,
        Layer::Saddle => mob.saddled,
        Layer::Fleece => !mob.sheared,
    }
}

fn visibility(visible: bool) -> Visibility {
    if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn add_creature_models(
    mut commands: Commands,
    assets: Res<CreatureAssets>,
    creatures: Query<(Entity, &Mob), Added<Living>>,
) {
    for (entity, mob) in &creatures {
        let Some(parts) = assets.models.get(&mob.kind) else {
            continue;
        };
        let node = commands
            .spawn((
                Name::new("Creature model"),
                CreatureModel { owner: entity },
                Transform::default(),
                Visibility::Inherited,
                ChildOf(entity),
            ))
            .id();
        for (index, (part, mesh)) in parts.iter().enumerate() {
            let Some(material) = assets.materials.get(layer_texture(mob, part.layer)) else {
                continue;
            };
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                creature_tag(Color::WHITE, 1.0),
                Transform::default(),
                visibility(layer_visible(mob, part.layer)),
                CreaturePart(index),
                ChildOf(node),
            ));
        }
    }
}

/// `RenderLiving.doRenderLiving`'s per-frame inputs, interpolated between
/// the last two ticks.
fn pose_creatures(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    weather: Option<Res<WorldWeather>>,
    assets: Res<CreatureAssets>,
    creatures: Query<(
        &Mob,
        &Living,
        &Transform,
        &PreviousTick,
        &EntitySize,
        Option<&Wings>,
        Option<&Swim>,
    )>,
    mut nodes: Query<(&CreatureModel, &mut Transform, &Children), Without<Mob>>,
    mut parts: Query<
        (
            &CreaturePart,
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &mut MeshMaterial3d<CreatureMaterial>,
        ),
        (Without<CreatureModel>, Without<Mob>),
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
    for (model, mut node, children) in &mut nodes {
        let Ok((mob, living, root, previous, size, wings, swim)) = creatures.get(model.owner)
        else {
            continue;
        };
        let Some(definitions) = assets.models.get(&mob.kind) else {
            continue;
        };
        let body_yaw = lerp(living.prev_body_yaw, living.body_yaw);
        let squid = swim.map(|swim| {
            (
                lerp(swim.prev_pitch, swim.pitch),
                lerp(swim.prev_yaw, swim.yaw),
            )
        });
        let mut frame = models::model_transform(body_yaw, squid);
        frame.translation += previous.0.lerp(root.translation, partial) - root.translation;
        node.set_if_neq(frame);

        let special = match (wings, swim) {
            (Some(wings), _) => wings.angle(partial),
            (_, Some(swim)) => lerp(swim.prev_tentacle, swim.tentacle),
            _ if mob.kind == MobKind::Wolf => models::wolf_tail(mob.angry, mob.tamed, mob.health),
            _ => mob.age as f32 + partial,
        };
        let input = PoseInput {
            limb_swing: living.limb_swing - living.limb_amount * (1.0 - partial),
            limb_amount: lerp(living.prev_limb_amount, living.limb_amount).min(1.0),
            special,
            head_yaw: lerp(living.prev_yaw, living.yaw) - body_yaw,
            head_pitch: lerp(living.prev_pitch, living.pitch),
            sitting: mob.sitting,
            angry: mob.angry,
        };
        // Bevy's lights shade lit mode; old lighting applies Beta's brightness.
        let brightness = if settings.old_lighting {
            entity_brightness(
                &chunks,
                light.as_deref(),
                root.translation,
                size.height,
                subtracted,
            )
        } else {
            1.0
        };
        let fleece = FLEECE[usize::from(mob.variant & 15)];
        for child in children.iter() {
            let Ok((part, mut pose, mut shown, mut tag, mut material)) = parts.get_mut(child)
            else {
                continue;
            };
            let Some((definition, _)) = definitions.get(part.0) else {
                continue;
            };
            pose.set_if_neq(models::pose(definition, &input));
            shown.set_if_neq(visibility(layer_visible(mob, definition.layer)));
            let tint = if definition.layer == Layer::Fleece {
                Color::srgb(fleece[0], fleece[1], fleece[2])
            } else {
                Color::WHITE
            };
            tag.set_if_neq(creature_tag(tint, brightness));
            // A wolf's skin follows its temper and owner.
            if let Some(skin) = assets.materials.get(layer_texture(mob, definition.layer))
                && material.0 != *skin
            {
                material.0 = skin.clone();
            }
        }
    }
}

/// `Entity.getEntityBrightness`: the light two-thirds of the way up the body.
fn entity_brightness(
    chunks: &WorldChunks,
    light: Option<&LightCache>,
    feet: Vec3,
    height: f32,
    subtracted: u8,
) -> f32 {
    let x = feet.x.floor() as i32;
    let y = (feet.y + height * 0.66).floor() as i32;
    let z = feet.z.floor() as i32;
    let level = light.and_then(|light| light.channels(x, y, z)).map_or_else(
        || light_level_at(chunks, x, y, z, subtracted),
        |(sky, block)| combined_light(sky, block, subtracted),
    );
    beta_brightness(level)
}

fn apply_creature_lighting(
    settings: Res<GameSettings>,
    assets: Res<CreatureAssets>,
    mut materials: ResMut<Assets<CreatureMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    for handle in assets.materials.values() {
        if let Some(mut material) = materials.get_mut(handle)
            && material.base.unlit != settings.old_lighting
        {
            material.base.unlit = settings.old_lighting;
        }
    }
}

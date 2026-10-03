//! Mob models posed each frame like `RenderLiving`.
//!
//! A mob's root entity stays at its simulated feet. Its one model child
//! carries this frame's slide between ticks and `RenderLiving`'s turn,
//! scale, flip, and lift; that child's children are the model's boxes,
//! re-posed from the mob's [`Living`] state. Meshes and materials are shared
//! between mobs, and per-mob brightness, hurt flashes, and creeper flashes
//! ride in each box's `MeshTag`. Arrows and fireballs are in [`projectiles`].

pub mod models;
mod projectiles;
mod shading;

use std::time::Instant;

use bevy::camera::visibility::VisibilitySystems;
use bevy::image::ImageAddressMode;
use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::material::OpaqueRendererMethod;
use bevy::mesh::MeshTag;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::EntityDiagnostics;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::creature::Bounce;
use crate::entity::creature::Fuse;
use crate::entity::creature::Hover;
use crate::entity::creature::Living;
use crate::entity::creature::Swim;
use crate::entity::creature::Wings;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobKind;
use crate::player::model::mesh::sprite_mesh;
use crate::rendering::appearance::item_tile;
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
use models::Frame;
use models::Layer;
use models::Part;
use models::PoseInput;
use models::Role;
pub use shading::CreatureMaterial;
pub use shading::CreatureShading;
pub use shading::Pass;
pub use shading::creature_tag;

/// Skins with a flat color to stand in when the reference texture is absent.
const SKINS: [(&str, [u8; 3]); 19] = [
    ("mob/pig.png", [237, 167, 175]),
    ("mob/saddle.png", [112, 72, 40]),
    ("mob/cow.png", [124, 93, 74]),
    ("mob/sheep.png", [214, 196, 180]),
    ("mob/chicken.png", [236, 230, 216]),
    ("mob/squid.png", [52, 70, 95]),
    ("mob/wolf.png", [159, 152, 136]),
    ("mob/wolf_tame.png", [159, 152, 136]),
    ("mob/wolf_angry.png", [159, 152, 136]),
    ("mob/zombie.png", [86, 130, 76]),
    ("mob/skeleton.png", [209, 209, 206]),
    ("mob/creeper.png", [81, 169, 60]),
    ("mob/spider.png", [52, 45, 40]),
    ("mob/slime.png", [98, 194, 88]),
    ("mob/ghast.png", [235, 235, 235]),
    ("mob/ghast_fire.png", [235, 235, 235]),
    ("mob/pigzombie.png", [222, 150, 150]),
    ("gui/items.png", [180, 180, 180]),
    ("item/arrows.png", [156, 125, 82]),
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

/// Posing looks up every box's model and skin each frame, so these maps use
/// Bevy's foldhash.
#[derive(Resource)]
struct CreatureAssets {
    models: HashMap<MobKind, Vec<(Part, Handle<Mesh>)>>,
    skins: HashMap<&'static str, Handle<CreatureMaterial>>,
    fleece: Vec<Handle<CreatureMaterial>>,
    eyes: Handle<CreatureMaterial>,
    charge: Handle<CreatureMaterial>,
    slime_outer: Handle<CreatureMaterial>,
    held: HashMap<MobKind, Handle<Mesh>>,
}

impl CreatureAssets {
    fn material(
        &self,
        mob: &Mob,
        layer: Layer,
        hover: Option<&Hover>,
    ) -> Option<Handle<CreatureMaterial>> {
        match layer {
            Layer::Base => {
                let skin = match hover {
                    // `EntityGhast`: the fiery face while about to fire.
                    Some(hover) if hover.attack_counter > 10 => "mob/ghast_fire.png",
                    _ => mob.kind.texture(mob),
                };
                self.skins.get(skin).cloned()
            }
            Layer::Saddle => self.skins.get("mob/saddle.png").cloned(),
            Layer::Fleece => self.fleece.get(usize::from(mob.variant & 15)).cloned(),
            Layer::Eyes => Some(self.eyes.clone()),
            Layer::Charge => Some(self.charge.clone()),
            Layer::SlimeOuter => Some(self.slime_outer.clone()),
        }
    }
}

/// The posed frame between a mob's feet and its boxes.
#[derive(Component)]
struct CreatureModel {
    owner: Entity,
}

/// One box of a mob's model, by its index in [`models::model`].
#[derive(Component)]
struct CreaturePart(usize);

/// The item a mob holds, attached to its right arm.
#[derive(Component)]
struct HeldItem;

pub(super) fn plugin(app: &mut App) {
    shading::plugin(app);
    app.add_systems(Startup, prepare_creature_assets)
        .add_systems(
            PostUpdate,
            (
                add_creature_models,
                pose_creatures,
                projectiles::add_projectile_models,
                projectiles::pose_projectiles,
                apply_creature_lighting,
            )
                .chain()
                .before(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate)
                .run_if(in_state(AppScreen::Playing)),
        );
}

fn material(
    texture: Option<Handle<Image>>,
    color: Color,
    unlit: bool,
    alpha_mode: AlphaMode,
    shading: CreatureShading,
) -> CreatureMaterial {
    CreatureMaterial {
        base: StandardMaterial {
            base_color: color,
            base_color_texture: texture,
            unlit,
            alpha_mode,
            // `RenderLiving` draws both sides of every face.
            cull_mode: None,
            perceptual_roughness: 1.0,
            // The entity lighting is applied by a forward fragment shader.
            opaque_render_method: OpaqueRendererMethod::Forward,
            ..default()
        },
        extension: shading,
    }
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
    let models = MobKind::ALL
        .into_iter()
        .map(|kind| {
            let parts = models::model(kind)
                .into_iter()
                .map(|part| (part, mesh_for(part.cuboid)))
                .collect();
            (kind, parts)
        })
        .collect();
    let held = MobKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let item = models::held_item(kind)?;
            let tile = item_tile(item.as_u16(), 0)?;
            Some((kind, meshes.add(sprite_mesh(tile, [255; 3], false))))
        })
        .collect();
    let unlit = settings.old_lighting;
    let available = |texture: &str| std::path::Path::new("assets").join(texture).exists();
    let load = |texture: &'static str| available(texture).then(|| server.load(texture));
    let skins = SKINS
        .into_iter()
        .map(|(texture, [r, g, b])| {
            let image = load(texture);
            let color = if image.is_some() {
                Color::WHITE
            } else {
                Color::srgb_u8(r, g, b)
            };
            let skin = material(
                image,
                color,
                unlit,
                // `RenderLiving` alpha-tests at 0.1.
                AlphaMode::Mask(0.1),
                CreatureShading::new(Pass::Skin, 0.0),
            );
            (texture, materials.add(skin))
        })
        .collect();
    // `RenderSheep` tints the white fleece texture by the wool color.
    let fur = load("mob/sheep_fur.png");
    let fleece = FLEECE
        .into_iter()
        .map(|[r, g, b]| {
            materials.add(material(
                fur.clone(),
                Color::srgb(r, g, b),
                unlit,
                AlphaMode::Mask(0.1),
                CreatureShading::new(Pass::Skin, 0.0),
            ))
        })
        .collect();
    let eyes = materials.add(material(
        load("mob/spider_eyes.png"),
        Color::WHITE,
        unlit,
        AlphaMode::Blend,
        CreatureShading::new(Pass::Glow, 0.0),
    ));
    // `RenderCreeper` scrolls `power.png` by 0.01 per tick on both axes, so
    // the texture repeats.
    let power = available("armor/power.png").then(|| {
        server
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    ..ImageSamplerDescriptor::nearest()
                });
            })
            .load("armor/power.png")
    });
    let charge_color = if power.is_some() {
        Color::WHITE
    } else {
        Color::srgb(0.35, 0.65, 1.0)
    };
    let charge = materials.add(material(
        power,
        charge_color,
        true,
        AlphaMode::Add,
        CreatureShading::new(Pass::Charge, 0.2),
    ));
    let slime_outer = materials.add(material(
        load("mob/slime.png"),
        Color::WHITE,
        unlit,
        AlphaMode::Blend,
        CreatureShading::new(Pass::Skin, 0.0),
    ));
    commands.insert_resource(CreatureAssets {
        models,
        skins,
        fleece,
        eyes,
        charge,
        slime_outer,
        held,
    });
}

/// `RenderPig.renderSaddledPig`, `RenderSheep.setWoolColorAndRender`, and
/// `RenderCreeper`'s charge pass.
fn layer_visible(mob: &Mob, layer: Layer) -> bool {
    match layer {
        Layer::Base | Layer::Eyes | Layer::SlimeOuter => true,
        Layer::Saddle => mob.saddled,
        Layer::Fleece => !mob.sheared,
        Layer::Charge => mob.charged,
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
    creatures: Query<(Entity, &Mob, Option<&Hover>), Added<Living>>,
) {
    for (entity, mob, hover) in &creatures {
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
            let Some(material) = assets.material(mob, part.layer, hover) else {
                continue;
            };
            let box_entity = commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    MeshTag::default(),
                    Transform::default(),
                    visibility(layer_visible(mob, part.layer)),
                    CreaturePart(index),
                    ChildOf(node),
                ))
                .id();
            if part.role == (Role::ZombieArm { right: true })
                && let (Some(item), Some(mesh), Some(items)) = (
                    models::held_item(mob.kind),
                    assets.held.get(&mob.kind),
                    assets.skins.get("gui/items.png"),
                )
            {
                let full_3d = item == crate::item::ItemId::GoldSword;
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(items.clone()),
                    MeshTag::default(),
                    models::held_item_transform(full_3d),
                    Visibility::Inherited,
                    HeldItem,
                    ChildOf(box_entity),
                ));
            }
        }
    }
}

/// `RenderLiving.doRenderLiving`'s per-frame inputs, interpolated between
/// the last two ticks.
#[allow(clippy::too_many_arguments)]
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
        (
            Option<&Wings>,
            Option<&Swim>,
            Option<&Fuse>,
            Option<&Bounce>,
            Option<&Hover>,
        ),
    )>,
    mut nodes: Query<(&CreatureModel, &mut Transform, &Children), Without<Mob>>,
    mut parts: Query<
        (
            &CreaturePart,
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
            &mut MeshMaterial3d<CreatureMaterial>,
            Option<&Children>,
        ),
        (Without<CreatureModel>, Without<Mob>),
    >,
    mut held: Query<&mut MeshTag, (With<HeldItem>, Without<CreaturePart>)>,
    mut diagnostics: Option<ResMut<EntityDiagnostics>>,
) {
    let start = Instant::now();
    let mut posed = 0;
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
        let Ok((mob, living, root, previous, size, (wings, swim, fuse, bounce, hover))) =
            creatures.get(model.owner)
        else {
            continue;
        };
        let Some(definitions) = assets.models.get(&mob.kind) else {
            continue;
        };
        let body_yaw = lerp(living.prev_body_yaw, living.body_yaw);
        let age = mob.age as f32 + partial;

        // `preRenderCallback`.
        let mut flash = 0.0;
        let scale = match mob.kind {
            MobKind::Creeper => fuse.map_or(Vec3::ONE, |fuse| {
                let near = fuse.flash(mob.fuse, partial);
                // `updateCreeperColorMultiplier`: white on every other tenth.
                if (near * 10.0) as i32 % 2 != 0 {
                    flash = (near * 0.2 * 255.0).clamp(0.0, 255.0).floor() / 255.0;
                }
                let jitter = 1.0 + (near * 100.0).sin() * near * 0.01;
                let swell = near.clamp(0.0, 1.0).powi(4);
                let wide = (1.0 + swell * 0.4) * jitter;
                Vec3::new(wide, (1.0 + swell * 0.1) / jitter, wide)
            }),
            MobKind::Slime => {
                let size = f32::from(mob.variant.max(1));
                let squish = bounce.map_or(0.0, |bounce| lerp(bounce.prev_squish, bounce.squish))
                    / (size * 0.5 + 1.0);
                let across = 1.0 / (squish + 1.0);
                Vec3::new(across * size, size / across, across * size)
            }
            MobKind::Ghast => {
                let charge = hover.map_or(0.0, |hover| {
                    lerp(
                        f32::from(hover.prev_attack_counter),
                        f32::from(hover.attack_counter),
                    ) / 20.0
                });
                let pulse = 1.0 / (charge.max(0.0).powi(5) * 2.0 + 1.0);
                let tall = (8.0 + pulse) / 2.0;
                let wide = (8.0 + 1.0 / pulse) / 2.0;
                Vec3::new(wide, tall, wide)
            }
            _ => Vec3::ONE,
        };
        let death = f32::from(living.death_time) + partial;
        let tilt_max = if mob.kind == MobKind::Spider {
            180.0
        } else {
            90.0
        };
        let mut frame = models::model_transform(Frame {
            body_yaw,
            squid: swim.map(|swim| {
                (
                    lerp(swim.prev_pitch, swim.pitch),
                    lerp(swim.prev_yaw, swim.yaw),
                )
            }),
            scale,
            death_tilt: if living.death_time > 0 {
                models::death_tilt(death, tilt_max)
            } else {
                0.0
            },
        });
        frame.translation += previous.0.lerp(root.translation, partial) - root.translation;
        node.set_if_neq(frame);

        let special = match (wings, swim) {
            (Some(wings), _) => wings.angle(partial),
            (_, Some(swim)) => lerp(swim.prev_tentacle, swim.tentacle),
            _ if mob.kind == MobKind::Wolf => models::wolf_tail(mob.angry, mob.tamed, mob.health),
            _ => age,
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
        let world_brightness = entity_brightness(
            &chunks,
            light.as_deref(),
            root.translation,
            size.height,
            subtracted,
        );
        // Bevy's lights shade lit mode; old lighting applies Beta's brightness.
        let brightness = if settings.old_lighting {
            world_brightness
        } else {
            1.0
        };
        let hurt = living.hurt_time > 0 || living.death_time > 0;
        for child in children.iter() {
            let Ok((part, mut pose, mut shown, mut tag, mut material, grandchildren)) =
                parts.get_mut(child)
            else {
                continue;
            };
            let Some((definition, _)) = definitions.get(part.0) else {
                continue;
            };
            posed += 1;
            pose.set_if_neq(models::pose(definition, &input));
            shown.set_if_neq(visibility(layer_visible(mob, definition.layer)));
            let next = match definition.layer {
                Layer::Base => creature_tag(brightness, hurt, flash, 1.0),
                Layer::Saddle | Layer::Fleece | Layer::SlimeOuter => {
                    creature_tag(brightness, hurt, 0.0, 1.0)
                }
                // `RenderSpider.setSpiderEyeBrightness`.
                Layer::Eyes => creature_tag(1.0, false, 0.0, (1.0 - world_brightness) * 0.5),
                Layer::Charge => MeshTag::default(),
            };
            tag.set_if_neq(next);
            for item in grandchildren.into_iter().flatten() {
                if let Ok(mut item) = held.get_mut(*item) {
                    item.set_if_neq(creature_tag(brightness, hurt, 0.0, 1.0));
                }
            }
            // A wolf's skin follows its temper and owner; a ghast's its aim.
            if let Some(skin) = assets.material(mob, definition.layer, hover)
                && material.0 != skin
            {
                material.0 = skin;
            }
        }
    }
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics.posing.record(start.elapsed());
        diagnostics.parts = posed;
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
    let charge = assets.charge.id();
    let handles = assets
        .skins
        .values()
        .chain(&assets.fleece)
        .chain([&assets.eyes, &assets.slime_outer]);
    for handle in handles {
        if handle.id() != charge
            && let Some(mut material) = materials.get_mut(handle)
            && material.base.unlit != settings.old_lighting
        {
            material.base.unlit = settings.old_lighting;
        }
    }
}

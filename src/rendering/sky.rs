//! Beta sky and distance fog.
//!
//! The sky is the geometry from `RenderGlobal.renderSky`: a flat ceiling 16
//! blocks above the camera, a void floor 16 blocks below, a sunrise fan, the
//! sun, the moon, and a star field. Bevy's cubemap skybox cannot rotate those
//! bodies or fog the ceiling on a shorter range than the world, so this uses
//! `DistanceFog` on a backdrop camera. The horizon is the world's fog line.
//! A dome centered on the camera would pin that line to eye level, so looking
//! down would drag it up the screen.
//!
//! Color math follows `World.getSkyColor`, `WorldProvider.func_4096_a`, and
//! `EntityRenderer.updateFogColor`. Channels are the same 0..=1 values Beta
//! wrote into a framebuffer the monitor treated as sRGB.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::pbr::DistanceFog;
use bevy::pbr::FogFalloff;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::random::JavaRandom;
use crate::rendering::textures::InstanceTint;
use crate::rendering::textures::TintedMaterial;
use crate::rendering::textures::tint_tag;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::light_level_at;

use crate::world::tick::WorldTick;

const SKY_LAYER: usize = 2;
/// `MathHelper` and the fog code use this float, not `std` PI.
const MC_PI: f32 = 3.141_592_7;
const SUN_DISTANCE: f32 = 100.0;
const SUN_SIZE: f32 = 30.0;
const MOON_SIZE: f32 = 20.0;
/// Height of Beta's sky plane (`glSkyList` uses `16`, `glSkyList2` uses `-16`).
const SKY_PLANE_HEIGHT: f32 = 16.0;
const CELESTIAL_LAYER: usize = 3;

/// Marker for the backdrop camera, which has its own fog range.
#[derive(Component)]
pub(crate) struct SkyCamera;

/// Sun, moon, stars, and the sunrise fan. Drawn after the ceiling so the
/// ceiling's depth does not cover them.
#[derive(Component)]
pub(crate) struct CelestialCamera;

#[derive(Component)]
struct SkyAttached;

#[derive(Component)]
pub(crate) struct SkyAnchor;

#[derive(Component)]
struct CelestialRig;

#[derive(Component)]
struct SunriseFan;

#[derive(Component)]
struct StarField;

/// The sun or the moon.
#[derive(Component)]
struct CelestialBody;

/// `EntityRenderer.fogColor1` / `fogColor2`, smoothed once per tick.
#[derive(Resource)]
struct EyeFog {
    previous: f32,
    current: f32,
    ready: bool,
}

impl Default for EyeFog {
    fn default() -> Self {
        Self {
            previous: 1.0,
            current: 1.0,
            ready: false,
        }
    }
}

#[derive(Clone, Resource)]
struct SkyAssets {
    sun: Handle<TintedMaterial>,
    moon: Handle<TintedMaterial>,
    stars: Handle<TintedMaterial>,
    sunrise: Handle<TintedMaterial>,
    sunrise_mesh: Handle<Mesh>,
}

#[derive(SystemParam)]
struct SkyViews<'w, 's> {
    player: Query<'w, 's, &'static GlobalTransform, With<PlayerCamera>>,
    eyes: Query<'w, 's, &'static Transform, With<Player>>,
    player_cameras: Query<
        'w,
        's,
        (&'static mut DistanceFog, &'static mut Projection),
        (
            With<PlayerCamera>,
            Without<SkyCamera>,
            Without<CelestialCamera>,
        ),
    >,
    sky_cameras: Query<
        'w,
        's,
        (
            &'static mut DistanceFog,
            &'static mut Camera,
            &'static mut Projection,
        ),
        (
            With<SkyCamera>,
            Without<PlayerCamera>,
            Without<CelestialCamera>,
        ),
    >,
    celestial_cameras: Query<
        'w,
        's,
        &'static mut Projection,
        (
            With<CelestialCamera>,
            Without<PlayerCamera>,
            Without<SkyCamera>,
        ),
    >,
    anchors: Query<
        'w,
        's,
        &'static mut Transform,
        (
            With<SkyAnchor>,
            Without<CelestialRig>,
            Without<SunriseFan>,
            Without<Player>,
        ),
    >,
    rigs: Query<
        'w,
        's,
        &'static mut Transform,
        (
            With<CelestialRig>,
            Without<SkyAnchor>,
            Without<SunriseFan>,
            Without<Player>,
        ),
    >,
    sunrises: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Visibility,
            &'static mut MeshTag,
        ),
        (
            With<SunriseFan>,
            Without<StarField>,
            Without<SkyAnchor>,
            Without<Player>,
        ),
    >,
    stars: Query<
        'w,
        's,
        (&'static mut Visibility, &'static mut MeshTag),
        (With<StarField>, Without<SunriseFan>),
    >,
    bodies: Query<
        'w,
        's,
        &'static mut MeshTag,
        (With<CelestialBody>, Without<StarField>, Without<SunriseFan>),
    >,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<EyeFog>().add_systems(
        Update,
        (ensure_sky, update_atmosphere)
            .chain()
            .after(super::weather::WeatherVisuals),
    );
}

use crate::world::environment::celestial_angle;
use crate::world::environment::daylight_factor;

/// `World.getStarBrightness`.
pub fn star_brightness(angle: f32) -> f32 {
    let brightness = (1.0 - ((angle * MC_PI * 2.0).cos() * 2.0 + 0.75)).clamp(0.0, 1.0);
    brightness * brightness * 0.5
}

/// Loaded view radius in blocks. Beta's four fog distances are 32, 64, 128, and 256.
pub fn view_distance_blocks(render_chunks: i32) -> f32 {
    (render_chunks.max(1) * 16) as f32
}

/// World pass: fog starts near the view distance edge and is opaque at the end.
/// Beta used `far * 0.25`, but that wastes a quarter of the visible area to a
/// gradual haze at modern render distances. A later start keeps the view clear.
pub fn world_fog_range(far_blocks: f32) -> (f32, f32) {
    (far_blocks * 0.8, far_blocks)
}

/// Sky pass (`setupFog(-1)`): fog starts at the camera and ends earlier than the world.
pub fn sky_fog_end(far_blocks: f32) -> f32 {
    far_blocks * 0.8
}

/// How strongly fog is pulled toward the sky color. Farther views pull harder.
pub fn sky_color_blend(far_blocks: f32) -> f32 {
    let denom = far_blocks.max(1.0).log2() - 4.0;
    if denom <= 0.0 {
        0.0
    } else {
        1.0 - (1.0 / denom).powf(0.25)
    }
}

/// `updateRenderer`'s mix of eye light and render distance. `1` ignores the dark.
pub fn distance_light_weight(far_blocks: f32) -> f32 {
    ((far_blocks.max(1.0).log2() - 5.0) / 3.0).clamp(0.0, 1.0)
}

/// `BiomeGenBase.getSkyColorByTemp`, returned as sRGB channels.
pub fn biome_sky_rgb(temperature: f32) -> [f32; 3] {
    let shifted = (temperature / 3.0).clamp(-1.0, 1.0);
    hsb_to_rgb(0.622_222_24 - shifted * 0.05, 0.5 + shifted * 0.1, 1.0)
}

/// What the weather does to the sky this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyWeather {
    /// `World.getRainStrength`.
    pub rain: f32,
    /// `World.getWeightedThunderStrength`.
    pub thunder: f32,
    /// Beta's `World.field_27172_i` less the partial tick: ticks of lightning
    /// flash left, `0` for none.
    pub flash: f32,
}

/// Pulls a color toward `gray_scale` times its own luminance, keeping `keep`.
pub(super) fn desaturate(rgb: [f32; 3], gray_scale: f32, keep: f32) -> [f32; 3] {
    let gray = (rgb[0] * 0.3 + rgb[1] * 0.59 + rgb[2] * 0.11) * gray_scale;
    rgb.map(|channel| channel * keep + gray * (1.0 - keep))
}

/// `World.getSkyColor`: the daylight cosine, then rain, thunder, and the
/// lightning flash.
pub fn sky_rgb(temperature: f32, angle: f32, weather: SkyWeather) -> [f32; 3] {
    let day = daylight_factor(angle);
    let mut sky = biome_sky_rgb(temperature).map(|channel| channel * day);
    if weather.rain > 0.0 {
        sky = desaturate(sky, 0.6, 1.0 - weather.rain * 0.75);
    }
    if weather.thunder > 0.0 {
        sky = desaturate(sky, 0.2, 1.0 - weather.thunder * 0.75);
    }
    if weather.flash > 0.0 {
        let flash = weather.flash.min(1.0) * 0.45;
        sky = [
            sky[0] * (1.0 - flash) + 0.8 * flash,
            sky[1] * (1.0 - flash) + 0.8 * flash,
            sky[2] * (1.0 - flash) + flash,
        ];
    }
    sky
}

/// `EntityRenderer.updateFogColor`'s rain and thunder dimming of the fog.
pub fn weather_fog_rgb(fog: [f32; 3], rain: f32, thunder: f32) -> [f32; 3] {
    let dim = 1.0 - thunder * 0.5;
    [
        fog[0] * (1.0 - rain * 0.5) * dim,
        fog[1] * (1.0 - rain * 0.5) * dim,
        fog[2] * (1.0 - rain * 0.4) * dim,
    ]
}

/// `WorldProvider.func_4096_a`, the overworld fog before it blends toward the sky.
pub fn base_fog_rgb(angle: f32) -> [f32; 3] {
    let day = daylight_factor(angle);
    [
        0.752_941_2 * (day * 0.94 + 0.06),
        0.847_058_83 * (day * 0.94 + 0.06),
        day * 0.91 + 0.09,
    ]
}

pub fn mix_fog_toward_sky(fog: [f32; 3], sky: [f32; 3], far_blocks: f32) -> [f32; 3] {
    let blend = sky_color_blend(far_blocks);
    [
        fog[0] + (sky[0] - fog[0]) * blend,
        fog[1] + (sky[1] - fog[1]) * blend,
        fog[2] + (sky[2] - fog[2]) * blend,
    ]
}

/// Color a sky direction would have on Beta's fogged sky plane.
///
/// `elevation` is radians above the horizon. The zenith is only 16 blocks from
/// the camera, so it stays near `sky`; the horizon is far enough to become `fog`.
pub fn sky_disc_color(elevation: f32, sky: [f32; 3], fog: [f32; 3], fog_end: f32) -> [f32; 3] {
    let sin_elevation = elevation.sin().max(0.0);
    if sin_elevation <= 0.0 || fog_end <= 0.0 {
        return fog;
    }
    let amount = (SKY_PLANE_HEIGHT / sin_elevation / fog_end).clamp(0.0, 1.0);
    [
        sky[0] + (fog[0] - sky[0]) * amount,
        sky[1] + (fog[1] - sky[1]) * amount,
        sky[2] + (fog[2] - sky[2]) * amount,
    ]
}

/// Below-horizon plane. `WorldProvider.func_28112_c` is true for the overworld.
pub fn void_rgb(sky: [f32; 3]) -> [f32; 3] {
    [sky[0] * 0.2 + 0.04, sky[1] * 0.2 + 0.04, sky[2] * 0.6 + 0.1]
}

/// Sunrise fan tint, or `None` while the sun is away from the horizon.
pub fn sunrise_rgba(angle: f32) -> Option<[f32; 4]> {
    let cosine = (angle * MC_PI * 2.0).cos();
    let threshold = 0.4;
    if !(-threshold..=threshold).contains(&cosine) {
        return None;
    }
    let fade = (cosine / threshold) * 0.5 + 0.5;
    let mut alpha = 1.0 - (1.0 - (fade * MC_PI).sin()) * 0.99;
    alpha *= alpha;
    Some([fade * 0.3 + 0.7, fade * fade * 0.7 + 0.2, 0.2, alpha])
}

fn hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> [f32; 3] {
    let byte = |value: f32| (value * 255.0 + 0.5).floor().clamp(0.0, 255.0) / 255.0;
    if saturation == 0.0 {
        let gray = byte(brightness);
        return [gray, gray, gray];
    }
    let hue = (hue - hue.floor()) * 6.0;
    let sector = hue.floor();
    let fraction = hue - sector;
    let p = brightness * (1.0 - saturation);
    let q = brightness * (1.0 - saturation * fraction);
    let t = brightness * (1.0 - saturation * (1.0 - fraction));
    let (red, green, blue) = match sector as i32 {
        0 => (brightness, t, p),
        1 => (q, brightness, p),
        2 => (p, brightness, t),
        3 => (p, q, brightness),
        4 => (t, p, brightness),
        _ => (brightness, p, q),
    };
    [byte(red), byte(green), byte(blue)]
}

fn srgb(rgb: [f32; 3]) -> Color {
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

fn eye_brightness(chunks: &WorldChunks, eye: Vec3, subtracted: u8) -> f32 {
    let x = eye.x.floor() as i32;
    let y = eye.y.floor() as i32;
    let z = eye.z.floor() as i32;
    beta_brightness(light_level_at(chunks, x, y, z, subtracted))
}

fn medium_at(chunks: &WorldChunks, eye: Vec3) -> Medium {
    match chunks.block_at(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    ) {
        Some(Block::Water | Block::FlowingWater) => Medium::Water,
        Some(Block::Lava | Block::FlowingLava) => Medium::Lava,
        _ => Medium::Air,
    }
}

enum Medium {
    Air,
    Water,
    Lava,
}

fn ensure_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TintedMaterial>>,
    assets: Option<Res<SkyAssets>>,
    asset_server: Res<AssetServer>,
    settings: Res<GameSettings>,
    mut cameras: Query<
        (Entity, &mut Camera, &mut Projection),
        (With<PlayerCamera>, Without<SkyAttached>),
    >,
) {
    let Ok((camera, mut view, mut projection)) = cameras.single_mut() else {
        return;
    };
    view.clear_color = ClearColorConfig::None;
    let far = view_distance_blocks(settings.render_distance);
    if let Projection::Perspective(perspective) = projection.as_mut() {
        perspective.far = far * 2.0;
    }
    let initial = DistanceFog {
        color: srgb(base_fog_rgb(celestial_angle(0, 0.0))),
        falloff: FogFalloff::Linear {
            start: far * 0.25,
            end: far,
        },
        directional_light_color: Color::NONE,
        ..default()
    };
    commands
        .entity(camera)
        .insert((SkyAttached, initial.clone()));

    let sky_assets = if let Some(existing) = assets.as_deref() {
        existing.clone()
    } else {
        // Star fade and sunrise color ride in each entity's `MeshTag`, so
        // these materials never change after creation.
        let created = SkyAssets {
            sun: materials.add(tinted(body_material(&asset_server, "terrain/sun.png"))),
            moon: materials.add(tinted(body_material(&asset_server, "terrain/moon.png"))),
            stars: materials.add(tinted(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            })),
            sunrise: materials.add(tinted(StandardMaterial {
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            })),
            sunrise_mesh: meshes.add(sunrise_mesh()),
        };
        commands.insert_resource(created.clone());
        created
    };

    commands.entity(camera).with_children(|parent| {
        parent.spawn((
            Name::new("Sky camera"),
            SkyCamera,
            Camera3d::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            Camera {
                order: -2,
                clear_color: ClearColorConfig::Custom(initial.color),
                ..default()
            },
            Projection::from(PerspectiveProjection {
                fov: settings.fov_radians(),
                far: 1024.0,
                ..default()
            }),
            initial,
            RenderLayers::layer(SKY_LAYER),
        ));
        parent.spawn((
            Name::new("Celestial camera"),
            CelestialCamera,
            Camera3d::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            Camera {
                order: -1,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Projection::from(PerspectiveProjection {
                // Same view as the world camera. A narrower default fov makes
                // the stars slide against the terrain when the player looks.
                fov: settings.fov_radians(),
                far: 1024.0,
                ..default()
            }),
            RenderLayers::layer(CELESTIAL_LAYER),
        ));
    });

    let sun_mesh = meshes.add(textured_quad(SUN_DISTANCE, SUN_SIZE, SUN_UVS));
    let moon_mesh = meshes.add(textured_quad(-SUN_DISTANCE, MOON_SIZE, MOON_UVS));
    let star_mesh = meshes.add(star_mesh());
    commands
        .spawn((
            Name::new("Sky"),
            SkyAnchor,
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|sky| {
            sky.spawn((
                Name::new("Sunrise"),
                SunriseFan,
                Mesh3d(sky_assets.sunrise_mesh.clone()),
                tint_tag(Color::NONE),
                MeshMaterial3d(sky_assets.sunrise.clone()),
                Transform::default(),
                Visibility::Hidden,
                RenderLayers::layer(CELESTIAL_LAYER),
                NoFrustumCulling,
            ));
            sky.spawn((
                Name::new("Celestial rig"),
                CelestialRig,
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|rig| {
                for (name, mesh, material) in [
                    ("Sun", sun_mesh, sky_assets.sun.clone()),
                    ("Moon", moon_mesh, sky_assets.moon.clone()),
                ] {
                    rig.spawn((
                        Name::new(name),
                        CelestialBody,
                        tint_tag(Color::WHITE),
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::default(),
                        Visibility::default(),
                        RenderLayers::layer(CELESTIAL_LAYER),
                        NoFrustumCulling,
                    ));
                }
                rig.spawn((
                    Name::new("Stars"),
                    StarField,
                    tint_tag(Color::NONE),
                    Mesh3d(star_mesh),
                    MeshMaterial3d(sky_assets.stars.clone()),
                    Transform::default(),
                    Visibility::default(),
                    RenderLayers::layer(CELESTIAL_LAYER),
                    NoFrustumCulling,
                ));
            });
        });
}

fn update_atmosphere(
    tick: Res<WorldTick>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    flash: Option<Res<super::weather::SkyFlash>>,
    settings: Res<GameSettings>,
    chunks: Res<WorldChunks>,
    mut eye_fog: ResMut<EyeFog>,
    assets: Option<Res<SkyAssets>>,
    mut views: SkyViews,
) {
    // Nothing to update until `ensure_sky` has created the sky entities.
    if assets.is_none() {
        return;
    }
    let far = view_distance_blocks(settings.render_distance);
    let angle = celestial_angle(tick.world_time(), tick.partial());
    let eye = views.eyes.single().ok().map(|eye| *eye);
    let temperature = eye
        .map(|eye| {
            chunks
                .climate_at(
                    eye.translation.x.floor() as i32,
                    eye.translation.z.floor() as i32,
                )
                .map_or(0.5, |climate| climate.temperature as f32)
        })
        .unwrap_or(0.5);
    let sky_weather = SkyWeather {
        rain: weather.as_ref().map_or(0.0, |w| w.rain_strength),
        thunder: weather.as_ref().map_or(0.0, |w| w.weighted_thunder()),
        flash: flash
            .as_ref()
            .map_or(0.0, |flash| (f32::from(flash.0) - tick.partial()).max(0.0)),
    };
    let sky = sky_rgb(temperature, angle, sky_weather);
    let mut fog = weather_fog_rgb(
        mix_fog_toward_sky(base_fog_rgb(angle), sky, far),
        sky_weather.rain,
        sky_weather.thunder,
    );
    let medium = eye.map_or(Medium::Air, |eye| medium_at(&chunks, eye.translation));
    match medium {
        Medium::Water => fog = [0.02, 0.02, 0.2],
        Medium::Lava => fog = [0.6, 0.1, 0.0],
        Medium::Air => {}
    }

    let subtracted = crate::world::weather::skylight_subtracted(weather.as_deref(), angle);
    let brightness = eye.map_or_else(
        || beta_brightness(15u8.saturating_sub(subtracted)),
        |eye| eye_brightness(&chunks, eye.translation, subtracted),
    );
    let weight = distance_light_weight(far);
    let target = brightness * (1.0 - weight) + weight;
    if eye_fog.ready {
        for _ in 0..tick.ticks_this_frame() {
            eye_fog.previous = eye_fog.current;
            eye_fog.current += (target - eye_fog.current) * 0.1;
        }
    } else {
        eye_fog.previous = target;
        eye_fog.current = target;
        eye_fog.ready = true;
    }
    let factor = eye_fog.previous + (eye_fog.current - eye_fog.previous) * tick.partial();
    let fog = fog.map(|channel| channel * factor);
    let fog_color = srgb(fog);

    let (start, end) = world_fog_range(far);
    let world_falloff = match medium {
        Medium::Water => FogFalloff::Exponential { density: 0.1 },
        Medium::Lava => FogFalloff::Exponential { density: 2.0 },
        Medium::Air => FogFalloff::Linear { start, end },
    };
    let mut view_fov = None;
    for (mut distance, mut projection) in &mut views.player_cameras {
        distance.color = fog_color;
        distance.falloff = world_falloff.clone();
        distance.directional_light_color = Color::NONE;
        if let Projection::Perspective(perspective) = projection.as_ref() {
            view_fov = Some(perspective.fov);
            if perspective.far != far * 2.0
                && let Projection::Perspective(perspective) = projection.as_mut()
            {
                perspective.far = far * 2.0;
            }
        }
    }
    for (mut distance, mut camera, mut projection) in &mut views.sky_cameras {
        distance.color = fog_color;
        distance.falloff = FogFalloff::Linear {
            start: 0.0,
            end: sky_fog_end(far),
        };
        distance.directional_light_color = Color::NONE;
        // Keep the atmosphere behind the fog-free sky geometry. World
        // blocks and clouds still use the player's fog, but the sky pass
        // should not introduce a second fog-colored horizon.
        let clear = srgb(sky);
        if !matches!(camera.clear_color, ClearColorConfig::Custom(color) if color == clear) {
            camera.clear_color = ClearColorConfig::Custom(clear);
        }
        copy_fov(view_fov, &mut projection);
    }
    for mut projection in &mut views.celestial_cameras {
        copy_fov(view_fov, &mut projection);
    }

    if let Ok(camera) = views.player.single() {
        for mut anchor in &mut views.anchors {
            let next = Transform {
                translation: camera.translation(),
                rotation: Quat::IDENTITY,
                ..*anchor
            };
            anchor.set_if_neq(next);
        }
    }
    let spin = Quat::from_rotation_x(angle * std::f32::consts::TAU);
    for mut rig in &mut views.rigs {
        rig.reborrow()
            .map_unchanged(|rig| &mut rig.rotation)
            .set_if_neq(spin);
    }
    // `RenderGlobal.renderSky` fades the sun, moon and stars out with the rain.
    let clear = 1.0 - sky_weather.rain;
    let body_tag = tint_tag(Color::WHITE.with_alpha(clear));
    for mut tag in &mut views.bodies {
        tag.set_if_neq(body_tag.clone());
    }
    let stars_on = star_brightness(angle) * clear;
    let star_tag = tint_tag(Color::WHITE.with_alpha(stars_on));
    for (mut visibility, mut tag) in &mut views.stars {
        visibility.set_if_neq(if stars_on > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        tag.set_if_neq(star_tag.clone());
    }
    let sunrise = sunrise_rgba(angle);
    for (mut transform, mut visibility, mut tag) in &mut views.sunrises {
        if let Some([red, green, blue, alpha]) = sunrise {
            let mut rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            if angle > 0.5 {
                rotation *= Quat::from_rotation_z(std::f32::consts::PI);
            }
            transform.rotation = rotation;
            // The rim bows back by 40 blocks times the fan's alpha. The center
            // sits at z = 0, so scaling z reproduces that without a new mesh.
            transform.scale = Vec3::new(1.0, 1.0, alpha.max(1e-4));
            visibility.set_if_neq(Visibility::Inherited);
            tag.set_if_neq(tint_tag(Color::linear_rgba(red, green, blue, alpha)));
        } else {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

fn copy_fov(fov: Option<f32>, projection: &mut Mut<Projection>) {
    let (Some(fov), Projection::Perspective(perspective)) = (fov, projection.as_ref()) else {
        return;
    };
    if perspective.fov != fov
        && let Projection::Perspective(perspective) = projection.as_mut()
    {
        perspective.fov = fov;
    }
}

fn unlit_color(color: Color, fog: bool) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        unlit: true,
        fog_enabled: fog,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

fn tinted(base: StandardMaterial) -> TintedMaterial {
    TintedMaterial {
        base,
        extension: InstanceTint {},
    }
}

fn body_material(server: &AssetServer, path: &str) -> StandardMaterial {
    let texture = std::path::Path::new("assets")
        .join(path)
        .exists()
        .then(|| server.load(path.to_string()));
    let mut material = unlit_color(Color::WHITE, false);
    material.base_color_texture = texture;
    material.alpha_mode = AlphaMode::Add;
    material
}

/// Sun disk. UVs flip V so the PNG's top stays the top under Bevy's texture space.
const SUN_UVS: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
/// Moon disk, in Beta's vertex order, with the same V flip.
const MOON_UVS: [[f32; 2]; 4] = [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

fn textured_quad(y: f32, size: f32, uvs: [[f32; 2]; 4]) -> Mesh {
    let positions = if y >= 0.0 {
        vec![
            [-size, y, -size],
            [size, y, -size],
            [size, y, size],
            [-size, y, size],
        ]
    } else {
        vec![
            [-size, y, size],
            [size, y, size],
            [size, y, -size],
            [-size, y, -size],
        ]
    };
    mesh_from(
        positions,
        vec![[0.0, y.signum(), 0.0]; 4],
        uvs.to_vec(),
        vec![[1.0; 4]; 4],
        vec![0_u32, 1, 2, 0, 2, 3],
    )
}

/// The sunrise fan at full alpha. Its color comes from the entity's tint tag
/// and its depth from the entity's z scale, both set by `update_atmosphere`.
fn sunrise_mesh() -> Mesh {
    let mut positions = vec![[0.0, 100.0, 0.0]];
    let mut colors = vec![[1.0; 4]];
    let segments = 16;
    for step in 0..=segments {
        let theta = step as f32 * MC_PI * 2.0 / segments as f32;
        let (sin, cos) = theta.sin_cos();
        positions.push([sin * 120.0, cos * 120.0, -cos * 40.0]);
        colors.push([1.0, 1.0, 1.0, 0.0]);
    }
    let mut indices = Vec::new();
    for step in 0..segments {
        let step = u32::try_from(step).unwrap_or(0);
        indices.extend([0, step + 1, step + 2]);
    }
    let count = positions.len();
    mesh_from(
        positions,
        vec![[0.0, 1.0, 0.0]; count],
        vec![[0.0, 0.0]; count],
        colors,
        indices,
    )
}

fn star_mesh() -> Mesh {
    let mut random = JavaRandom::new(10_842);
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for _ in 0..1500 {
        let mut x = f64::from(random.next_float() * 2.0 - 1.0);
        let mut y = f64::from(random.next_float() * 2.0 - 1.0);
        let mut z = f64::from(random.next_float() * 2.0 - 1.0);
        let size = f64::from(0.25 + random.next_float() * 0.25);
        let mut length_sq = x * x + y * y + z * z;
        if !(0.01..1.0).contains(&length_sq) {
            continue;
        }
        length_sq = 1.0 / length_sq.sqrt();
        x *= length_sq;
        y *= length_sq;
        z *= length_sq;
        let (sx, sy, sz) = (x * 100.0, y * 100.0, z * 100.0);
        let yaw = x.atan2(z);
        let (yaw_sin, yaw_cos) = yaw.sin_cos();
        let pitch = (x * x + z * z).sqrt().atan2(y);
        let (pitch_sin, pitch_cos) = pitch.sin_cos();
        let spin = random.next_double() * std::f64::consts::PI * 2.0;
        let (spin_sin, spin_cos) = spin.sin_cos();
        let base = positions.len() as u32;
        for corner in 0..4 {
            let local_u = f64::from((corner & 2) - 1) * size;
            let local_v = f64::from(((corner + 1) & 2) - 1) * size;
            let rotated_u = local_u * spin_cos - local_v * spin_sin;
            let rotated_v = local_v * spin_cos + local_u * spin_sin;
            let pitched_y = rotated_u * pitch_sin;
            let pitched_z = -rotated_u * pitch_cos;
            let world_x = pitched_z * yaw_sin - rotated_v * yaw_cos;
            let world_z = rotated_v * yaw_sin + pitched_z * yaw_cos;
            positions.push([
                (sx + world_x) as f32,
                (sy + pitched_y) as f32,
                (sz + world_z) as f32,
            ]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let count = positions.len();
    mesh_from(
        positions,
        vec![[0.0, 1.0, 0.0]; count],
        vec![[0.0, 0.0]; count],
        vec![[1.0; 4]; count],
        indices,
    )
}

fn mesh_from(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

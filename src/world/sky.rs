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
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::light::NotShadowCaster;
use bevy::mesh::Indices;
use bevy::pbr::DistanceFog;
use bevy::pbr::FogFalloff;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::world::block::block::BlockId;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::light_emission;
use crate::world::lighting::light_opacity;

use super::plugin::apply_lighting_settings;
use super::tick::WorldTick;

const SKY_LAYER: usize = 2;
/// Full sun used by the directional light at noon.
const SUN_ILLUMINANCE: f32 = 10_000.0;
/// `MathHelper` and the fog code use this float, not `std` PI.
const MC_PI: f32 = 3.141_592_7;
const SUN_DISTANCE: f32 = 100.0;
const SUN_SIZE: f32 = 30.0;
const MOON_SIZE: f32 = 20.0;
/// Height of Beta's sky plane (`glSkyList` uses `16`, `glSkyList2` uses `-16`).
const SKY_PLANE_HEIGHT: f32 = 16.0;
/// Half-width of the ceiling and floor. Larger than `sky_fog_end` at the
/// maximum render distance, so fog hides the edge instead of a hard seam.
const SKY_PLANE_EXTENT: f32 = 512.0;
const CELESTIAL_LAYER: usize = 3;

/// Marker so the world camera keeps its own fog range and the Ultra water pass
/// does not put screen-space reflections on the sky.
#[derive(Component)]
pub(crate) struct SkyCamera;

/// Sun, moon, stars, and the sunrise fan. Drawn after the ceiling so the
/// ceiling's depth does not cover them.
#[derive(Component)]
pub(crate) struct CelestialCamera;

#[derive(Component)]
struct SkyAttached;

#[derive(Component)]
struct SkyAnchor;

#[derive(Component)]
struct CelestialRig;

#[derive(Component)]
struct SunriseFan;

#[derive(Component)]
struct StarField;

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
    ceiling: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    sun: Handle<StandardMaterial>,
    moon: Handle<StandardMaterial>,
    stars: Handle<StandardMaterial>,
    sunrise: Handle<StandardMaterial>,
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
        (&'static mut Transform, &'static mut Visibility),
        (
            With<SunriseFan>,
            Without<StarField>,
            Without<SkyAnchor>,
            Without<Player>,
        ),
    >,
    stars: Query<'w, 's, &'static mut Visibility, (With<StarField>, Without<SunriseFan>)>,
    suns: Query<
        'w,
        's,
        (&'static mut DirectionalLight, &'static mut Transform),
        (
            Without<SkyAnchor>,
            Without<CelestialRig>,
            Without<SunriseFan>,
            Without<Player>,
        ),
    >,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<EyeFog>().add_systems(
        Update,
        (ensure_sky, update_atmosphere)
            .chain()
            .after(apply_lighting_settings),
    );
}

/// Day length fraction after Beta's sunrise offset and cosine smoothing.
/// Noon (`world_time = 6000`) is `0`.
pub fn celestial_angle(world_time: u64, partial: f32) -> f32 {
    let day = (world_time % super::tick::DAY_LENGTH) as f32;
    let mut angle = (day + partial) / super::tick::DAY_LENGTH as f32 - 0.25;
    if angle < 0.0 {
        angle += 1.0;
    }
    if angle > 1.0 {
        angle -= 1.0;
    }
    let base = angle;
    let cosine = (f64::from(angle) * std::f64::consts::PI).cos();
    let fraction = ((cosine + 1.0) / 2.0) as f32;
    let smoothed = 1.0 - fraction;
    base + (smoothed - base) / 3.0
}

/// `cos(angle * 2π) * 2 + 0.5`, clamped, shared by sky, fog, and sunlight.
pub fn daylight_factor(angle: f32) -> f32 {
    ((angle * MC_PI * 2.0).cos() * 2.0 + 0.5).clamp(0.0, 1.0)
}

/// `World.calculateSkylightSubtracted` with no rain or thunder.
pub fn skylight_subtracted(angle: f32) -> u8 {
    let mut light = 1.0 - daylight_factor(angle);
    light = 1.0 - light;
    light = 1.0 - light;
    (light * 11.0) as u8
}

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

/// Sky color after the daylight cosine. Rain and thunder are not simulated.
pub fn sky_rgb(temperature: f32, angle: f32) -> [f32; 3] {
    let day = daylight_factor(angle);
    biome_sky_rgb(temperature).map(|channel| channel * day)
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

/// Direct sun reaches the eye when no opaque block sits in the column above it.
fn open_to_sky(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> bool {
    if y >= CHUNK_HEIGHT as i32 {
        return true;
    }
    for above in (y + 1)..CHUNK_HEIGHT as i32 {
        let Some(block) = chunks.block_at(x, above, z) else {
            return true;
        };
        if light_opacity(block) >= 15 {
            return false;
        }
    }
    true
}

fn eye_brightness(chunks: &WorldChunks, eye: Vec3, subtracted: u8) -> f32 {
    let x = eye.x.floor() as i32;
    let y = eye.y.floor() as i32;
    let z = eye.z.floor() as i32;
    let sky = if open_to_sky(chunks, x, y, z) {
        15u8.saturating_sub(subtracted)
    } else {
        0
    };
    let emitted = chunks.block_at(x, y, z).map_or(0, light_emission);
    beta_brightness(sky.max(emitted))
}

fn medium_at(chunks: &WorldChunks, eye: Vec3) -> Medium {
    match chunks.block_at(
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    ) {
        Some(BlockId::Water | BlockId::FlowingWater) => Medium::Water,
        Some(BlockId::Lava | BlockId::FlowingLava) => Medium::Lava,
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
    mut materials: ResMut<Assets<StandardMaterial>>,
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
        let created = SkyAssets {
            ceiling: materials.add(plane_material(Color::WHITE)),
            floor: materials.add(plane_material(Color::WHITE)),
            sun: materials.add(body_material(&asset_server, "terrain/sun.png")),
            moon: materials.add(body_material(&asset_server, "terrain/moon.png")),
            stars: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            }),
            sunrise: materials.add(StandardMaterial {
                unlit: true,
                fog_enabled: false,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                double_sided: true,
                ..default()
            }),
            sunrise_mesh: meshes.add(sunrise_mesh([0.0; 4])),
        };
        commands.insert_resource(created.clone());
        created
    };

    commands.entity(camera).with_children(|parent| {
        parent.spawn((
            Name::new("Sky camera"),
            SkyCamera,
            Camera3d::default(),
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
            spawn_layer(
                sky,
                "Sky ceiling",
                meshes.add(sky_plane_mesh(SKY_PLANE_HEIGHT)),
                sky_assets.ceiling.clone(),
                Transform::default(),
                SKY_LAYER,
            );
            spawn_layer(
                sky,
                "Sky floor",
                meshes.add(sky_plane_mesh(-SKY_PLANE_HEIGHT)),
                sky_assets.floor.clone(),
                Transform::default(),
                SKY_LAYER,
            );
            sky.spawn((
                Name::new("Sunrise"),
                SunriseFan,
                Mesh3d(sky_assets.sunrise_mesh.clone()),
                MeshMaterial3d(sky_assets.sunrise.clone()),
                Transform::default(),
                Visibility::Hidden,
                RenderLayers::layer(CELESTIAL_LAYER),
                NotShadowCaster,
                NoFrustumCulling,
            ));
            sky.spawn((
                Name::new("Celestial rig"),
                CelestialRig,
                Transform::default(),
                Visibility::default(),
            ))
            .with_children(|rig| {
                spawn_layer(
                    rig,
                    "Sun",
                    sun_mesh,
                    sky_assets.sun.clone(),
                    Transform::default(),
                    CELESTIAL_LAYER,
                );
                spawn_layer(
                    rig,
                    "Moon",
                    moon_mesh,
                    sky_assets.moon.clone(),
                    Transform::default(),
                    CELESTIAL_LAYER,
                );
                rig.spawn((
                    Name::new("Stars"),
                    StarField,
                    Mesh3d(star_mesh),
                    MeshMaterial3d(sky_assets.stars.clone()),
                    Transform::default(),
                    Visibility::default(),
                    RenderLayers::layer(CELESTIAL_LAYER),
                    NotShadowCaster,
                    NoFrustumCulling,
                ));
            });
        });
}

fn spawn_layer(
    parent: &mut ChildSpawnerCommands,
    name: &str,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    layer: usize,
) {
    parent.spawn((
        Name::new(name.to_string()),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        transform,
        Visibility::default(),
        RenderLayers::layer(layer),
        NotShadowCaster,
        NoFrustumCulling,
    ));
}

fn update_atmosphere(
    tick: Res<WorldTick>,
    settings: Res<GameSettings>,
    chunks: Res<WorldChunks>,
    state: Option<Res<State<AppScreen>>>,
    mut eye_fog: ResMut<EyeFog>,
    assets: Option<Res<SkyAssets>>,
    mut views: SkyViews,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(assets) = assets else {
        return;
    };
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
    let sky = sky_rgb(temperature, angle);
    let mut fog = mix_fog_toward_sky(base_fog_rgb(angle), sky, far);
    let medium = eye.map_or(Medium::Air, |eye| medium_at(&chunks, eye.translation));
    match medium {
        Medium::Water => fog = [0.02, 0.02, 0.2],
        Medium::Lava => fog = [0.6, 0.1, 0.0],
        Medium::Air => {}
    }

    let subtracted = skylight_subtracted(angle);
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
        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.far = far * 2.0;
            view_fov = Some(perspective.fov);
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
        camera.clear_color = ClearColorConfig::Custom(srgb(sky));
        copy_fov(view_fov, &mut projection);
    }
    for mut projection in &mut views.celestial_cameras {
        copy_fov(view_fov, &mut projection);
    }

    if let Ok(camera) = views.player.single() {
        for mut anchor in &mut views.anchors {
            anchor.translation = camera.translation();
            anchor.rotation = Quat::IDENTITY;
        }
    }
    let spin = Quat::from_rotation_x(angle * std::f32::consts::TAU);
    for mut rig in &mut views.rigs {
        rig.rotation = spin;
    }
    if let Some(mut material) = materials.get_mut(&assets.ceiling) {
        material.base_color = srgb(sky);
    }
    if let Some(mut material) = materials.get_mut(&assets.floor) {
        material.base_color = srgb(void_rgb(sky));
    }
    let stars_on = star_brightness(angle);
    if let Some(mut material) = materials.get_mut(&assets.stars) {
        material.base_color = Color::WHITE.with_alpha(stars_on);
    }
    for mut visibility in &mut views.stars {
        *visibility = if stars_on > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    let sunrise = sunrise_rgba(angle);
    for (mut transform, mut visibility) in &mut views.sunrises {
        if let Some(rgba) = sunrise {
            let mut rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            if angle > 0.5 {
                rotation *= Quat::from_rotation_z(std::f32::consts::PI);
            }
            transform.rotation = rotation;
            *visibility = Visibility::Inherited;
            if let Some(mut mesh) = meshes.get_mut(&assets.sunrise_mesh) {
                *mesh = sunrise_mesh(rgba);
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    let playing = matches!(state.as_deref().map(State::get), Some(AppScreen::Playing));
    if !playing {
        return;
    }
    let day = daylight_factor(angle);
    let direction = spin * Vec3::Y;
    let up = if direction.y.abs() > 0.9 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    for (mut light, mut transform) in &mut views.suns {
        light.illuminance = if settings.directional_lighting {
            SUN_ILLUMINANCE * day
        } else {
            0.0
        };
        light.shadow_maps_enabled = settings.directional_lighting;
        transform.translation = direction * SUN_DISTANCE;
        transform.look_at(Vec3::ZERO, up);
    }
}

fn copy_fov(fov: Option<f32>, projection: &mut Projection) {
    let (Some(fov), Projection::Perspective(perspective)) = (fov, projection) else {
        return;
    };
    perspective.fov = fov;
}

fn plane_material(color: Color) -> StandardMaterial {
    // The sky is the atmosphere/background pass. Fog belongs to world
    // geometry and clouds; applying it here creates a hard band where the
    // flat sky planes meet the world horizon.
    unlit_color(color, false)
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

/// Horizontal quad at `y`, wide enough that sky fog covers its edge.
fn sky_plane_mesh(y: f32) -> Mesh {
    let h = SKY_PLANE_EXTENT;
    mesh_from(
        vec![[-h, y, -h], [h, y, -h], [h, y, h], [-h, y, h]],
        vec![[0.0, 1.0, 0.0]; 4],
        vec![[0.0, 0.0]; 4],
        vec![[1.0, 1.0, 1.0, 1.0]; 4],
        vec![0, 2, 1, 0, 3, 2],
    )
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

fn sunrise_mesh(rgba: [f32; 4]) -> Mesh {
    let mut positions = vec![[0.0, 100.0, 0.0]];
    let mut colors = vec![[rgba[0], rgba[1], rgba[2], rgba[3]]];
    let segments = 16;
    for step in 0..=segments {
        let theta = step as f32 * MC_PI * 2.0 / segments as f32;
        let (sin, cos) = theta.sin_cos();
        positions.push([sin * 120.0, cos * 120.0, -cos * 40.0 * rgba[3]]);
        colors.push([rgba[0], rgba[1], rgba[2], 0.0]);
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

/// `java.util.Random`, only the methods the star field uses.
struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    fn new(seed: u64) -> Self {
        Self {
            seed: (seed ^ 0x5DEECE66D) & ((1u64 << 48) - 1),
        }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1u64 << 48) - 1);
        (self.seed >> (48 - bits)) as i32
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1i32 << 24) as f32
    }

    fn next_double(&mut self) -> f64 {
        let high = i64::from(self.next(26));
        let low = i64::from(self.next(27));
        ((high << 27) + low) as f64 / (1u64 << 53) as f64
    }
}

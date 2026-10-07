//! Rain, snow and lightning as Beta draws them.
//!
//! `EntityRenderer.renderRainSnow` draws two crossed quads in every block
//! column near the player, from the column's top solid block up, textured with
//! `environment/rain.png` or `snow.png` and scrolled downward. Here each kind
//! is one mesh, rebuilt only when its columns change (the player crosses a
//! block, a block or the light changes). The scroll runs on the shader's clock
//! and the fade with rain strength rides in [`MeshTag`], so falling rain
//! rewrites no asset. Two things differ from Beta: the fade toward the edge is
//! per fragment rather than per column, and rain scrolls continuously where
//! Beta's restarts every 32 ticks.
//!
//! Lightning is `RenderLightningBolt`'s jagged, branching bolt, re-rolled as
//! `EntityLightningBolt` does, and it lights the sky through [`SkyFlash`].
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::asset::load_internal_asset;
use bevy::asset::uuid_handle;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::ImageAddressMode;
use bevy::image::ImageLoaderSettings;
use bevy::image::ImageSampler;
use bevy::image::ImageSamplerDescriptor;
use bevy::mesh::Indices;
use bevy::mesh::MeshTag;
use bevy::pbr::ExtendedMaterial;
use bevy::pbr::MaterialExtension;
use bevy::pbr::MaterialPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::shader::Shader;
use bevy::shader::ShaderRef;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::player::Player;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::lighting::LightCache;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::column_channels;
use crate::world::lighting::combined_light;
use crate::world::tick::WorldTick;
use crate::world::weather::LightningStrike;
use crate::world::weather::WorldWeather;

const WEATHER_SHADER_HANDLE: Handle<Shader> = uuid_handle!("7c1e52a4-3b0d-4f86-9a57-d2e84b6f1c39");

/// Column radius on Fancy graphics; Fast uses [`FAST_RADIUS`].
pub const FANCY_RADIUS: i32 = 10;
pub const FAST_RADIUS: i32 = 5;

/// `StandardMaterial` with the scrolling, fading fragment stage in `weather.wgsl`.
pub type PrecipitationMaterial = ExtendedMaterial<StandardMaterial, PrecipitationExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct PrecipitationExtension {}

impl MaterialExtension for PrecipitationExtension {
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }

    fn fragment_shader() -> ShaderRef {
        WEATHER_SHADER_HANDLE.into()
    }
}

/// Which precipitation a biome gets: Beta's `canSpawnLightningBolt` for rain
/// and `getEnableSnow` for snow. Deserts get neither.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Precipitation {
    Rain,
    Snow,
}

impl Precipitation {
    pub fn of(biome: Biome) -> Option<Self> {
        match biome {
            Biome::Taiga | Biome::Tundra | Biome::IceDesert => Some(Self::Snow),
            Biome::Desert => None,
            _ => Some(Self::Rain),
        }
    }

    pub fn at(chunks: &WorldChunks, x: i32, z: i32) -> Option<Self> {
        Self::of(chunks.climate_at(x, z)?.biome)
    }
}

/// One block column of falling rain or snow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrecipitationColumn {
    pub x: i32,
    pub z: i32,
    /// The quads span `bottom..top` in world y.
    pub bottom: i32,
    pub top: i32,
    pub kind: Precipitation,
    /// Beta's vertex color for the column, `0..=1`.
    pub brightness: f32,
}

/// The columns `renderRainSnow` draws around `eye`, appended to `columns`.
/// A column starts at its top solid block, so nothing falls under a roof, and
/// reaches `radius` blocks above and below the eye.
pub fn precipitation_columns(
    chunks: &WorldChunks,
    light: Option<&LightCache>,
    eye: Vec3,
    radius: i32,
    skylight_subtracted: u8,
    columns: &mut Vec<PrecipitationColumn>,
) {
    let center = eye.floor().as_ivec3();
    // Rain is lit from above the world, where only the sky's own light counts.
    let rain_brightness = beta_brightness(15u8.saturating_sub(skylight_subtracted)) * 0.85 + 0.15;
    for x in center.x - radius..=center.x + radius {
        for z in center.z - radius..=center.z + radius {
            let Some(kind) = Precipitation::at(chunks, x, z) else {
                continue;
            };
            let mut ground = chunks.top_solid_block(x, z);
            if kind == Precipitation::Snow {
                ground = ground.max(0);
            }
            let bottom = (center.y - radius).max(ground);
            let top = (center.y + radius).max(ground);
            if bottom == top {
                continue;
            }
            let brightness = match kind {
                Precipitation::Rain => rain_brightness,
                Precipitation::Snow => {
                    let y = ground.max(center.y);
                    let (sky, block) = if y >= CHUNK_HEIGHT as i32 {
                        (15, 0)
                    } else {
                        light
                            .and_then(|light| light.channels(x, y, z))
                            .unwrap_or_else(|| column_channels(chunks, x, y, z))
                    };
                    beta_brightness(combined_light(sky, block, skylight_subtracted))
                }
            };
            columns.push(PrecipitationColumn {
                x,
                z,
                bottom,
                top,
                kind,
                brightness,
            });
        }
    }
}

/// The seed `renderRainSnow` gives its random for a column, in Java `int`
/// arithmetic.
fn column_hash(x: i32, z: i32) -> i32 {
    x.wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
        .wrapping_add(z.wrapping_mul(13_761))
}

/// `Random.nextGaussian` twice: Java computes the values in pairs and hands
/// the second out on the next call.
fn gaussian_pair(random: &mut JavaRandom) -> (f32, f32) {
    loop {
        let first = 2.0 * random.next_double() - 1.0;
        let second = 2.0 * random.next_double() - 1.0;
        let square = first * first + second * second;
        if square < 1.0 && square != 0.0 {
            let scale = (-2.0 * square.ln() / square).sqrt();
            return ((first * scale) as f32, (second * scale) as f32);
        }
    }
}

/// A column's fixed texture offset and its scroll per world tick.
///
/// Rain: `((ticks + hash & 31) + partial) / 32 * (3 + nextFloat)` on V.
/// Snow: a random start on both axes, a slow sideways drift, and one texture
/// height every 512 ticks plus a little per-column variation.
pub fn column_scroll(x: i32, z: i32, kind: Precipitation) -> (Vec2, Vec2) {
    let hash = column_hash(x, z);
    let mut random = JavaRandom::new(i64::from(hash) as u64);
    match kind {
        Precipitation::Rain => {
            let speed = 3.0 + random.next_float();
            let phase = (hash & 31) as f32 / 32.0;
            (Vec2::new(0.0, phase * speed), Vec2::new(0.0, speed / 32.0))
        }
        Precipitation::Snow => {
            let start_u = random.next_float();
            let (drift_u, drift_v) = gaussian_pair(&mut random);
            let start_v = random.next_float();
            (
                Vec2::new(start_u, start_v),
                Vec2::new(0.01 * drift_u, 1.0 / 512.0 + 0.001 * drift_v),
            )
        }
    }
}

/// Crossed quads for every column of `kind`, positioned relative to `origin`.
/// `None` when there are no such columns.
pub fn precipitation_mesh(
    columns: &[PrecipitationColumn],
    kind: Precipitation,
    origin: IVec3,
) -> Option<Mesh> {
    let count = columns.iter().filter(|column| column.kind == kind).count();
    if count == 0 {
        return None;
    }
    let mut positions = Vec::with_capacity(count * 8);
    let mut uvs = Vec::with_capacity(count * 8);
    let mut scrolls = Vec::with_capacity(count * 8);
    let mut colors = Vec::with_capacity(count * 8);
    let mut indices = Vec::with_capacity(count * 12);
    for column in columns.iter().filter(|column| column.kind == kind) {
        let (offset, scroll) = column_scroll(column.x, column.z, kind);
        let x = (column.x - origin.x) as f32;
        let z = (column.z - origin.z) as f32;
        let low = (column.bottom - origin.y) as f32;
        let high = (column.top - origin.y) as f32;
        // Beta's V is the world height over four, so the texture stays put in
        // the world as the player moves.
        let low_v = column.bottom as f32 / 4.0 + offset.y;
        let high_v = column.top as f32 / 4.0 + offset.y;
        // The texture is multiplied in sRGB space in Beta; vertex colors are linear.
        let shade = Color::srgb(column.brightness, column.brightness, column.brightness)
            .to_linear()
            .to_f32_array();
        for quad in [
            [[x, z + 0.5], [x + 1.0, z + 0.5]],
            [[x + 0.5, z], [x + 0.5, z + 1.0]],
        ] {
            let base = positions.len() as u32;
            let [[x0, z0], [x1, z1]] = quad;
            positions.extend([[x0, low, z0], [x1, low, z1], [x1, high, z1], [x0, high, z0]]);
            uvs.extend([
                [offset.x, low_v],
                [offset.x + 1.0, low_v],
                [offset.x + 1.0, high_v],
                [offset.x, high_v],
            ]);
            scrolls.extend([scroll.to_array(); 4]);
            colors.extend([shade; 4]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Some(sheet_mesh(positions, uvs, scrolls, colors, indices))
}

fn sheet_mesh(
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    scrolls: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
) -> Mesh {
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, scrolls)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Bevy 0.19 does not allocate zero-vertex meshes, so an idle sheet keeps
/// this one and stays hidden.
fn placeholder_mesh() -> Mesh {
    sheet_mesh(
        vec![[0.0; 3]; 3],
        vec![[0.0; 2]; 3],
        vec![[0.0; 2]; 3],
        vec![[0.0; 4]; 3],
        vec![0, 1, 2],
    )
}

/// The tag `weather.wgsl` unpacks: strength, column radius, and the snow flag.
pub fn precipitation_tag(strength: f32, radius: i32, kind: Precipitation) -> MeshTag {
    let strength = (strength.clamp(0.0, 1.0) * 255.0).round() as u32;
    let radius = radius.clamp(1, 255) as u32;
    MeshTag(strength | radius << 8 | u32::from(kind == Precipitation::Snow) << 16)
}

#[derive(Component)]
struct PrecipitationSheet {
    kind: Precipitation,
    mesh: Handle<Mesh>,
}

/// What the sheets were last built from.
#[derive(Resource, Default)]
struct PrecipitationState {
    columns: Vec<PrecipitationColumn>,
    scratch: Vec<PrecipitationColumn>,
    origin: IVec3,
}

/// Beta's `World.field_27172_i`: ticks of lightning flash left in the sky.
#[derive(Resource, Default)]
pub(crate) struct SkyFlash(pub u8);

/// The drawn half of `EntityLightningBolt`. Fire and damage are applied once
/// by `world::weather`.
#[derive(Component)]
pub(crate) struct LightningBolt {
    /// `lightningState`: the bolt flashes while this is not negative.
    state: i32,
    /// `boltLivingTime`: re-strikes left.
    living_time: u32,
    random: JavaRandom,
}

/// The sky reads [`SkyFlash`] after these systems have set it.
#[derive(SystemSet, Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct WeatherVisuals;

#[derive(Resource)]
struct BoltMaterial(Handle<StandardMaterial>);

pub(super) fn plugin(app: &mut App) {
    if !app.world().contains_resource::<Assets<Shader>>() {
        app.init_asset::<Shader>();
    }
    app.add_plugins(MaterialPlugin::<PrecipitationMaterial>::default());
    load_internal_asset!(
        app,
        WEATHER_SHADER_HANDLE,
        "weather.wgsl",
        Shader::from_wgsl
    );
    app.init_resource::<PrecipitationState>()
        .init_resource::<SkyFlash>()
        .add_systems(Startup, prepare)
        .add_systems(
            Update,
            (update_precipitation, update_lightning)
                .in_set(WeatherVisuals)
                // An app without the menus, such as the render harness, is
                // always playing.
                .run_if(|state: Option<Res<State<AppScreen>>>| {
                    state.is_none_or(|state| *state.get() == AppScreen::Playing)
                }),
        );
}

fn prepare(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sheets: ResMut<Assets<PrecipitationMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (kind, name, path) in [
        (Precipitation::Rain, "Rain", "environment/rain.png"),
        (Precipitation::Snow, "Snow", "environment/snow.png"),
    ] {
        let texture = Path::new("assets").join(path).exists().then(|| {
            asset_server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    // The columns scroll and tile the texture. Filtering stays
                    // nearest, matching the engine-wide sampler in `main`.
                    let mut descriptor = ImageSamplerDescriptor {
                        lod_max_clamp: 0.0,
                        ..ImageSamplerDescriptor::nearest()
                    };
                    descriptor.address_mode_u = ImageAddressMode::Repeat;
                    descriptor.address_mode_v = ImageAddressMode::Repeat;
                    descriptor.address_mode_w = ImageAddressMode::Repeat;
                    settings.sampler = ImageSampler::Descriptor(descriptor);
                })
                .load(path.to_string())
        });
        let mesh = meshes.add(placeholder_mesh());
        commands.spawn((
            Name::new(name),
            PrecipitationSheet {
                kind,
                mesh: mesh.clone(),
            },
            Mesh3d(mesh),
            MeshMaterial3d(sheets.add(PrecipitationMaterial {
                base: StandardMaterial {
                    base_color_texture: texture,
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    cull_mode: None,
                    double_sided: true,
                    ..default()
                },
                extension: PrecipitationExtension {},
            })),
            precipitation_tag(0.0, FANCY_RADIUS, kind),
            Transform::default(),
            Visibility::Hidden,
            NoFrustumCulling,
        ));
    }
    // `RenderLightningBolt` adds (0.45, 0.45, 0.5) at alpha 0.3 per layer.
    commands.insert_resource(BoltMaterial(materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.45, 0.5, 0.3),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        double_sided: true,
        ..default()
    })));
}

fn update_precipitation(
    tick: Res<WorldTick>,
    weather: Option<Res<WorldWeather>>,
    settings: Option<Res<GameSettings>>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    player: Query<&Transform, (With<Player>, Without<PrecipitationSheet>)>,
    mut state: ResMut<PrecipitationState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sheets: Query<
        (
            &PrecipitationSheet,
            &mut Transform,
            &mut Visibility,
            &mut MeshTag,
        ),
        Without<Player>,
    >,
) {
    let strength = weather
        .as_ref()
        .map_or(0.0, |weather| weather.rain_strength);
    let (Ok(player), true) = (player.single(), strength > 0.0) else {
        for (_, _, mut visibility, _) in &mut sheets {
            visibility.set_if_neq(Visibility::Hidden);
        }
        state.columns.clear();
        return;
    };
    let fancy = settings
        .as_ref()
        .is_none_or(|settings| settings.graphics.fancy_leaves());
    let radius = if fancy { FANCY_RADIUS } else { FAST_RADIUS };
    let origin = player.translation.floor().as_ivec3();

    // Blocks and light only change on a world tick; between ticks the only
    // thing that can move the columns is the player.
    let state = state.into_inner();
    let stale = state.columns.is_empty() || state.origin != origin || tick.ticks_this_frame() > 0;
    let mut rebuild = false;
    if stale {
        let subtracted = crate::world::weather::skylight_subtracted(
            weather.as_deref(),
            celestial_angle(tick.world_time(), tick.partial()),
        );
        state.scratch.clear();
        precipitation_columns(
            &chunks,
            light.as_deref(),
            player.translation,
            radius,
            subtracted,
            &mut state.scratch,
        );
        if state.scratch != state.columns || state.origin != origin {
            std::mem::swap(&mut state.columns, &mut state.scratch);
            state.origin = origin;
            rebuild = true;
        }
    }
    for (sheet, mut transform, mut visibility, mut tag) in &mut sheets {
        tag.set_if_neq(precipitation_tag(strength, radius, sheet.kind));
        if !rebuild {
            continue;
        }
        let built = precipitation_mesh(&state.columns, sheet.kind, origin);
        visibility.set_if_neq(if built.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(built) = built
            && let Some(mut mesh) = meshes.get_mut(&sheet.mesh)
        {
            *mesh = built;
            transform.translation = origin.as_vec3();
        }
    }
}

/// `RenderLightningBolt`: a main trunk of eight 16-block segments and two
/// side branches, each drawn four times at growing widths. Positions are
/// relative to the strike, whose x and z are already a block's center.
pub fn bolt_mesh(seed: i64) -> Mesh {
    let mut xs = [0.0_f32; 8];
    let mut zs = [0.0_f32; 8];
    let (mut end_x, mut end_z) = (0.0_f32, 0.0_f32);
    let mut random = JavaRandom::new(seed as u64);
    let step =
        |random: &mut JavaRandom, bound: u32| random.next_int(bound) as f32 - (bound / 2) as f32;
    for segment in (0..8).rev() {
        xs[segment] = end_x;
        zs[segment] = end_z;
        end_x += step(&mut random, 11);
        end_z += step(&mut random, 11);
    }

    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for pass in 0..4 {
        let mut random = JavaRandom::new(seed as u64);
        for branch in 0..3_usize {
            let top = 7 - branch;
            let bottom = if branch > 0 { top - 2 } else { 0 };
            let mut x = xs[top] - end_x;
            let mut z = zs[top] - end_z;
            for segment in (bottom..=top).rev() {
                let (upper_x, upper_z) = (x, z);
                let bound = if branch == 0 { 11 } else { 31 };
                x += step(&mut random, bound);
                z += step(&mut random, bound);
                let width = 0.1 + pass as f32 * 0.2;
                let (upper_width, lower_width) = if branch == 0 {
                    (
                        width * (segment as f32 * 0.1 + 1.0),
                        width * ((segment as f32 - 1.0) * 0.1 + 1.0),
                    )
                } else {
                    (width, width)
                };
                let lower_y = segment as f32 * 16.0;
                let upper_y = lower_y + 16.0;
                let corner = |index: usize, half: f32| {
                    let east = index == 1 || index == 2;
                    let south = index == 2 || index == 3;
                    (
                        if east { half } else { -half },
                        if south { half } else { -half },
                    )
                };
                // Beta's triangle strip walks the four corners and back to
                // the first; that is one quad per side of the square tube.
                for side in 0..4 {
                    let base = positions.len() as u32;
                    for index in [side, (side + 1) % 4] {
                        let (lower_dx, lower_dz) = corner(index, lower_width);
                        let (upper_dx, upper_dz) = corner(index, upper_width);
                        positions.push([lower_dx + x, lower_y, lower_dz + z]);
                        positions.push([upper_dx + upper_x, upper_y, upper_dz + upper_z]);
                    }
                    indices.extend([base, base + 1, base + 2, base + 2, base + 1, base + 3]);
                }
            }
        }
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
    .with_inserted_indices(Indices::U32(indices))
}

fn update_lightning(
    mut commands: Commands,
    mut strikes: MessageReader<LightningStrike>,
    mut bolts: Query<(Entity, &mut LightningBolt, &mut Mesh3d)>,
    tick: Res<WorldTick>,
    material: Res<BoltMaterial>,
    mut flash: ResMut<SkyFlash>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // `EntityLightningBolt.onUpdate`, less the fire and damage.
    for _ in 0..tick.ticks_this_frame() {
        if flash.0 > 0 {
            flash.0 -= 1;
        }
        for (entity, mut bolt, mut mesh) in &mut bolts {
            bolt.state -= 1;
            if bolt.state < 0 {
                if bolt.living_time == 0 {
                    commands.entity(entity).despawn();
                    continue;
                }
                if bolt.state < -(bolt.random.next_int(10) as i32) {
                    bolt.living_time -= 1;
                    bolt.state = 1;
                    mesh.0 = meshes.add(bolt_mesh(bolt.random.next_long()));
                }
            }
            if bolt.state >= 0 {
                flash.0 = 2;
            }
        }
    }
    for &LightningStrike(position) in strikes.read() {
        let mut random = JavaRandom::new(
            tick.world_time()
                ^ u64::from(position.x.to_bits()) << 32
                ^ u64::from(position.z.to_bits()),
        );
        let seed = random.next_long();
        let living_time = random.next_int(3) + 1;
        commands.spawn((
            Name::new("Lightning bolt"),
            LightningBolt {
                state: 2,
                living_time,
                random,
            },
            Mesh3d(meshes.add(bolt_mesh(seed))),
            MeshMaterial3d(material.0.clone()),
            Transform::from_translation(position),
            NoFrustumCulling,
        ));
    }
}

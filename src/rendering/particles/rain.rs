//! Rain splashes from `EntityRenderer.addRainParticles`: each world tick of
//! rain scatters `EntityRainFX` drops on the ground and water around the
//! player, and `EntitySmokeFX` puffs where rain meets lava. One dynamic mesh
//! draws the bounded pool from `particles.png`.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use super::registry::ParticleAtlas;
use super::registry::ParticleSprite;
use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::block::fluids::is_liquid;
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::random::JavaRandom;
use crate::rendering::weather::Precipitation;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::lighting::LightCache;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::column_channels;
use crate::world::lighting::combined_light;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;

/// Full rain adds 100 a tick and a splash lasts at most 40, so the pool never
/// fills in practice; the cap only bounds the mesh.
const MAX_PARTICLES: usize = 4_000;
/// How far from the player, in blocks, `addRainParticles` looks on each axis.
const REACH: i32 = 10;

pub struct RainParticlePlugin;

impl Plugin for RainParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RainParticles>()
            .init_asset::<StandardMaterial>()
            .add_systems(Startup, setup_renderer)
            .add_systems(
                Update,
                update_particles
                    .after(PhysicsSet::ApplyInput)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Splash,
    Smoke,
}

#[derive(Clone, Copy)]
struct Particle {
    kind: Kind,
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    age: u8,
    max_age: u8,
    size: f32,
    sprite: ParticleSprite,
    /// Smoke's own gray; splashes are white.
    shade: f32,
    /// World light at the particle, sampled each tick.
    brightness: f32,
    on_ground: bool,
}

/// The splashes and smoke puffs rain is making around the player.
#[derive(Resource)]
pub struct RainParticles {
    active: VecDeque<Particle>,
    random: JavaRandom,
}

impl Default for RainParticles {
    fn default() -> Self {
        Self {
            active: VecDeque::new(),
            random: JavaRandom::new(0x5241_494e),
        }
    }
}

impl RainParticles {
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    fn push(&mut self, particle: Particle) {
        if self.active.len() == MAX_PARTICLES {
            self.active.pop_front();
        }
        self.active.push_back(particle);
    }

    /// One tick of `addRainParticles`. `strength` is the rain strength and
    /// `eye` the player's eye, which Beta measures both reach and height from.
    pub fn spawn(&mut self, chunks: &WorldChunks, eye: Vec3, strength: f32, fancy: bool) {
        let strength = if fancy { strength } else { strength / 2.0 };
        let center = eye.floor().as_ivec3();
        for _ in 0..(100.0 * strength * strength) as i32 {
            let mut spread = || {
                self.random.next_int(REACH as u32) as i32
                    - self.random.next_int(REACH as u32) as i32
            };
            let x = center.x + spread();
            let z = center.z + spread();
            let top = chunks.top_solid_block(x, z);
            if top > center.y + REACH
                || top < center.y - REACH
                || Precipitation::at(chunks, x, z) != Some(Precipitation::Rain)
            {
                continue;
            }
            let offset_x = self.random.next_float();
            let offset_z = self.random.next_float();
            let Some(below) = chunks
                .block_at(x, top - 1, z)
                .filter(|&block| block != Block::Air)
            else {
                continue;
            };
            let position = Vec3::new(x as f32 + offset_x, top as f32 + 0.1, z as f32 + offset_z);
            let particle = if Fluid::of(below) == Some(Fluid::Lava) {
                self.smoke(position)
            } else {
                self.splash(position)
            };
            self.push(particle);
        }
    }

    /// `EntityFX`'s constructor: a random direction at a random speed, with
    /// a little lift, and a random size.
    fn base(&mut self, kind: Kind, position: Vec3) -> Particle {
        let mut unit = || self.random.next_float();
        let direction = Vec3::new(
            (unit() * 2.0 - 1.0) * 0.4,
            (unit() * 2.0 - 1.0) * 0.4,
            (unit() * 2.0 - 1.0) * 0.4,
        );
        let speed = (unit() + unit() + 1.0) * 0.15;
        let mut velocity = direction.normalize_or_zero() * speed * 0.4;
        velocity.y += 0.1;
        let size = (unit() * 0.5 + 0.5) * 2.0;
        let max_age = (4.0 / (unit() * 0.9 + 0.1)) as u8;
        Particle {
            kind,
            position,
            previous_position: position,
            velocity,
            age: 0,
            max_age,
            size,
            sprite: ParticleSprite::WaterSplash(0),
            shade: 1.0,
            brightness: 1.0,
            on_ground: false,
        }
    }

    /// `EntityRainFX`.
    fn splash(&mut self, position: Vec3) -> Particle {
        let mut particle = self.base(Kind::Splash, position);
        particle.velocity.x *= 0.3;
        particle.velocity.y = self.random.next_float() * 0.2 + 0.1;
        particle.velocity.z *= 0.3;
        // Beta's tiles 19 to 22, the four splash frames after the gap.
        particle.sprite = ParticleSprite::WaterSplash(2 + self.random.next_int(4) as u8);
        particle.max_age = (8.0 / (self.random.next_float() * 0.8 + 0.2)) as u8;
        particle
    }

    /// `EntitySmokeFX` at its default scale.
    fn smoke(&mut self, position: Vec3) -> Particle {
        let mut particle = self.base(Kind::Smoke, position);
        particle.velocity *= 0.1;
        particle.shade = self.random.next_float() * 0.3;
        particle.size *= 0.75;
        particle.max_age = (8.0 / (self.random.next_float() * 0.8 + 0.2)) as u8;
        particle.sprite = ParticleSprite::Explosion(7);
        particle
    }

    fn tick(&mut self, chunks: &WorldChunks, light: Option<&LightCache>, subtracted: u8) {
        let Self { active, random } = self;
        active.retain_mut(|particle| {
            particle.tick(chunks, random) && {
                let cell = particle.position.floor().as_ivec3();
                let (sky, block) = light
                    .and_then(|light| light.channels(cell.x, cell.y, cell.z))
                    .unwrap_or_else(|| column_channels(chunks, cell.x, cell.y, cell.z));
                particle.brightness = beta_brightness(
                    combined_light(sky, block, subtracted),
                    crate::world::dimension::Dimension::Overworld.ambient_light(),
                );
                true
            }
        });
    }
}

impl Particle {
    /// `onUpdate`; false once the particle is dead.
    fn tick(&mut self, chunks: &WorldChunks, random: &mut JavaRandom) -> bool {
        self.previous_position = self.position;
        match self.kind {
            Kind::Splash => {
                self.velocity.y -= 0.06;
                self.travel(chunks);
                self.velocity *= 0.98;
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                if self.on_ground {
                    if random.next_float() < 0.5 {
                        return false;
                    }
                    self.velocity.x *= 0.7;
                    self.velocity.z *= 0.7;
                }
                // A drop that sinks below the surface of what it landed in is gone.
                let cell = self.position.floor().as_ivec3();
                if let Some(block) = chunks.block_at(cell.x, cell.y, cell.z)
                    && (is_liquid(block) || block.is_solid_material())
                {
                    let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
                    let level = if metadata >= 8 { 0 } else { metadata };
                    let surface = (cell.y + 1) as f32 - f32::from(level + 1) / 9.0;
                    if self.position.y < surface {
                        return false;
                    }
                }
            }
            Kind::Smoke => {
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                // The puff thins out through the eight frames, last to first.
                let frame = 7 - u32::from(self.age) * 8 / u32::from(self.max_age.max(1));
                self.sprite = ParticleSprite::Explosion(frame.min(7) as u8);
                self.velocity.y += 0.004;
                self.travel(chunks);
                if self.position.y == self.previous_position.y {
                    self.velocity.x *= 1.1;
                    self.velocity.z *= 1.1;
                }
                self.velocity *= 0.96;
                if self.on_ground {
                    self.velocity.x *= 0.7;
                    self.velocity.z *= 0.7;
                }
            }
        }
        true
    }

    /// `moveEntity` for a point: stop on whichever axis runs into a block.
    fn travel(&mut self, chunks: &WorldChunks) {
        self.on_ground = false;
        let next_x = self.position + Vec3::X * self.velocity.x;
        if collides(chunks, next_x) {
            self.velocity.x = 0.0;
        } else {
            self.position.x = next_x.x;
        }
        let next_z = self.position + Vec3::Z * self.velocity.z;
        if collides(chunks, next_z) {
            self.velocity.z = 0.0;
        } else {
            self.position.z = next_z.z;
        }
        let next_y = self.position + Vec3::Y * self.velocity.y;
        if collides(chunks, next_y) {
            self.on_ground = self.velocity.y < 0.0;
            self.velocity.y = 0.0;
        } else {
            self.position.y = next_y.y;
        }
    }
}

fn collides(chunks: &WorldChunks, point: Vec3) -> bool {
    chunks
        .block_at(
            point.x.floor() as i32,
            point.y.floor() as i32,
            point.z.floor() as i32,
        )
        .is_some_and(Block::blocks_movement)
}

#[derive(Resource)]
struct RainRenderer {
    entity: Entity,
    mesh: Handle<Mesh>,
    has_geometry: bool,
}

fn setup_renderer(
    mut commands: Commands,
    atlas: Option<Res<ParticleAtlas>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Bevy 0.19 does not allocate zero-vertex meshes but may still try to
    // upload their data. Keep a nonempty mesh allocated and hide it while idle.
    let mesh = meshes.add(placeholder_mesh());
    let material = materials.add(StandardMaterial {
        base_color_texture: atlas.map(|atlas| atlas.0.clone()),
        alpha_mode: AlphaMode::Mask(0.1),
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let entity = commands
        .spawn((
            Name::new("Rain particles"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Visibility::Hidden,
            NoFrustumCulling,
        ))
        .id();
    commands.insert_resource(RainRenderer {
        entity,
        mesh,
        has_geometry: false,
    });
}

fn update_particles(
    tick: Res<WorldTick>,
    weather: Option<Res<WorldWeather>>,
    settings: Option<Res<GameSettings>>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    player: Query<&Transform, With<Player>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    mut particles: ResMut<RainParticles>,
    mut renderer: ResMut<RainRenderer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visibility: Query<&mut Visibility>,
) {
    let strength = weather
        .as_ref()
        .map_or(0.0, |weather| weather.rain_strength);
    if strength == 0.0 && particles.active.is_empty() {
        if renderer.has_geometry {
            if let Ok(mut visible) = visibility.get_mut(renderer.entity) {
                *visible = Visibility::Hidden;
            }
            renderer.has_geometry = false;
        }
        return;
    }
    let fancy = settings
        .as_ref()
        .is_none_or(|settings| settings.graphics.fancy_leaves());
    let subtracted = crate::world::weather::skylight_subtracted(
        weather.as_deref(),
        celestial_angle(tick.world_time(), 0.0),
    );
    for _ in 0..tick.ticks_this_frame() {
        particles.tick(&chunks, light.as_deref(), subtracted);
        if strength > 0.0
            && let Ok(player) = player.single()
        {
            particles.spawn(&chunks, player.translation, strength, fancy);
        }
    }
    let rotation = camera
        .single()
        .map_or(Quat::IDENTITY, GlobalTransform::rotation);
    let has_geometry = !particles.active.is_empty();
    if has_geometry && let Some(mut mesh) = meshes.get_mut(&renderer.mesh) {
        *mesh = particle_mesh(
            particles.active.iter(),
            rotation * Vec3::X,
            rotation * Vec3::Y,
            tick.partial(),
        );
    }
    if renderer.has_geometry != has_geometry {
        if let Ok(mut visible) = visibility.get_mut(renderer.entity) {
            *visible = if has_geometry {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        renderer.has_geometry = has_geometry;
    }
}

fn placeholder_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0; 4]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; 3])
    .with_inserted_indices(Indices::U32(vec![0, 1, 2]))
}

fn particle_mesh<'a>(
    particles: impl ExactSizeIterator<Item = &'a Particle>,
    right: Vec3,
    up: Vec3,
    partial: f32,
) -> Mesh {
    let count = particles.len();
    let mut positions = Vec::with_capacity(count * 4);
    let mut normals = Vec::with_capacity(count * 4);
    let mut colors = Vec::with_capacity(count * 4);
    let mut uvs = Vec::with_capacity(count * 4);
    let mut indices = Vec::with_capacity(count * 6);
    let normal = right.cross(up);
    for particle in particles {
        let base = positions.len() as u32;
        // `EntityFX.renderParticle` draws a tenth of the particle's scale.
        let r = right * particle.size * 0.1;
        let u = up * particle.size * 0.1;
        let center = particle.previous_position.lerp(particle.position, partial);
        for point in [
            center - r - u,
            center + r - u,
            center + r + u,
            center - r + u,
        ] {
            positions.push(point.to_array());
        }
        normals.extend([normal.to_array(); 4]);
        // Beta multiplies in sRGB space; vertex colors are linear.
        let shade = particle.shade * particle.brightness;
        colors.extend([Color::srgb(shade, shade, shade).to_linear().to_f32_array(); 4]);
        let (u0, v0, u1, v1) = particle.sprite.uvs().unwrap_or((0.0, 0.0, 0.0, 0.0));
        uvs.extend([[u0, v1], [u1, v1], [u1, v0], [u0, v0]]);
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // Rebuilt every frame while particles live, so keep no main-world copy
    // for extraction to clone.
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

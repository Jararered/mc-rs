//! Beta's `EntityFX` particles: rain splashes, smoke, flames, lava pops,
//! redstone dust, portal swirls, explosion puffs, bubbles, splashes, hearts
//! and notes. Producers call [`EffectParticles::spawn`], which is
//! `World.spawnParticle`: it drops anything more than 16 blocks from the
//! viewer and builds the particle from its Java constructor. One dynamic mesh
//! draws the bounded pool from `particles.png`.
//!
//! Beta also has slime and snowball drops, which are item-sheet sprites and
//! are not drawn yet.

use std::collections::VecDeque;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use super::display::random_display_updates;
use super::registry::ParticleAtlas;
use super::registry::ParticleSprite;
use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::block::fluids::is_liquid;
use crate::block::fluids::is_water;
use crate::entity::projectiles::gaussian;
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
/// `RenderGlobal.spawnParticle` ignores effects farther than this from the
/// view entity.
const SPAWN_RANGE: f32 = 16.0;

pub struct EffectParticlePlugin;

impl Plugin for EffectParticlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EffectParticles>()
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

/// The names `World.spawnParticle` accepts that this game draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FxKind {
    Bubble,
    Smoke,
    /// `largesmoke`: smoke at two and a half times the scale.
    LargeSmoke,
    /// The velocity's x is the pitch's hue, `pitch / 24`.
    Note,
    Portal,
    Explode,
    Flame,
    Lava,
    Splash,
    /// The velocity is the dust's red, green and blue.
    Reddust,
    Heart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    /// `EntityRainFX`, and `EntitySplashFX` with a different gravity.
    Splash,
    Smoke,
    Reddust,
    Flame,
    Lava,
    Portal,
    Explode,
    Bubble,
    Heart,
    Note,
}

#[derive(Clone, Copy)]
struct Particle {
    kind: Kind,
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    /// Where a portal particle orbits from.
    origin: Vec3,
    age: u16,
    max_age: u16,
    /// `particleScale`, before the per-kind growth and fade in
    /// [`Self::render_scale`].
    size: f32,
    gravity: f32,
    sprite: ParticleSprite,
    /// `particleRed`, `Green` and `Blue`.
    color: Vec3,
    /// World light at the particle, sampled each tick.
    brightness: f32,
    on_ground: bool,
    no_clip: bool,
}

/// The effects drawn around the player.
#[derive(Resource)]
pub struct EffectParticles {
    active: VecDeque<Particle>,
    random: JavaRandom,
    /// `World.rand` as `randomDisplayUpdates` draws from it.
    pub(super) display_random: JavaRandom,
    /// The view entity `spawn` measures its range from; `None` accepts all.
    viewer: Option<Vec3>,
}

impl Default for EffectParticles {
    fn default() -> Self {
        Self {
            active: VecDeque::new(),
            random: JavaRandom::new(0x5241_494e),
            display_random: JavaRandom::new(0x4449_5350),
            viewer: None,
        }
    }
}

impl EffectParticles {
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Where the player's view is, for the 16-block range check.
    pub fn set_viewer(&mut self, viewer: Option<Vec3>) {
        self.viewer = viewer;
    }

    fn push(&mut self, particle: Particle) {
        if self.active.len() == MAX_PARTICLES {
            self.active.pop_front();
        }
        self.active.push_back(particle);
    }

    /// `World.spawnParticle`.
    pub fn spawn(&mut self, kind: FxKind, position: Vec3, velocity: Vec3) {
        if let Some(viewer) = self.viewer
            && viewer.distance_squared(position) > SPAWN_RANGE * SPAWN_RANGE
        {
            return;
        }
        self.spawn_unchecked(kind, position, velocity);
    }

    fn spawn_unchecked(&mut self, kind: FxKind, position: Vec3, velocity: Vec3) {
        let particle = match kind {
            FxKind::Bubble => self.bubble(position, velocity),
            FxKind::Smoke => self.smoke(position, velocity, 1.0),
            FxKind::LargeSmoke => self.smoke(position, velocity, 2.5),
            FxKind::Note => self.note(position, velocity),
            FxKind::Portal => self.portal(position, velocity),
            FxKind::Explode => self.explode(position, velocity),
            FxKind::Flame => self.flame(position, velocity),
            FxKind::Lava => self.lava(position),
            FxKind::Splash => self.water_splash(position, velocity),
            FxKind::Reddust => self.reddust(position, velocity, 1.0),
            FxKind::Heart => self.heart(position, 2.0),
        };
        self.push(particle);
    }

    /// `Explosion.doExplosionB`'s particles for one destroyed cell: a puff
    /// halfway to the center and smoke at the debris.
    pub fn blast_cell(&mut self, cell: IVec3, center: Vec3, strength: f32) {
        let base = cell.as_vec3();
        let spot = base
            + Vec3::new(
                self.random.next_float(),
                self.random.next_float(),
                self.random.next_float(),
            );
        let offset = spot - center;
        let distance = offset.length();
        let direction = offset.normalize_or_zero();
        let mut speed = 0.5 / (distance / strength + 0.1);
        speed *= self.random.next_float() * self.random.next_float() + 0.3;
        let velocity = direction * speed;
        self.spawn(FxKind::Explode, (spot + center) / 2.0, velocity);
        self.spawn(FxKind::Smoke, spot, velocity);
    }

    /// `Entity.onEntityUpdate`'s entry into water: bubbles and splashes
    /// along the surface the body crossed, `floor` being its feet's block.
    pub fn water_entry(&mut self, position: Vec3, floor: f32, width: f32, motion: Vec3) {
        let count = (1.0 + width * 20.0).ceil() as usize;
        for _ in 0..count {
            let dx = (self.random.next_float() * 2.0 - 1.0) * width;
            let dz = (self.random.next_float() * 2.0 - 1.0) * width;
            let sink = self.random.next_float() * 0.2;
            let at = Vec3::new(position.x + dx, floor + 1.0, position.z + dz);
            self.spawn(
                FxKind::Bubble,
                at,
                Vec3::new(motion.x, motion.y - sink, motion.z),
            );
        }
        for _ in 0..count {
            let dx = (self.random.next_float() * 2.0 - 1.0) * width;
            let dz = (self.random.next_float() * 2.0 - 1.0) * width;
            let at = Vec3::new(position.x + dx, floor + 1.0, position.z + dz);
            self.spawn(FxKind::Splash, at, motion);
        }
    }

    /// `EntityBoat.onUpdate`'s wake: splashes along the hull once it moves
    /// faster than 0.15 blocks a tick. `yaw` is the boat's heading in degrees.
    pub fn boat_wake(&mut self, center: Vec3, yaw: f32, speed: f32, motion: Vec3) {
        if speed <= 0.15 {
            return;
        }
        let (sin, cos) = yaw.to_radians().sin_cos();
        let mut index = 0.0;
        while index < 1.0 + speed * 60.0 {
            index += 1.0;
            let along = self.random.next_float() * 2.0 - 1.0;
            let side = (self.random.next_int(2) as f32 * 2.0 - 1.0) * 0.7;
            let (x, z) = if self.random.next_int(2) == 0 {
                (
                    center.x - cos * along * 0.8 + sin * side,
                    center.z - sin * along * 0.8 - cos * side,
                )
            } else {
                (
                    center.x + cos + sin * along * 0.7,
                    center.z + sin - cos * along * 0.7,
                )
            };
            self.spawn(FxKind::Splash, Vec3::new(x, center.y - 0.125, z), motion);
        }
    }

    /// `EntityLiving.onEntityUpdate`'s last gasp: eight bubbles round the
    /// body as it takes a drowning hit.
    pub fn drown(&mut self, position: Vec3, motion: Vec3) {
        for _ in 0..8 {
            let mut spread = || self.random.next_float() - self.random.next_float();
            let offset = Vec3::new(spread(), spread(), spread());
            self.spawn(FxKind::Bubble, position + offset, motion);
        }
    }

    /// The twenty `explode` puffs a creature leaves when its death animation
    /// ends. `feet` is the bottom center of its box.
    pub fn death_puffs(&mut self, feet: Vec3, width: f32, height: f32) {
        for _ in 0..20 {
            let velocity = self.gaussian_velocity();
            let at = self.body_point(feet, width, height, 0.0);
            self.spawn(FxKind::Explode, at, velocity);
        }
    }

    /// `EntityWolf.showHeartsOrSmokeFX`: seven hearts for a tamed wolf, smoke
    /// for a bone that did not take.
    pub fn tame_burst(&mut self, feet: Vec3, width: f32, height: f32, tamed: bool) {
        for _ in 0..7 {
            let velocity = self.gaussian_velocity();
            let at = self.body_point(feet, width, height, 0.5);
            let kind = if tamed { FxKind::Heart } else { FxKind::Smoke };
            self.spawn(kind, at, velocity);
        }
    }

    /// `rand.nextGaussian() * 0.02` on each axis.
    fn gaussian_velocity(&mut self) -> Vec3 {
        let mut axis = || gaussian(&mut self.random) * 0.02;
        Vec3::new(axis(), axis(), axis())
    }

    /// A random point in a creature's box: `posX + rand * width * 2 - width`.
    fn body_point(&mut self, feet: Vec3, width: f32, height: f32, lift: f32) -> Vec3 {
        Vec3::new(
            feet.x + self.random.next_float() * width * 2.0 - width,
            feet.y + lift + self.random.next_float() * height,
            feet.z + self.random.next_float() * width * 2.0 - width,
        )
    }

    /// `TileEntityMobSpawner.updateEntity`: smoke and fire at a random point
    /// in the cage's cell every tick a player is near.
    pub fn spawner_idle(&mut self, cell: IVec3) {
        let at = cell.as_vec3()
            + Vec3::new(
                self.random.next_float(),
                self.random.next_float(),
                self.random.next_float(),
            );
        self.spawn(FxKind::Smoke, at, Vec3::ZERO);
        self.spawn(FxKind::Flame, at, Vec3::ZERO);
    }

    /// The twenty puffs of smoke and fire when a spawner releases a mob.
    pub fn spawner_burst(&mut self, cell: IVec3) {
        for _ in 0..20 {
            let mut spread = || (self.random.next_float() - 0.5) * 2.0;
            let at = cell.as_vec3() + Vec3::splat(0.5) + Vec3::new(spread(), spread(), spread());
            self.spawn(FxKind::Smoke, at, Vec3::ZERO);
            self.spawn(FxKind::Flame, at, Vec3::ZERO);
        }
    }

    /// One tick of `addRainParticles`. `strength` is the rain strength and
    /// `eye` the player's eye, which Beta measures both reach and height from.
    pub fn spawn_rain(&mut self, chunks: &WorldChunks, eye: Vec3, strength: f32, fancy: bool) {
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
                self.smoke(position, Vec3::ZERO, 1.0)
            } else {
                self.rain_drop(position)
            };
            self.push(particle);
        }
    }

    /// `EntityFX`'s constructor: the given velocity plus a random push,
    /// renormalised to a random speed with a little lift, and a random size.
    fn base(&mut self, kind: Kind, position: Vec3, velocity: Vec3) -> Particle {
        let mut unit = || self.random.next_float();
        let jitter = Vec3::new(
            (unit() * 2.0 - 1.0) * 0.4,
            (unit() * 2.0 - 1.0) * 0.4,
            (unit() * 2.0 - 1.0) * 0.4,
        );
        let direction = velocity + jitter;
        let speed = (unit() + unit() + 1.0) * 0.15;
        let mut velocity = direction.normalize_or_zero() * speed * 0.4;
        velocity.y += 0.1;
        let size = (unit() * 0.5 + 0.5) * 2.0;
        let max_age = (4.0 / (unit() * 0.9 + 0.1)) as u16;
        Particle {
            kind,
            position,
            previous_position: position,
            velocity,
            origin: position,
            age: 0,
            max_age,
            size,
            gravity: 0.0,
            sprite: ParticleSprite::WaterSplash(0),
            color: Vec3::ONE,
            brightness: 1.0,
            on_ground: false,
            no_clip: false,
        }
    }

    /// `8 / (Math.random() * 0.8 + 0.2)`, the lifetime most effects share.
    fn lifetime(&mut self) -> f32 {
        8.0 / (self.random.next_float() * 0.8 + 0.2)
    }

    /// `EntityRainFX`.
    fn rain_drop(&mut self, position: Vec3) -> Particle {
        let mut particle = self.base(Kind::Splash, position, Vec3::ZERO);
        particle.velocity.x *= 0.3;
        particle.velocity.y = self.random.next_float() * 0.2 + 0.1;
        particle.velocity.z *= 0.3;
        particle.gravity = 0.06;
        // Beta's tiles 19 to 22, the four splash frames after the gap.
        particle.sprite = ParticleSprite::WaterSplash(2 + self.random.next_int(4) as u8);
        particle.max_age = self.lifetime() as u16;
        particle
    }

    /// `EntitySplashFX`: a rain drop that falls slower and one tile along.
    fn water_splash(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.rain_drop(position);
        particle.gravity = 0.04;
        // Tile 23 is empty in the sheet; Beta draws nothing for that quarter
        // of splashes, which is clamped to the last frame here.
        if let ParticleSprite::WaterSplash(frame) = particle.sprite {
            particle.sprite = ParticleSprite::WaterSplash((frame + 1).min(5));
        }
        if velocity.y == 0.0 && (velocity.x != 0.0 || velocity.z != 0.0) {
            particle.velocity = Vec3::new(velocity.x, velocity.y + 0.1, velocity.z);
        }
        particle
    }

    /// `EntitySmokeFX`.
    fn smoke(&mut self, position: Vec3, velocity: Vec3, scale: f32) -> Particle {
        let mut particle = self.base(Kind::Smoke, position, Vec3::ZERO);
        particle.velocity = particle.velocity * 0.1 + velocity;
        particle.color = Vec3::splat(self.random.next_float() * 0.3);
        particle.size *= 0.75 * scale;
        particle.max_age = (self.lifetime() as u16 as f32 * scale) as u16;
        particle.sprite = ParticleSprite::Explosion(7);
        particle
    }

    /// `EntityReddustFX`: `color` is the red, green and blue the caller asked
    /// for, and a zero red means full red.
    fn reddust(&mut self, position: Vec3, color: Vec3, scale: f32) -> Particle {
        let mut particle = self.base(Kind::Reddust, position, Vec3::ZERO);
        particle.velocity *= 0.1;
        let red = if color.x == 0.0 { 1.0 } else { color.x };
        let shade = self.random.next_float() * 0.4 + 0.6;
        let mut channel = |level: f32| (self.random.next_float() * 0.2 + 0.8) * level * shade;
        particle.color = Vec3::new(channel(red), channel(color.y), channel(color.z));
        particle.size *= 0.75 * scale;
        particle.max_age = (self.lifetime() as u16 as f32 * scale) as u16;
        particle.sprite = ParticleSprite::Explosion(7);
        particle
    }

    /// `EntityFlameFX`.
    fn flame(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.base(Kind::Flame, position, velocity);
        particle.velocity = particle.velocity * 0.01 + velocity;
        particle.max_age = self.lifetime() as u16 + 4;
        particle.no_clip = true;
        particle.sprite = ParticleSprite::Flame;
        particle
    }

    /// `EntityLavaFX`.
    fn lava(&mut self, position: Vec3) -> Particle {
        let mut particle = self.base(Kind::Lava, position, Vec3::ZERO);
        particle.velocity *= 0.8;
        particle.velocity.y = self.random.next_float() * 0.4 + 0.05;
        particle.size *= self.random.next_float() * 2.0 + 0.2;
        particle.max_age = (16.0 / (self.random.next_float() * 0.8 + 0.2)) as u16;
        particle.sprite = ParticleSprite::Lava;
        particle
    }

    /// `EntityPortalFX`: the velocity is how far the particle drifts from
    /// where it appeared over its life.
    fn portal(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.base(Kind::Portal, position, velocity);
        particle.velocity = velocity;
        let level = self.random.next_float() * 0.6 + 0.4;
        particle.size = self.random.next_float() * 0.2 + 0.5;
        particle.color = Vec3::new(level * 0.9, level * 0.3, level);
        particle.max_age = self.random.next_int(10) as u16 + 40;
        particle.no_clip = true;
        particle.sprite = ParticleSprite::Explosion(self.random.next_int(8) as u8);
        particle
    }

    /// `EntityExplodeFX`.
    fn explode(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.base(Kind::Explode, position, velocity);
        let mut jitter = || (self.random.next_float() * 2.0 - 1.0) * 0.05;
        particle.velocity = velocity + Vec3::new(jitter(), jitter(), jitter());
        particle.color = Vec3::splat(self.random.next_float() * 0.3 + 0.7);
        particle.size = self.random.next_float() * self.random.next_float() * 6.0 + 1.0;
        particle.max_age = (16.0 / (self.random.next_float() * 0.8 + 0.2)) as u16 + 2;
        particle.sprite = ParticleSprite::Explosion(0);
        particle
    }

    /// `EntityBubbleFX`.
    fn bubble(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.base(Kind::Bubble, position, velocity);
        particle.sprite = ParticleSprite::AirBubble;
        particle.size *= self.random.next_float() * 0.6 + 0.2;
        let mut jitter = || (self.random.next_float() * 2.0 - 1.0) * 0.02;
        particle.velocity = velocity * 0.2 + Vec3::new(jitter(), jitter(), jitter());
        particle.max_age = self.lifetime() as u16;
        particle
    }

    /// `EntityHeartFX`.
    fn heart(&mut self, position: Vec3, scale: f32) -> Particle {
        let mut particle = self.base(Kind::Heart, position, Vec3::ZERO);
        particle.velocity *= 0.01;
        particle.velocity.y += 0.1;
        particle.size *= 0.75 * scale;
        particle.max_age = 16;
        particle.sprite = ParticleSprite::HealthHeart;
        particle
    }

    /// `EntityNoteFX`; `velocity.x` is the hue.
    fn note(&mut self, position: Vec3, velocity: Vec3) -> Particle {
        let mut particle = self.base(Kind::Note, position, Vec3::ZERO);
        particle.velocity *= 0.01;
        particle.velocity.y += 0.2;
        let wave =
            |offset: f32| ((velocity.x + offset) * std::f32::consts::PI * 2.0).sin() * 0.65 + 0.35;
        particle.color = Vec3::new(wave(0.0), wave(1.0 / 3.0), wave(2.0 / 3.0));
        particle.size *= 0.75 * 2.0;
        particle.max_age = 6;
        particle.sprite = ParticleSprite::MusicNote;
        particle
    }

    /// Advance every particle one world tick.
    pub fn tick(&mut self, chunks: &WorldChunks, light: Option<&LightCache>, subtracted: u8) {
        let Self { active, random, .. } = self;
        let mut spawned: Vec<(FxKind, Vec3, Vec3)> = Vec::new();
        active.retain_mut(|particle| {
            particle.tick(chunks, random, &mut spawned) && {
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
        for (kind, position, velocity) in spawned {
            self.spawn_unchecked(kind, position, velocity);
        }
    }
}

impl Particle {
    /// `onUpdate`; false once the particle is dead.
    fn tick(
        &mut self,
        chunks: &WorldChunks,
        random: &mut JavaRandom,
        spawned: &mut Vec<(FxKind, Vec3, Vec3)>,
    ) -> bool {
        self.previous_position = self.position;
        match self.kind {
            Kind::Splash => {
                self.velocity.y -= self.gravity;
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
                if !self.age_frame() {
                    return false;
                }
                self.velocity.y += 0.004;
                self.travel(chunks);
                self.spread_on_flat();
                self.velocity *= 0.96;
                self.ground_friction();
            }
            Kind::Reddust => {
                if !self.age_frame() {
                    return false;
                }
                self.travel(chunks);
                self.spread_on_flat();
                self.velocity *= 0.96;
                self.ground_friction();
            }
            Kind::Explode => {
                if !self.age_frame() {
                    return false;
                }
                self.velocity.y += 0.004;
                self.travel(chunks);
                self.velocity *= 0.9;
                self.ground_friction();
            }
            Kind::Flame => {
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                self.travel(chunks);
                self.velocity *= 0.96;
                self.ground_friction();
            }
            Kind::Lava => {
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                let life = f32::from(self.age) / f32::from(self.max_age.max(1));
                if random.next_float() > life {
                    spawned.push((FxKind::Smoke, self.position, self.velocity));
                }
                self.velocity.y -= 0.03;
                self.travel(chunks);
                self.velocity *= 0.999;
                self.ground_friction();
            }
            Kind::Portal => {
                let life = f32::from(self.age) / f32::from(self.max_age.max(1));
                let along = 1.0 - (-life + life * life * 2.0);
                self.position = self.origin + self.velocity * along + Vec3::Y * (1.0 - life);
                self.age += 1;
                if self.age >= self.max_age {
                    return false;
                }
            }
            Kind::Bubble => {
                self.velocity.y += 0.002;
                self.travel(chunks);
                self.velocity *= 0.85;
                let cell = self.position.floor().as_ivec3();
                if !chunks
                    .block_at(cell.x, cell.y, cell.z)
                    .is_some_and(is_water)
                {
                    return false;
                }
                if self.max_age == 0 {
                    return false;
                }
                self.max_age -= 1;
            }
            Kind::Heart => {
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                self.travel(chunks);
                self.spread_on_flat();
                self.velocity *= 0.86;
                self.ground_friction();
            }
            Kind::Note => {
                self.age += 1;
                if self.age > self.max_age {
                    return false;
                }
                self.travel(chunks);
                self.spread_on_flat();
                self.velocity *= 0.66;
                self.ground_friction();
            }
        }
        true
    }

    /// The `age++ >= maxAge` check and the eight-frame fade, last to first.
    fn age_frame(&mut self) -> bool {
        self.age += 1;
        if self.age > self.max_age {
            return false;
        }
        let frame = 7 - i32::from(self.age) * 8 / i32::from(self.max_age.max(1));
        self.sprite = ParticleSprite::Explosion(frame.clamp(0, 7) as u8);
        true
    }

    /// A particle that did not move vertically slides faster sideways.
    fn spread_on_flat(&mut self) {
        if self.position.y == self.previous_position.y {
            self.velocity.x *= 1.1;
            self.velocity.z *= 1.1;
        }
    }

    fn ground_friction(&mut self) {
        if self.on_ground {
            self.velocity.x *= 0.7;
            self.velocity.z *= 0.7;
        }
    }

    /// `moveEntity` for a point: stop on whichever axis runs into a block.
    fn travel(&mut self, chunks: &WorldChunks) {
        self.on_ground = false;
        if self.no_clip {
            self.position += self.velocity;
            return;
        }
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

    /// Progress through the particle's life, `(age + partial) / maxAge`.
    fn life(&self, partial: f32) -> f32 {
        (f32::from(self.age) + partial) / f32::from(self.max_age.max(1))
    }

    /// `renderParticle`'s per-kind scale: puffs and notes grow in over their
    /// first thirty-second, flames and lava shrink, portals ease in.
    fn render_scale(&self, partial: f32) -> f32 {
        let life = self.life(partial);
        match self.kind {
            Kind::Smoke | Kind::Reddust | Kind::Heart | Kind::Note => {
                self.size * (life * 32.0).clamp(0.0, 1.0)
            }
            Kind::Flame => self.size * (1.0 - life * life * 0.5),
            Kind::Lava => self.size * (1.0 - life * life),
            Kind::Portal => {
                let rest = 1.0 - life;
                self.size * (1.0 - rest * rest)
            }
            Kind::Splash | Kind::Explode | Kind::Bubble => self.size,
        }
    }

    /// `getEntityBrightness`: flames and portals brighten as they age, lava
    /// glows.
    fn render_brightness(&self, partial: f32) -> f32 {
        match self.kind {
            Kind::Lava => 1.0,
            Kind::Flame => {
                let life = self.life(partial).clamp(0.0, 1.0);
                self.brightness * life + (1.0 - life)
            }
            Kind::Portal => {
                let life = f32::from(self.age) / f32::from(self.max_age.max(1));
                let weight = life * life * life * life;
                self.brightness * (1.0 - weight) + weight
            }
            _ => self.brightness,
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
struct EffectRenderer {
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
            Name::new("Effect particles"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Visibility::Hidden,
            NoFrustumCulling,
        ))
        .id();
    commands.insert_resource(EffectRenderer {
        entity,
        mesh,
        has_geometry: false,
    });
}

#[allow(clippy::too_many_arguments)]
fn update_particles(
    tick: Res<WorldTick>,
    weather: Option<Res<WorldWeather>>,
    settings: Option<Res<GameSettings>>,
    chunks: Res<WorldChunks>,
    light: Option<Res<LightCache>>,
    player: Query<&Transform, With<Player>>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    mut particles: ResMut<EffectParticles>,
    mut renderer: ResMut<EffectRenderer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visibility: Query<&mut Visibility>,
) {
    let strength = weather
        .as_ref()
        .map_or(0.0, |weather| weather.rain_strength);
    let player = player.single().ok().map(|player| player.translation);
    let viewer = camera
        .single()
        .ok()
        .map(GlobalTransform::translation)
        .or(player);
    particles.set_viewer(viewer);
    let subtracted = crate::world::weather::skylight_subtracted(
        weather.as_deref(),
        celestial_angle(tick.world_time(), 0.0),
    );
    let fancy = settings
        .as_ref()
        .is_none_or(|settings| settings.graphics.fancy_leaves());
    for _ in 0..tick.ticks_this_frame() {
        particles.tick(&chunks, light.as_deref(), subtracted);
        if let Some(viewer) = viewer {
            random_display_updates(&mut particles, &chunks, viewer.floor().as_ivec3());
        }
        if strength > 0.0
            && let Some(player) = player
        {
            particles.spawn_rain(&chunks, player, strength, fancy);
        }
    }
    let has_geometry = !particles.active.is_empty();
    if has_geometry {
        let rotation = camera
            .single()
            .map_or(Quat::IDENTITY, GlobalTransform::rotation);
        if let Some(mut mesh) = meshes.get_mut(&renderer.mesh) {
            *mesh = particle_mesh(
                particles.active.iter(),
                rotation * Vec3::X,
                rotation * Vec3::Y,
                tick.partial(),
            );
        }
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
        let scale = particle.render_scale(partial) * 0.1;
        let r = right * scale;
        let u = up * scale;
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
        let shade = particle.color * particle.render_brightness(partial);
        colors.extend(
            [Color::srgb(shade.x, shade.y, shade.z)
                .to_linear()
                .to_f32_array(); 4],
        );
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

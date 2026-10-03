//! Animals: `EntityChicken`, `EntitySquid`, and `EntityWolf`. Pigs, cows, and
//! sheep need nothing beyond `EntityAnimal`'s wandering.

use std::f32::consts::PI;
use std::f32::consts::TAU;

use bevy::prelude::*;

use super::Body;
use super::Effects;
use super::Surroundings;
use super::Target;
use super::grow;
use crate::block::properties::is_opaque_cube;
use crate::entity::combat::Hit;
use crate::entity::combat::Source;
use crate::entity::combat::hurt_player;
use crate::entity::drops::items::spawn_entity_drop;
use crate::entity::pathfinding::Pathfinder;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::physics::WATER_CURRENT_PER_TICK;
use crate::physics::water_movement;
use crate::random::JavaRandom;

/// `EntityChicken`'s wing flap. Its render angle is
/// `(sin(rotation) + 1) * flap_speed`.
#[derive(Component, Clone, Debug)]
pub struct Wings {
    /// `wingRotation` and `oFlap`.
    pub rotation: f32,
    pub prev_rotation: f32,
    /// `destPos` and `oFlapSpeed`: 0 on the ground, rising to 1 in the air.
    pub flap_speed: f32,
    pub prev_flap_speed: f32,
    /// `wingRotDelta`.
    rotation_delta: f32,
}

impl Default for Wings {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            prev_rotation: 0.0,
            flap_speed: 0.0,
            prev_flap_speed: 0.0,
            rotation_delta: 1.0,
        }
    }
}

impl Wings {
    /// `RenderChicken.getWingRotation`.
    pub fn angle(&self, partial: f32) -> f32 {
        let rotation = self.prev_rotation + (self.rotation - self.prev_rotation) * partial;
        let speed = self.prev_flap_speed + (self.flap_speed - self.prev_flap_speed) * partial;
        (rotation.sin() + 1.0) * speed
    }
}

/// `EntitySquid`'s swimming pulse. Pitch and yaw are degrees, and only the
/// renderer reads them.
#[derive(Component, Clone, Debug)]
pub struct Swim {
    /// `squidPitch`: 0 upright, -90 lying on its side out of water.
    pub pitch: f32,
    pub prev_pitch: f32,
    /// `squidYaw`: a spin about the body's own axis.
    pub yaw: f32,
    pub prev_yaw: f32,
    /// `tentacleAngle`.
    pub tentacle: f32,
    pub prev_tentacle: f32,
    /// `squidRotation`: progress through one stroke, `0..TAU`.
    rotation: f32,
    rotation_velocity: f32,
    motion_speed: f32,
    rotate_speed: f32,
    /// `randomMotionVecX/Y/Z`.
    heading: Vec3,
}

impl Swim {
    pub fn new(rng: &mut JavaRandom) -> Self {
        Self {
            pitch: 0.0,
            prev_pitch: 0.0,
            yaw: 0.0,
            prev_yaw: 0.0,
            tentacle: 0.0,
            prev_tentacle: 0.0,
            rotation: 0.0,
            rotation_velocity: stroke_velocity(rng),
            motion_speed: 0.0,
            rotate_speed: 0.0,
            heading: Vec3::ZERO,
        }
    }
}

/// `1 / (nextFloat + 1) * 0.2`: how fast a squid strokes.
fn stroke_velocity(rng: &mut JavaRandom) -> f32 {
    1.0 / (rng.next_float() + 1.0) * 0.2
}

impl Body<'_> {
    /// `EntityChicken.onLivingUpdate` after its `super` call.
    pub(super) fn flap(&mut self, wings: &mut Wings, fx: &mut Effects) {
        let on_ground = self.collision.on_ground;
        wings.prev_rotation = wings.rotation;
        wings.prev_flap_speed = wings.flap_speed;
        let lift = if on_ground { -1.0 } else { 4.0 };
        wings.flap_speed = (wings.flap_speed + lift * 0.3).clamp(0.0, 1.0);
        if !on_ground && wings.rotation_delta < 1.0 {
            wings.rotation_delta = 1.0;
        }
        wings.rotation_delta *= 0.9;
        if !on_ground && self.motion.y < 0.0 {
            self.motion.y *= 0.6;
        }
        wings.rotation += wings.rotation_delta * 2.0;
        self.mob.egg_timer = self.mob.egg_timer.saturating_sub(1);
        if self.mob.egg_timer == 0 {
            if let Ok(egg) = ItemStack::new(ItemId::Egg, 1) {
                spawn_entity_drop(fx.commands, fx.loot, self.feet, egg);
            }
            self.mob.egg_timer = self.mob.rng.next_int(6000) as u16 + 6000;
        }
    }

    /// `EntitySquid.updatePlayerActionState`: pick a new swim direction now
    /// and then, or whenever it leaves the water.
    pub(super) fn squid_action(&mut self, world: &Surroundings, swim: Option<&mut Swim>) {
        if let Some(swim) = swim {
            let rng = &mut self.mob.rng;
            if rng.next_int(50) == 0 || !self.living.in_water || swim.heading == Vec3::ZERO {
                let angle = rng.next_float() * TAU;
                swim.heading = Vec3::new(
                    angle.cos() * 0.2,
                    -0.1 + rng.next_float() * 0.2,
                    angle.sin() * 0.2,
                );
            }
        }
        self.despawn(world);
    }

    /// `EntitySquid.isInWater`, which also pushes the squid along the
    /// current. Its box shrinks by 0.6 from above and below, so for part of
    /// each block the scan is empty and a squid near the surface sinks back.
    pub(super) fn squid_in_water(&mut self, world: &Surroundings) -> bool {
        let (in_water, current) =
            water_movement(grow(self.aabb(), Vec3::new(0.0, -0.6, 0.0)), world.chunks);
        self.motion += current * WATER_CURRENT_PER_TICK;
        in_water
    }

    /// `EntitySquid.onLivingUpdate` after its `super` call.
    pub(super) fn swim(&mut self, world: &Surroundings, swim: &mut Swim) {
        swim.prev_pitch = swim.pitch;
        swim.prev_yaw = swim.yaw;
        swim.prev_tentacle = swim.tentacle;
        swim.rotation += swim.rotation_velocity;
        if swim.rotation > TAU {
            swim.rotation -= TAU;
            if self.mob.rng.next_int(10) == 0 {
                swim.rotation_velocity = stroke_velocity(&mut self.mob.rng);
            }
        }
        if self.squid_in_water(world) {
            if swim.rotation < PI {
                let stroke = swim.rotation / PI;
                swim.tentacle = (stroke * stroke * PI).sin() * PI * 0.25;
                if stroke > 0.75 {
                    swim.motion_speed = 1.0;
                    swim.rotate_speed = 1.0;
                } else {
                    swim.rotate_speed *= 0.8;
                }
            } else {
                swim.tentacle = 0.0;
                swim.motion_speed *= 0.9;
                swim.rotate_speed *= 0.99;
            }
            self.motion = swim.heading * swim.motion_speed;
            let horizontal = (self.motion.x * self.motion.x + self.motion.z * self.motion.z).sqrt();
            let living = &mut *self.living;
            living.body_yaw +=
                (-self.motion.x.atan2(self.motion.z).to_degrees() - living.body_yaw) * 0.1;
            living.yaw = living.body_yaw;
            swim.yaw += PI * swim.rotate_speed * 1.5;
            swim.pitch += (-horizontal.atan2(self.motion.y).to_degrees() - swim.pitch) * 0.1;
        } else {
            swim.tentacle = swim.rotation.sin().abs() * PI * 0.25;
            self.motion.x = 0.0;
            self.motion.y = (self.motion.y - 0.08) * 0.98;
            self.motion.z = 0.0;
            swim.pitch += (-90.0 - swim.pitch) * 0.02;
        }
    }

    /// `EntityWolf.updatePlayerActionState` after its `super` call. Hunting
    /// sheep is not simulated: mobs only target the player.
    pub(super) fn wolf_action(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
        has_attacked: bool,
    ) {
        if !has_attacked
            && self.living.path.is_none()
            && self.mob.tamed
            && let Some(owner) = world.player
        {
            let distance = owner.eye.distance(self.feet);
            if distance > 5.0 {
                self.follow_owner(world, pathfinder, owner, distance);
            }
        }
        if self.living.in_water {
            self.mob.sitting = false;
        }
    }

    /// `EntityWolf.getPathOrWalkableBlock`: path to the owner, or jump to a
    /// free cell around them when they are far away and out of reach.
    fn follow_owner(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
        owner: Target,
        distance: f32,
    ) {
        let path = pathfinder.path_to_feet(world.chunks, self.feet, self.size, owner.feet, 16.0);
        if path.is_some() || distance <= 12.0 {
            self.living.path = path;
            return;
        }
        let normal_cube = |x, y, z| world.chunks.block_at(x, y, z).is_some_and(is_opaque_cube);
        let x = owner.eye.x.floor() as i32 - 2;
        let z = owner.eye.z.floor() as i32 - 2;
        let y = owner.feet.y.floor() as i32;
        for i in 0..=4 {
            for j in 0..=4 {
                if (i < 1 || j < 1 || i > 3 || j > 3)
                    && normal_cube(x + i, y - 1, z + j)
                    && !normal_cube(x + i, y, z + j)
                    && !normal_cube(x + i, y + 1, z + j)
                {
                    self.feet = Vec3::new((x + i) as f32 + 0.5, y as f32, (z + j) as f32 + 0.5);
                    self.previous = self.feet;
                    self.teleported = true;
                    return;
                }
            }
        }
    }

    /// `EntityWolf.attackEntity`: leap from a few blocks out, bite up close.
    /// A tamed wolf bites twice as hard.
    pub(super) fn wolf_bite(
        &mut self,
        world: &Surroundings,
        player: &Target,
        distance: f32,
        fx: &mut Effects,
    ) {
        if distance > 2.0 && distance < 6.0 && self.mob.rng.next_int(10) == 0 {
            self.leap_at(player);
        } else if distance < 1.5 && self.overlaps_vertically(player) {
            self.living.attack_time = 20;
            let amount = if self.mob.tamed { 4 } else { 2 };
            let hit = Hit {
                amount,
                from: Some(self.feet),
                source: Source::Creature,
            };
            self.strike(world, hit, fx);
        }
    }

    /// The leap `EntityWolf` and `EntitySpider` make from the ground.
    pub(super) fn leap_at(&mut self, player: &Target) {
        if !self.collision.on_ground {
            return;
        }
        let dx = player.eye.x - self.feet.x;
        let dz = player.eye.z - self.feet.z;
        let length = (dx * dx + dz * dz).sqrt();
        self.motion.x = dx / length * 0.5 * 0.8 + self.motion.x * 0.2;
        self.motion.z = dz / length * 0.5 * 0.8 + self.motion.z * 0.2;
        self.motion.y = 0.4;
    }

    /// The player's box overlaps this body's height.
    pub(super) fn overlaps_vertically(&self, player: &Target) -> bool {
        let aabb = self.aabb();
        let theirs = player.aabb();
        theirs.max.y > aabb.min.y && theirs.min.y < aabb.max.y
    }

    /// `player.attackEntityFrom(this, amount)`.
    pub(super) fn strike(&self, world: &Surroundings, hit: Hit, fx: &mut Effects) -> bool {
        match fx.victim.as_mut() {
            Some(victim) => hurt_player(victim, hit, world.difficulty, fx.loot),
            None => false,
        }
    }

    /// `EntityWolf.findPlayerToAttack`.
    pub(super) fn wolf_wants(&self, world: &Surroundings) -> bool {
        self.mob.angry
            && world
                .player
                .is_some_and(|player| player.eye.distance_squared(self.feet) < 256.0)
    }
}

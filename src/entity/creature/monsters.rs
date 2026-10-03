//! Monsters: `EntityMob` and its zombie, skeleton, creeper, spider, and
//! zombie pigman, plus `EntitySlime` and `EntityGhast`, which are monsters
//! without being `EntityMob`s. Also the per-kind hooks the shared tick calls.

use bevy::prelude::*;

use super::Body;
use super::Effects;
use super::Surroundings;
use super::Target;
use super::Traits;
use super::grow;
use crate::entity::combat::Hit;
use crate::entity::combat::Source;
use crate::entity::mobs::Explosion;
use crate::entity::mobs::MobKind;
use crate::entity::projectiles::spawn_arrow;
use crate::entity::projectiles::spawn_fireball;
use crate::physics::colliding_aabbs;
use crate::random::JavaRandom;

/// `EntityCreeper`'s fuse. `Mob::fuse` holds `timeSinceIgnited`.
#[derive(Component, Clone, Debug, Default)]
pub struct Fuse {
    /// `lastActiveTime`: the fuse at the start of this tick.
    pub prev: i16,
    /// `getCreeperState`: 1 while hissing, -1 while cooling off.
    state: i8,
}

impl Fuse {
    /// `EntityCreeper.setCreeperFlashTime`: how near the blast, from 0 to
    /// just past 1. It drives the swell and the white flashes.
    pub fn flash(&self, fuse: i16, partial: f32) -> f32 {
        (f32::from(self.prev) + f32::from(fuse - self.prev) * partial) / 28.0
    }
}

/// `EntitySlime`'s hop.
#[derive(Component, Clone, Debug)]
pub struct Bounce {
    /// `slimeJumpDelay`.
    jump_delay: i32,
    /// `squishAmount` and `squishFactor`: positive stretches tall on a jump,
    /// negative flattens on landing.
    pub squish: f32,
    pub prev_squish: f32,
}

impl Bounce {
    pub fn new(rng: &mut JavaRandom) -> Self {
        Self {
            jump_delay: rng.next_int(20) as i32 + 10,
            squish: 0.0,
            prev_squish: 0.0,
        }
    }
}

/// `EntityGhast`'s drifting and firing.
#[derive(Component, Clone, Debug, Default)]
pub struct Hover {
    waypoint: Vec3,
    course_change: i32,
    aggro_cooldown: i32,
    /// `targetedEntity` is the player.
    targeting: bool,
    /// `attackCounter`: counts up to 20 while the ghast can see its target,
    /// then fires and drops to -40.
    pub attack_counter: i16,
    pub prev_attack_counter: i16,
}

impl Body<'_> {
    /// The start of each kind's `onLivingUpdate`: `EntityMob`s age faster in
    /// light, so they despawn sooner there, and zombies and skeletons catch
    /// fire in daylight. A zombie pigman inherits the check but cannot burn.
    pub(super) fn living_prelude(&mut self, world: &Surroundings) {
        if !self.mob.kind.is_monster() {
            return;
        }
        let brightness = self.brightness(world);
        if matches!(
            self.mob.kind,
            MobKind::Zombie | MobKind::Skeleton | MobKind::PigZombie
        ) && world.daytime()
            && brightness > 0.5
        {
            let cell = self.feet.floor().as_ivec3();
            if world.can_see_sky(cell.x, cell.y, cell.z)
                && self.mob.rng.next_float() * 30.0 < (brightness - 0.4) * 2.0
            {
                self.mob.fire_ticks = 300;
            }
        }
        if brightness > 0.5 {
            self.living.entity_age += 2;
        }
    }

    /// `findPlayerToAttack`.
    pub(super) fn find_player_to_attack(&self, world: &Surroundings) -> bool {
        let Some(player) = world.player else {
            return false;
        };
        let within = |range: f32| player.eye.distance_squared(self.feet) < range * range;
        match self.mob.kind {
            // `EntityMob`: the nearest player within 16 it can see.
            MobKind::Zombie | MobKind::Skeleton | MobKind::Creeper => {
                within(16.0) && self.can_see(world, &player)
            }
            MobKind::PigZombie => self.mob.angry && within(16.0) && self.can_see(world, &player),
            // Spiders hunt only in the dark, but need no line of sight.
            MobKind::Spider => self.brightness(world) < 0.5 && within(16.0),
            MobKind::Wolf => self.wolf_wants(world),
            _ => false,
        }
    }

    /// `attackEntity`, called while the target is in sight.
    pub(super) fn attack_player(
        &mut self,
        world: &Surroundings,
        player: &Target,
        distance: f32,
        traits: &mut Traits,
        fx: &mut Effects,
    ) {
        match self.mob.kind {
            MobKind::Zombie | MobKind::PigZombie => self.melee(world, player, distance, 5, fx),
            MobKind::Spider => {
                if self.brightness(world) > 0.5 && self.mob.rng.next_int(100) == 0 {
                    self.living.chasing = false;
                } else if distance > 2.0 && distance < 6.0 && self.mob.rng.next_int(10) == 0 {
                    self.leap_at(player);
                } else {
                    self.melee(world, player, distance, 2, fx);
                }
            }
            MobKind::Skeleton => self.shoot(player, distance, fx),
            MobKind::Creeper => {
                if let Some(fuse) = traits.fuse.as_deref_mut() {
                    self.hiss(fuse, distance, fx);
                }
            }
            MobKind::Wolf => self.wolf_bite(world, player, distance, fx),
            _ => {}
        }
    }

    /// `attackBlockedEntity`: a creeper that loses sight of its target cools.
    pub(super) fn attack_blocked_player(&mut self, _distance: f32, traits: &mut Traits) {
        if let Some(fuse) = traits.fuse.as_deref_mut() {
            self.cool(fuse);
        }
    }

    /// `EntityMob.attackEntity`: strike at arm's length once the cooldown ends.
    fn melee(
        &mut self,
        world: &Surroundings,
        player: &Target,
        distance: f32,
        strength: i16,
        fx: &mut Effects,
    ) {
        if self.living.attack_time <= 0 && distance < 2.0 && self.overlaps_vertically(player) {
            self.living.attack_time = 20;
            let hit = Hit {
                amount: strength,
                from: Some(self.feet),
                source: Source::Monster,
            };
            self.strike(world, hit, fx);
        }
    }

    /// `EntitySkeleton.attackEntity`: stand within ten blocks, face the
    /// target, and loose an arrow every 30 ticks, aimed high for the drop.
    fn shoot(&mut self, player: &Target, distance: f32, fx: &mut Effects) {
        if distance >= 10.0 {
            return;
        }
        let dx = player.eye.x - self.feet.x;
        let dz = player.eye.z - self.feet.z;
        if self.living.attack_time == 0 {
            // `new EntityArrow(world, this)` starts at the eyes, nudged
            // sideways, and the skeleton then raises it a further block.
            let (sin, cos) = self.living.yaw.to_radians().sin_cos();
            let start = Vec3::new(
                self.feet.x - cos * 0.16,
                self.feet.y + self.eye_height() - 0.1 + 1.0,
                self.feet.z - sin * 0.16,
            );
            let dy = player.eye.y + super::PLAYER_EYE_HEIGHT - 0.2 - start.y;
            let lift = (dx * dx + dz * dz).sqrt() * 0.2;
            spawn_arrow(
                fx.commands,
                start,
                Vec3::new(dx, dy + lift, dz),
                0.6,
                12.0,
                Some(self.entity),
                &mut self.mob.rng,
            );
            self.living.attack_time = 30;
        }
        self.living.yaw = dz.atan2(dx).to_degrees() - 90.0;
        self.living.has_attacked = true;
    }

    /// `EntityCreeper.attackEntity`: hiss within three blocks, keep hissing
    /// out to seven, and explode after 30 ticks.
    fn hiss(&mut self, fuse: &mut Fuse, distance: f32, fx: &mut Effects) {
        if fuse.state <= 0 && distance < 3.0 || fuse.state > 0 && distance < 7.0 {
            fuse.state = 1;
            self.mob.fuse += 1;
            if self.mob.fuse >= 30 {
                fx.explosions.push(Explosion {
                    center: self.feet,
                    strength: if self.mob.charged { 6.0 } else { 3.0 },
                    flaming: false,
                    source: Source::Monster,
                });
                self.dead = true;
            }
            self.living.has_attacked = true;
        } else {
            self.cool(fuse);
        }
    }

    fn cool(&mut self, fuse: &mut Fuse) {
        fuse.state = -1;
        self.mob.fuse = (self.mob.fuse - 1).max(0);
    }

    /// `EntitySlime.updatePlayerActionState`: hop now and then, three times as
    /// often toward a player within 16.
    pub(super) fn slime_action(&mut self, world: &Surroundings, bounce: Option<&mut Bounce>) {
        self.despawn(world);
        let near = world
            .player
            .filter(|player| player.eye.distance_squared(self.feet) < 256.0);
        if let Some(player) = near {
            self.face_player(player, 10.0, 20.0);
        }
        let Some(bounce) = bounce else {
            return;
        };
        let ready = self.collision.on_ground && {
            let ready = bounce.jump_delay <= 0;
            bounce.jump_delay -= 1;
            ready
        };
        if ready {
            bounce.jump_delay = self.mob.rng.next_int(20) as i32 + 10;
            if near.is_some() {
                bounce.jump_delay /= 3;
            }
            self.living.jumping = true;
            bounce.squish = 1.0;
            self.living.move_strafing = 1.0 - self.mob.rng.next_float() * 2.0;
            self.living.move_forward = f32::from(self.mob.variant.max(1));
        } else {
            self.living.jumping = false;
            if self.collision.on_ground {
                self.living.move_strafing = 0.0;
                self.living.move_forward = 0.0;
            }
        }
    }

    /// `EntitySlime.onCollideWithPlayer`, which Beta runs from the player's
    /// update: a slime bigger than the smallest hurts by its size on contact.
    fn slime_touch(&mut self, world: &Surroundings, fx: &mut Effects) {
        let size = self.mob.variant.max(1);
        let Some(player) = world.player else {
            return;
        };
        if size <= 1
            || !player.alive
            || !grow(player.aabb(), Vec3::new(1.0, 0.0, 1.0)).intersects(self.aabb())
            || player.eye.distance(self.feet) >= 0.6 * f32::from(size)
            || !self.can_see(world, &player)
        {
            return;
        }
        let hit = Hit {
            amount: i16::from(size),
            from: Some(self.feet),
            source: Source::Creature,
        };
        self.strike(world, hit, fx);
    }

    /// `EntityGhast.updatePlayerActionState`: drift between random waypoints,
    /// and lob a fireball at a visible player within 64 blocks.
    pub(super) fn ghast_action(
        &mut self,
        world: &Surroundings,
        hover: Option<&mut Hover>,
        fx: &mut Effects,
    ) {
        self.despawn(world);
        let Some(hover) = hover else {
            return;
        };
        hover.prev_attack_counter = hover.attack_counter;
        let mut toward = hover.waypoint - self.feet;
        let mut distance = toward.length();
        if !(1.0..=60.0).contains(&distance) {
            let rng = &mut self.mob.rng;
            hover.waypoint = self.feet
                + Vec3::new(
                    (rng.next_float() * 2.0 - 1.0) * 16.0,
                    (rng.next_float() * 2.0 - 1.0) * 16.0,
                    (rng.next_float() * 2.0 - 1.0) * 16.0,
                );
            toward = hover.waypoint - self.feet;
            distance = toward.length();
        }
        let due = hover.course_change <= 0;
        hover.course_change -= 1;
        if due {
            hover.course_change += self.mob.rng.next_int(5) as i32 + 2;
            if self.course_clear(toward, distance, world) {
                self.motion += toward / distance * 0.1;
            } else {
                hover.waypoint = self.feet;
            }
        }
        let player = world.player.filter(|player| player.alive);
        if player.is_none() {
            hover.targeting = false;
        }
        let refresh = !hover.targeting || {
            let due = hover.aggro_cooldown <= 0;
            hover.aggro_cooldown -= 1;
            due
        };
        if refresh {
            hover.targeting =
                player.is_some_and(|player| player.eye.distance_squared(self.feet) < 100.0 * 100.0);
            if hover.targeting {
                hover.aggro_cooldown = 20;
            }
        }
        match player {
            Some(player)
                if hover.targeting && player.eye.distance_squared(self.feet) < 64.0 * 64.0 =>
            {
                let half = self.size.height / 2.0;
                let aim = Vec3::new(
                    player.eye.x - self.feet.x,
                    player.feet.y + 0.9 - (self.feet.y + half),
                    player.eye.z - self.feet.z,
                );
                self.living.yaw = -aim.x.atan2(aim.z).to_degrees();
                self.living.body_yaw = self.living.yaw;
                if self.can_see(world, &player) {
                    hover.attack_counter += 1;
                    if hover.attack_counter == 20 {
                        let (sin, cos) = self.living.yaw.to_radians().sin_cos();
                        let pitch = self.living.pitch.to_radians().cos();
                        let start = Vec3::new(
                            self.feet.x - sin * pitch * 4.0,
                            self.feet.y + half + 0.5,
                            self.feet.z + cos * pitch * 4.0,
                        );
                        spawn_fireball(
                            fx.commands,
                            start,
                            aim,
                            Some(self.entity),
                            &mut self.mob.rng,
                        );
                        hover.attack_counter = -40;
                    }
                } else if hover.attack_counter > 0 {
                    hover.attack_counter -= 1;
                }
            }
            _ => {
                self.living.yaw = -self.motion.x.atan2(self.motion.z).to_degrees();
                self.living.body_yaw = self.living.yaw;
                if hover.attack_counter > 0 {
                    hover.attack_counter -= 1;
                }
            }
        }
    }

    /// `EntityGhast.isCourseTraversable`: slide the body toward the waypoint
    /// a block at a time and stop at the first collision.
    fn course_clear(&self, toward: Vec3, distance: f32, world: &Surroundings) -> bool {
        let step = toward / distance;
        let mut aabb = self.aabb();
        let mut i = 1.0;
        while i < distance {
            aabb = aabb.offset(step);
            if !colliding_aabbs(world.chunks, aabb).is_empty() {
                return false;
            }
            i += 1.0;
        }
        true
    }

    /// Each kind's work after `EntityLiving.onLivingUpdate` moved it.
    pub(super) fn living_epilogue(
        &mut self,
        world: &Surroundings,
        traits: &mut Traits,
        fx: &mut Effects,
    ) {
        match self.mob.kind {
            MobKind::Chicken => {
                if let Some(wings) = traits.wings.as_deref_mut() {
                    self.flap(wings, fx);
                }
            }
            MobKind::Squid => {
                if let Some(swim) = traits.swim.as_deref_mut() {
                    self.swim(world, swim);
                }
            }
            MobKind::Slime => self.slime_touch(world, fx),
            _ => {}
        }
    }

    /// Each kind's work after the whole `onUpdate`.
    pub(super) fn update_epilogue(&mut self, was_on_ground: bool, traits: &mut Traits) {
        if let Some(fuse) = traits.fuse.as_deref_mut()
            && !self.living.chasing
            && self.mob.fuse > 0
        {
            self.cool(fuse);
        }
        if let Some(bounce) = traits.bounce.as_deref_mut() {
            if self.collision.on_ground && !was_on_ground {
                bounce.squish = -0.5;
            }
            bounce.squish *= 0.6;
        }
    }
}

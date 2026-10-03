//! Beta 1.7.3 creatures: pigs, cows, sheep, chickens, wolves, and squid.
//!
//! Each world tick transcribes `Entity.onEntityUpdate`, `EntityLiving.onLivingUpdate`,
//! and the tail of `EntityLiving.onUpdate`, in Beta's blocks-per-tick units:
//!
//! - `EntityCreature.updatePlayerActionState` walks a [`Path`] from the Beta
//!   [`Pathfinder`]. About one tick in eighty a creature picks the best of ten
//!   random nearby cells by `EntityAnimal.getBlockPathWeight` (grass above all,
//!   then light) and paths there. Between paths it falls back to
//!   `EntityLiving.updatePlayerActionState`: idle head turns, glancing at a
//!   player within eight blocks, and despawning when far from the player.
//! - `moveEntityWithHeading` accelerates along the body's yaw with Beta's
//!   ground friction, water and lava drag, ladders, and step-up.
//! - Chickens flap and fall slowly, and lay eggs. Squid swim with
//!   `EntitySquid`'s pulses. Tamed wolves sit and follow their owner.
//!
//! Hostile mobs still use the steering in [`super::mobs`]. Attacking is not
//! simulated here yet: an angry wolf paths after the player, but its bite and
//! wild wolves hunting sheep are left for combat.

use std::f32::consts::PI;
use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::block::properties::slipperiness;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobKind;
use crate::entity::mobs::burn;
use crate::entity::mobs::drop_item;
use crate::entity::mobs::kill_mob;
use crate::entity::mobs::update_fire;
use crate::entity::pathfinding::Path;
use crate::entity::pathfinding::Pathfinder;
use crate::item::ItemId;
use crate::physics::Aabb;
use crate::physics::WATER_CURRENT_PER_TICK;
use crate::physics::colliding_aabbs;
use crate::physics::intersects_liquid;
use crate::physics::lava_contains;
use crate::physics::move_entity;
use crate::physics::step_on_block;
use crate::physics::water_movement;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::JavaRandom;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::environment::skylight_subtracted;
use crate::world::lighting::LightCache;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::combined_light;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;
use crate::world::weather::WorldWeather;

/// `EntityLiving.jump`.
const JUMP_MOTION: f32 = 0.42;
/// `EntityPlayer.getEyeHeight`. A player's `posY` is already at its eyes.
const PLAYER_EYE_HEIGHT: f32 = 0.12;
/// `EntityLiving.moveSpeed`, which `EntityWolf` raises.
const MOVE_SPEED: f32 = 0.7;
const WOLF_MOVE_SPEED: f32 = 1.1;

/// `EntityLiving` facing, steering, and walk-cycle state. Angles are Beta's
/// degrees: yaw 0 faces +Z and 90 faces -X; positive pitch looks down.
///
/// Bodies with this component are moved by [`tick_creatures`], not by the
/// generic `integrate_bodies`.
#[derive(Component, Clone, Debug, Default)]
pub struct Living {
    /// `rotationYaw`: where the head faces.
    pub yaw: f32,
    pub prev_yaw: f32,
    /// `rotationPitch`.
    pub pitch: f32,
    pub prev_pitch: f32,
    /// `renderYawOffset`: where the body faces. It trails the direction of
    /// travel and never strays more than 75° from the head.
    pub body_yaw: f32,
    pub prev_body_yaw: f32,
    /// `legSwing`: the walk cycle's phase.
    pub limb_swing: f32,
    /// `legYaw`: how far the legs swing, from 0 standing to 1 at a trot.
    pub limb_amount: f32,
    pub prev_limb_amount: f32,
    move_forward: f32,
    move_strafing: f32,
    random_yaw_velocity: f32,
    jumping: bool,
    /// `Entity.inWater`, from this tick's `handleWaterMovement`.
    in_water: bool,
    /// `entityAge`: ticks spent idle, which gates despawning.
    entity_age: u32,
    /// `currentTarget` and `numTicksToChaseTarget`: the player being looked
    /// at, and for how many more ticks.
    looking: Option<i32>,
    /// `playerToAttack`. Only angry wolves set it.
    chasing: bool,
    path: Option<Path>,
}

impl Living {
    pub fn facing(yaw: f32) -> Self {
        Self {
            yaw,
            prev_yaw: yaw,
            body_yaw: yaw,
            prev_body_yaw: yaw,
            ..default()
        }
    }

    /// The path being walked, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref()
    }

    /// Whether the last tick found water around the body.
    pub fn in_water(&self) -> bool {
        self.in_water
    }

    /// Idle ticks counted toward despawning.
    pub fn entity_age(&self) -> u32 {
        self.entity_age
    }
}

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

/// How a creature's tick ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fate {
    Alive,
    Despawned,
    Killed,
}

/// The player, as creatures see it.
#[derive(Clone, Copy)]
struct Target {
    /// Beta's player `posY` sits at the eyes, so this is the player's `pos`.
    eye: Vec3,
    feet: Vec3,
    alive: bool,
}

/// World state shared by every creature this frame.
struct Surroundings<'a> {
    chunks: &'a WorldChunks,
    light: &'a LightCache,
    raining: bool,
    skylight_subtracted: u8,
    player: Option<Target>,
    /// Pushable bodies at the start of the frame, for `applyEntityCollision`.
    crowd: &'a [(Entity, Aabb)],
}

impl Surroundings<'_> {
    /// `World.getBlockLightValue`. Slabs, farmland, and stairs take the
    /// brightest of their upper and side neighbours.
    fn light_value(&self, x: i32, y: i32, z: i32) -> u8 {
        let raw = |x, y, z| {
            self.light.channels(x, y, z).map_or(0, |(sky, block)| {
                combined_light(sky, block, self.skylight_subtracted)
            })
        };
        if matches!(
            self.chunks.block_at(x, y, z),
            Some(Id::StoneSlab | Id::Farmland | Id::CobblestoneStairs | Id::WoodenStairs)
        ) {
            [(0, 1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)]
                .into_iter()
                .map(|(dx, dy, dz)| raw(x + dx, y + dy, z + dz))
                .max()
                .unwrap_or(0)
        } else {
            raw(x, y, z)
        }
    }

    /// `EntityAnimal.getBlockPathWeight`.
    fn path_weight(&self, x: i32, y: i32, z: i32) -> f32 {
        if self.chunks.block_at(x, y - 1, z) == Some(Id::Grass) {
            10.0
        } else {
            beta_brightness(self.light_value(x, y, z)) - 0.5
        }
    }
}

/// One creature's simulated state while its ticks run.
struct Body<'a> {
    entity: Entity,
    mob: &'a mut Mob,
    living: &'a mut Living,
    size: EntitySize,
    step_height: f32,
    feet: Vec3,
    /// `prevPosX/Y/Z`: the feet at the start of this tick.
    previous: Vec3,
    /// `motionX/Y/Z`, in blocks per tick.
    motion: Vec3,
    collision: CollisionState,
    /// `setEntityDead` was called this tick.
    dead: bool,
    /// `setLocationAndAngles` moved the body without travel.
    teleported: bool,
}

/// Run each creature's Beta update once per world tick this frame.
pub(crate) fn tick_creatures(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Res<LightCache>,
    weather: Option<Res<WorldWeather>>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
    player: Query<(&Transform, &EntitySize, Option<&PlayerHealth>), With<Player>>,
    others: Query<(Entity, &Transform, &EntitySize), (With<Mob>, Without<Living>, Without<Player>)>,
    mut creatures: Query<
        (
            Entity,
            &mut Mob,
            &mut Living,
            &mut Transform,
            &mut Velocity,
            &mut CollisionState,
            &EntitySize,
            &StepHeight,
            &mut PreviousTick,
            Option<&mut Wings>,
            Option<&mut Swim>,
            Option<&mut StepDistance>,
        ),
        Without<Player>,
    >,
    mut pathfinder: Local<Pathfinder>,
    mut crowd: Local<Vec<(Entity, Aabb)>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    let player = player
        .single()
        .ok()
        .map(|(transform, size, health)| Target {
            eye: transform.translation,
            feet: transform.translation - Vec3::Y * size.y_offset,
            alive: health.is_none_or(|health| health.current > 0),
        });
    crowd.clear();
    crowd.extend(
        creatures
            .iter()
            .map(|(entity, _, _, transform, _, _, size, ..)| {
                (entity, size.aabb(transform.translation))
            }),
    );
    crowd.extend(
        others
            .iter()
            .map(|(entity, transform, size)| (entity, size.aabb(transform.translation))),
    );
    if let Some(player) = player {
        crowd.push((Entity::PLACEHOLDER, EntitySize::PLAYER.aabb(player.eye)));
    }
    let penalty = weather
        .as_ref()
        .map_or(0, |weather| weather.skylight_penalty());
    let world = Surroundings {
        chunks: &chunks,
        light: &light,
        raining: weather.as_ref().is_some_and(|weather| weather.is_raining()),
        skylight_subtracted: skylight_subtracted(celestial_angle(tick.world_time(), 0.0))
            .saturating_add(penalty)
            .min(15),
        player,
        crowd: crowd.as_slice(),
    };

    for (
        entity,
        mut mob,
        mut living,
        mut transform,
        mut velocity,
        mut collision,
        size,
        step_height,
        mut previous,
        mut wings,
        mut swim,
        mut steps,
    ) in &mut creatures
    {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            commands.entity(entity).despawn();
            continue;
        }
        let mut body = Body {
            entity,
            mob: &mut mob,
            living: &mut living,
            size: *size,
            step_height: step_height.0,
            feet: transform.translation,
            previous: transform.translation,
            motion: velocity.0 * TICK_SECONDS,
            collision: *collision,
            dead: false,
            teleported: false,
        };
        let mut fate = Fate::Alive;
        for step in 0..ticks {
            let now = tick
                .world_time()
                .saturating_sub(u64::from(ticks - step - 1));
            previous.0 = body.feet;
            fate = body.tick(
                &world,
                &mut pathfinder,
                now,
                wings.as_deref_mut(),
                swim.as_deref_mut(),
                steps.as_deref_mut(),
                block_ticks.as_deref_mut(),
                &mut commands,
            );
            if fate != Fate::Alive {
                break;
            }
        }
        if body.teleported {
            previous.0 = body.feet;
        }
        transform.translation = body.feet;
        velocity.0 = body.motion / TICK_SECONDS;
        *collision = body.collision;
        match fate {
            Fate::Alive => {}
            Fate::Despawned => commands.entity(entity).despawn(),
            Fate::Killed => kill_mob(&mut commands, entity, &mut mob, transform.translation),
        }
    }
}

impl Body<'_> {
    fn aabb(&self) -> Aabb {
        self.size.aabb(self.feet)
    }

    fn is(&self, kind: MobKind) -> bool {
        self.mob.kind == kind
    }

    /// `getEyeHeight`, which `EntityWolf` lowers.
    fn eye_height(&self) -> f32 {
        self.size.height * if self.is(MobKind::Wolf) { 0.8 } else { 0.85 }
    }

    #[allow(clippy::too_many_arguments)]
    fn tick(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
        now: u64,
        wings: Option<&mut Wings>,
        swim: Option<&mut Swim>,
        steps: Option<&mut StepDistance>,
        block_ticks: Option<&mut BlockTicks>,
        commands: &mut Commands,
    ) -> Fate {
        self.previous = self.feet;
        let living = &mut *self.living;
        living.prev_yaw = living.yaw;
        living.prev_pitch = living.pitch;
        living.prev_body_yaw = living.body_yaw;
        self.mob.age = self.mob.age.saturating_add(1);

        // `Entity.onEntityUpdate`: the water current pushes, and puts out fire.
        let (in_water, current) = water_movement(
            grow(self.aabb(), Vec3::new(-0.001, -0.401, -0.001)),
            world.chunks,
        );
        self.living.in_water = in_water;
        self.motion += current * WATER_CURRENT_PER_TICK;
        if in_water {
            self.mob.fire_ticks = 0;
        }
        update_fire(self.mob, world.chunks, self.feet, world.raining);
        burn(self.mob, now);
        if self.mob.health <= 0 {
            return Fate::Killed;
        }

        // `EntityLiving.onLivingUpdate`.
        let mut swim = swim;
        if let Some(swim) = swim.as_deref_mut() {
            self.squid_action(world, swim);
        } else {
            let has_attacked = self.creature_action(world, pathfinder);
            if self.is(MobKind::Wolf) {
                self.wolf_action(world, pathfinder, has_attacked);
            }
        }
        let in_lava = lava_contains(self.aabb(), world.chunks);
        let in_water = if swim.is_some() {
            self.squid_in_water(world)
        } else {
            self.living.in_water
        };
        if self.living.jumping {
            if in_water || in_lava {
                self.motion.y += 0.04;
            } else if self.collision.on_ground {
                self.motion.y = JUMP_MOTION;
            }
        }
        self.living.move_strafing *= 0.98;
        self.living.move_forward *= 0.98;
        self.living.random_yaw_velocity *= 0.9;
        if swim.is_some() {
            // `EntitySquid.moveEntityWithHeading` only applies its motion.
            self.move_entity(world.chunks, steps, block_ticks);
        } else {
            self.move_with_heading(world.chunks, in_lava, steps, block_ticks);
        }
        self.push_apart(world);
        if let Some(wings) = wings {
            self.flap(wings, commands);
        }
        if let Some(swim) = swim {
            self.swim(world, swim);
        }

        self.update_body_yaw();
        if self.dead {
            Fate::Despawned
        } else {
            Fate::Alive
        }
    }

    /// `EntityCreature.updatePlayerActionState`. Returns `hasAttacked`, which
    /// is really "may not wander": a sitting wolf.
    fn creature_action(&mut self, world: &Surroundings, pathfinder: &mut Pathfinder) -> bool {
        let has_attacked = self.is(MobKind::Wolf) && self.mob.sitting;
        if self.mob.sitting {
            // `EntityWolf.interact` drops the path when told to sit.
            self.living.path = None;
        }
        if !self.living.chasing {
            if self.find_player_to_attack(world) {
                self.living.chasing = true;
                self.living.path = self.path_to_player(world, pathfinder);
            }
        } else if !world.player.is_some_and(|player| player.alive) {
            self.living.chasing = false;
        }
        // `attackEntity` and `attackBlockedEntity` arrive with combat.

        let rng = &mut self.mob.rng;
        if has_attacked
            || !self.living.chasing
            || self.living.path.is_some() && rng.next_int(20) != 0
        {
            if !has_attacked
                && (self.living.path.is_none() && rng.next_int(80) == 0 || rng.next_int(80) == 0)
            {
                self.update_wander_path(world, pathfinder);
            }
        } else {
            self.living.path = self.path_to_player(world, pathfinder);
        }

        let in_water = self.living.in_water;
        let in_lava = lava_contains(self.aabb(), world.chunks);
        self.living.pitch = 0.0;
        if self.living.path.is_some() && self.mob.rng.next_int(100) != 0 {
            self.follow_path(world, has_attacked, in_water, in_lava);
        } else {
            self.living_action(world, in_water, in_lava);
            self.living.path = None;
        }
        has_attacked
    }

    /// The path-walking half of `EntityCreature.updatePlayerActionState`.
    fn follow_path(
        &mut self,
        world: &Surroundings,
        has_attacked: bool,
        in_water: bool,
        in_lava: bool,
    ) {
        let width = self.size.width;
        let reach = width * 2.0;
        let feet = self.feet;
        let mut target = None;
        if let Some(path) = self.living.path.as_mut() {
            target = Some(path.position(width));
            while let Some(point) = target
                && (point.x - feet.x).powi(2) + (point.z - feet.z).powi(2) < reach * reach
            {
                path.advance();
                target = (!path.is_finished()).then(|| path.position(width));
            }
        }
        if target.is_none() {
            self.living.path = None;
        }
        self.living.jumping = false;
        if let Some(point) = target {
            let foot_y = (self.aabb().min.y + 0.5).floor();
            let heading = (point.z - feet.z).atan2(point.x - feet.x).to_degrees() - 90.0;
            let turn = wrap_degrees(heading - self.living.yaw).clamp(-30.0, 30.0);
            self.living.move_forward = self.move_speed();
            self.living.yaw += turn;
            if has_attacked
                && self.living.chasing
                && let Some(player) = world.player
            {
                let previous = self.living.yaw;
                self.living.yaw = (player.eye.z - feet.z)
                    .atan2(player.eye.x - feet.x)
                    .to_degrees()
                    - 90.0;
                let angle = (previous - self.living.yaw + 90.0).to_radians();
                let forward = self.living.move_forward;
                self.living.move_strafing = -angle.sin() * forward;
                self.living.move_forward = angle.cos() * forward;
            }
            if point.y - foot_y > 0.0 {
                self.living.jumping = true;
            }
        }
        if self.living.chasing
            && let Some(player) = world.player
        {
            self.face_player(player, 30.0, 30.0);
        }
        if (self.collision.collided_x || self.collision.collided_z) && self.living.path.is_none() {
            self.living.jumping = true;
        }
        if self.mob.rng.next_float() < 0.8 && (in_water || in_lava) {
            self.living.jumping = true;
        }
    }

    /// `EntityLiving.updatePlayerActionState`: stand, turn idly, and glance
    /// at a nearby player.
    fn living_action(&mut self, world: &Surroundings, in_water: bool, in_lava: bool) {
        self.living.entity_age += 1;
        self.despawn(world);
        self.living.move_strafing = 0.0;
        self.living.move_forward = 0.0;
        let feet = self.feet;
        let near = |player: &Target| player.eye.distance_squared(feet) < 64.0;
        if self.mob.rng.next_float() < 0.02 {
            if world.player.as_ref().is_some_and(near) {
                self.living.looking = Some(10 + self.mob.rng.next_int(20) as i32);
            } else {
                self.living.random_yaw_velocity = (self.mob.rng.next_float() - 0.5) * 20.0;
            }
        }
        if let Some(ticks) = self.living.looking
            && let Some(player) = world.player
        {
            self.face_player(player, 10.0, self.vertical_face_speed());
            let expired = ticks <= 0 || !player.alive || !near(&player);
            self.living.looking = (!expired).then_some(ticks - 1);
        } else {
            self.living.looking = None;
            if self.mob.rng.next_float() < 0.05 {
                self.living.random_yaw_velocity = (self.mob.rng.next_float() - 0.5) * 20.0;
            }
            self.living.yaw += self.living.random_yaw_velocity;
            self.living.pitch = 0.0;
        }
        if in_water || in_lava {
            self.living.jumping = self.mob.rng.next_float() < 0.8;
        }
    }

    /// `EntityCreature.updateWanderPath`.
    fn update_wander_path(&mut self, world: &Surroundings, pathfinder: &mut Pathfinder) {
        let mut best = None;
        let mut best_weight = -99999.0;
        for _ in 0..10 {
            let rng = &mut self.mob.rng;
            let x = (self.feet.x + rng.next_int(13) as f32 - 6.0).floor() as i32;
            let y = (self.feet.y + rng.next_int(7) as f32 - 3.0).floor() as i32;
            let z = (self.feet.z + rng.next_int(13) as f32 - 6.0).floor() as i32;
            let weight = world.path_weight(x, y, z);
            if weight > best_weight {
                best_weight = weight;
                best = Some(IVec3::new(x, y, z));
            }
        }
        if let Some(target) = best {
            self.living.path =
                pathfinder.path_to_block(world.chunks, self.feet, self.size, target, 10.0);
        }
    }

    /// `EntityLiving.despawnEntity`. Beta's animals despawn like monsters,
    /// except a tamed wolf.
    fn despawn(&mut self, world: &Surroundings) {
        let Some(player) = world.player else {
            return;
        };
        if self.is(MobKind::Wolf) && self.mob.tamed {
            return;
        }
        let distance = player.eye.distance_squared(self.feet);
        if distance > 16384.0 {
            self.dead = true;
        }
        if self.living.entity_age > 600 && self.mob.rng.next_int(800) == 0 {
            if distance < 1024.0 {
                self.living.entity_age = 0;
            } else {
                self.dead = true;
            }
        }
    }

    /// `EntityWolf.findPlayerToAttack`.
    fn find_player_to_attack(&self, world: &Surroundings) -> bool {
        self.is(MobKind::Wolf)
            && self.mob.angry
            && world
                .player
                .is_some_and(|player| player.eye.distance_squared(self.feet) < 256.0)
    }

    /// `World.getPathToEntity(this, playerToAttack, 16)`.
    fn path_to_player(&self, world: &Surroundings, pathfinder: &mut Pathfinder) -> Option<Path> {
        let player = world.player?;
        pathfinder.path_to_feet(world.chunks, self.feet, self.size, player.feet, 16.0)
    }

    fn move_speed(&self) -> f32 {
        if self.is(MobKind::Wolf) {
            WOLF_MOVE_SPEED
        } else {
            MOVE_SPEED
        }
    }

    /// `getVerticalFaceSpeed`, which a sitting wolf halves.
    fn vertical_face_speed(&self) -> f32 {
        if self.is(MobKind::Wolf) && self.mob.sitting {
            20.0
        } else {
            40.0
        }
    }

    /// `EntityLiving.faceEntity`, with the player as the target.
    fn face_player(&mut self, player: Target, max_yaw: f32, max_pitch: f32) {
        let dx = player.eye.x - self.feet.x;
        let dz = player.eye.z - self.feet.z;
        let dy = self.feet.y + self.eye_height() - (player.eye.y + PLAYER_EYE_HEIGHT);
        let horizontal = (dx * dx + dz * dz).sqrt();
        let yaw = dz.atan2(dx).to_degrees() - 90.0;
        let pitch = -dy.atan2(horizontal).to_degrees();
        // Beta negates the result, so the pitch settles at the opposite sign.
        self.living.pitch = -update_rotation(self.living.pitch, pitch, max_pitch);
        self.living.yaw = update_rotation(self.living.yaw, yaw, max_yaw);
    }

    /// `EntityWolf.updatePlayerActionState` after its `super` call. Hunting
    /// sheep waits for combat.
    fn wolf_action(
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

    /// `EntitySquid.updatePlayerActionState`: pick a new swim direction now
    /// and then, or whenever it leaves the water.
    fn squid_action(&mut self, world: &Surroundings, swim: &mut Swim) {
        let rng = &mut self.mob.rng;
        if rng.next_int(50) == 0 || !self.living.in_water || swim.heading == Vec3::ZERO {
            let angle = rng.next_float() * TAU;
            swim.heading = Vec3::new(
                angle.cos() * 0.2,
                -0.1 + rng.next_float() * 0.2,
                angle.sin() * 0.2,
            );
        }
        self.despawn(world);
    }

    /// `EntitySquid.isInWater`, which also pushes the squid along the
    /// current. Its box shrinks by 0.6 from above and below, so for part of
    /// each block the scan is empty and a squid near the surface sinks back.
    fn squid_in_water(&mut self, world: &Surroundings) -> bool {
        let (in_water, current) =
            water_movement(grow(self.aabb(), Vec3::new(0.0, -0.6, 0.0)), world.chunks);
        self.motion += current * WATER_CURRENT_PER_TICK;
        in_water
    }

    /// `EntityLiving.moveEntityWithHeading`.
    fn move_with_heading(
        &mut self,
        chunks: &WorldChunks,
        in_lava: bool,
        steps: Option<&mut StepDistance>,
        block_ticks: Option<&mut BlockTicks>,
    ) {
        let strafe = self.living.move_strafing;
        let forward = self.living.move_forward;
        if self.living.in_water || in_lava {
            let start_y = self.feet.y;
            self.move_flying(strafe, forward, 0.02);
            self.move_entity(chunks, steps, block_ticks);
            self.motion *= if self.living.in_water { 0.8 } else { 0.5 };
            self.motion.y -= 0.02;
            if (self.collision.collided_x || self.collision.collided_z)
                && self.offset_position_clear(
                    chunks,
                    Vec3::new(
                        self.motion.x,
                        self.motion.y + 0.6 - self.feet.y + start_y,
                        self.motion.z,
                    ),
                )
            {
                self.motion.y = 0.3;
            }
        } else {
            let friction = self.ground_friction(chunks);
            let acceleration = if self.collision.on_ground {
                0.1 * (0.162_771_36 / (friction * friction * friction))
            } else {
                0.02
            };
            self.move_flying(strafe, forward, acceleration);
            let friction = self.ground_friction(chunks);
            let on_ladder = self.on_ladder(chunks);
            if on_ladder {
                self.motion.x = self.motion.x.clamp(-0.15, 0.15);
                self.motion.z = self.motion.z.clamp(-0.15, 0.15);
                self.motion.y = self.motion.y.max(-0.15);
            }
            self.move_entity(chunks, steps, block_ticks);
            if (self.collision.collided_x || self.collision.collided_z) && self.on_ladder(chunks) {
                self.motion.y = 0.2;
            }
            self.motion.y = (self.motion.y - 0.08) * 0.98;
            self.motion.x *= friction;
            self.motion.z *= friction;
        }

        let living = &mut *self.living;
        living.prev_limb_amount = living.limb_amount;
        let travelled = ((self.feet.x - self.previous.x).powi(2)
            + (self.feet.z - self.previous.z).powi(2))
        .sqrt();
        let amount = (travelled * 4.0).min(1.0);
        living.limb_amount += (amount - living.limb_amount) * 0.4;
        living.limb_swing += living.limb_amount;
    }

    /// Slipperiness of the block underfoot times air drag, or air drag alone.
    fn ground_friction(&self, chunks: &WorldChunks) -> f32 {
        if !self.collision.on_ground {
            return 0.91;
        }
        match chunks.block_at(
            self.feet.x.floor() as i32,
            self.aabb().min.y.floor() as i32 - 1,
            self.feet.z.floor() as i32,
        ) {
            Some(block) if block != Id::Air => slipperiness(block) * 0.91,
            _ => 0.546,
        }
    }

    /// `EntityLiving.isOnLadder`.
    fn on_ladder(&self, chunks: &WorldChunks) -> bool {
        chunks
            .block_at(
                self.feet.x.floor() as i32,
                self.aabb().min.y.floor() as i32,
                self.feet.z.floor() as i32,
            )
            .is_some_and(|block| block.is_ladder())
    }

    /// `Entity.isOffsetPositionInLiquid`, which despite its name is true when
    /// the moved box would touch neither a solid block nor any liquid.
    fn offset_position_clear(&self, chunks: &WorldChunks, offset: Vec3) -> bool {
        let moved = self.aabb().offset(offset);
        colliding_aabbs(chunks, moved).is_empty() && !intersects_liquid(moved, chunks)
    }

    /// `Entity.moveFlying`: accelerate toward the head's yaw.
    fn move_flying(&mut self, strafe: f32, forward: f32, acceleration: f32) {
        let magnitude = (strafe * strafe + forward * forward).sqrt();
        if magnitude < 0.01 {
            return;
        }
        let scale = acceleration / magnitude.max(1.0);
        let (strafe, forward) = (strafe * scale, forward * scale);
        let (sin, cos) = self.living.yaw.to_radians().sin_cos();
        self.motion.x += strafe * cos - forward * sin;
        self.motion.z += forward * cos + strafe * sin;
    }

    /// `Entity.moveEntity`: collide, step up, stop on blocked axes, and count
    /// steps for `onEntityWalking`.
    fn move_entity(
        &mut self,
        chunks: &WorldChunks,
        steps: Option<&mut StepDistance>,
        block_ticks: Option<&mut BlockTicks>,
    ) {
        let movement = move_entity(
            self.aabb(),
            self.motion,
            self.step_height,
            self.collision.on_ground,
            chunks,
        );
        self.feet = self.size.position_from_aabb(movement.aabb);
        self.collision = movement.collision;
        if movement.collision.collided_x {
            self.motion.x = 0.0;
        }
        if movement.collision.collided_y {
            self.motion.y = 0.0;
        }
        if movement.collision.collided_z {
            self.motion.z = 0.0;
        }
        step_on_block(steps, block_ticks, chunks, movement, false);
    }

    /// `applyEntityCollision` against every pushable body nearby. Both bodies'
    /// updates push the pair apart, so each feels the push twice a tick.
    fn push_apart(&mut self, world: &Surroundings) {
        let area = grow(self.aabb(), Vec3::new(0.2, 0.0, 0.2));
        for &(other, aabb) in world.crowd {
            if other == self.entity || !aabb.intersects(area) {
                continue;
            }
            let mut dx = (aabb.min.x + aabb.max.x) * 0.5 - self.feet.x;
            let mut dz = (aabb.min.z + aabb.max.z) * 0.5 - self.feet.z;
            let distance = dx.abs().max(dz.abs());
            if distance < 0.01 {
                continue;
            }
            let distance = distance.sqrt();
            let strength = (1.0 / distance).min(1.0) * 0.05;
            dx = dx / distance * strength;
            dz = dz / distance * strength;
            self.motion.x -= dx * 2.0;
            self.motion.z -= dz * 2.0;
        }
    }

    /// `EntityChicken.onLivingUpdate` after its `super` call.
    fn flap(&mut self, wings: &mut Wings, commands: &mut Commands) {
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
            drop_item(commands, ItemId::Egg, 0, self.feet);
            self.mob.egg_timer = self.mob.rng.next_int(6000) as u16 + 6000;
        }
    }

    /// `EntitySquid.onLivingUpdate` after its `super` call.
    fn swim(&mut self, world: &Surroundings, swim: &mut Swim) {
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

    /// The tail of `EntityLiving.onUpdate`: the body turns toward the
    /// direction of travel and is kept within 75° of the head.
    fn update_body_yaw(&mut self) {
        let dx = self.feet.x - self.previous.x;
        let dz = self.feet.z - self.previous.z;
        let living = &mut *self.living;
        let mut toward = living.body_yaw;
        if (dx * dx + dz * dz).sqrt() > 0.05 {
            toward = dz.atan2(dx).to_degrees() - 90.0;
        }
        living.body_yaw += wrap_degrees(toward - living.body_yaw) * 0.3;
        let offset = wrap_degrees(living.yaw - living.body_yaw).clamp(-75.0, 75.0);
        living.body_yaw = living.yaw - offset;
        if offset * offset > 2500.0 {
            living.body_yaw += offset * 0.2;
        }
        // Keep each previous angle within half a turn so renders lerp the
        // short way round.
        living.prev_yaw = near_angle(living.prev_yaw, living.yaw);
        living.prev_body_yaw = near_angle(living.prev_body_yaw, living.body_yaw);
        living.prev_pitch = near_angle(living.prev_pitch, living.pitch);
    }
}

/// Beta's `AxisAlignedBB.expand`: grow (or, with negative amounts, shrink)
/// a box by the same amount on both sides of each axis.
fn grow(aabb: Aabb, amount: Vec3) -> Aabb {
    Aabb::new(aabb.min - amount, aabb.max + amount)
}

/// An angle in degrees folded into `-180..180`.
fn wrap_degrees(angle: f32) -> f32 {
    (angle + 180.0).rem_euclid(360.0) - 180.0
}

/// `previous` moved by whole turns to within half a turn of `current`.
fn near_angle(previous: f32, current: f32) -> f32 {
    current - wrap_degrees(current - previous)
}

/// `EntityLiving.updateRotation`: turn toward `target` by at most `max`.
fn update_rotation(current: f32, target: f32, max: f32) -> f32 {
    current + wrap_degrees(target - current).clamp(-max, max)
}

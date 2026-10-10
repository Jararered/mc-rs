//! Beta 1.7.3 mobs, simulated as `EntityLiving` every world tick.
//!
//! Each tick transcribes `Entity.onEntityUpdate`, `EntityLiving.onEntityUpdate`
//! and `onLivingUpdate`, and the tail of `EntityLiving.onUpdate`, in Beta's
//! blocks-per-tick units:
//!
//! - Fire, lava, drowning, suffocation, and falling hurt through the shared
//!   damage rules in [`super::combat`]. A mob whose health runs out drops its
//!   loot at once and tips over for 20 ticks before it is removed.
//! - `EntityCreature.updatePlayerActionState` walks a [`Path`] from the Beta
//!   [`Pathfinder`]. A creature with a target paths to the player and attacks
//!   when it can see them; otherwise it now and then picks the best of ten
//!   random nearby cells and walks there. Animals prefer grass and light,
//!   monsters the dark. Between paths it falls back to
//!   `EntityLiving.updatePlayerActionState`: idle head turns, glancing at a
//!   player within eight blocks, and despawning when far from them.
//! - `moveEntityWithHeading` accelerates along the head's yaw with Beta's
//!   ground friction, water and lava drag, ladders, and step-up.
//!
//! Each kind's own behavior lives in [`animals`] and [`monsters`]. Mobs only
//! ever target the player: Beta's retaliation against other mobs is not
//! simulated.

mod animals;
mod monsters;

pub use animals::Swim;
pub use animals::Wings;
pub use monsters::Bounce;
pub use monsters::Fuse;
pub use monsters::Hover;

use std::time::Instant;

use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::CollisionState;
use crate::entity::EntityDiagnostics;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::combat::DEATH_TICKS;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::Victim;
use crate::entity::combat::drop_loot;
use crate::entity::combat::hurt_creature;
use crate::entity::combat::remove_dead;
use crate::entity::mobs::Explosion;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mount::Mounted;
use crate::entity::pathfinding::LastSearch;
use crate::entity::pathfinding::Path;
use crate::entity::pathfinding::Pathfinder;
use crate::entity::projectiles::victim_of;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::WATER_CURRENT_PER_TICK;
use crate::physics::burning_in;
use crate::physics::collides;
use crate::physics::eye_in_water;
use crate::physics::inside_opaque_block;
use crate::physics::intersects_liquid;
use crate::physics::lava_contains;
use crate::physics::move_entity;
use crate::physics::raycast_blocks;
use crate::physics::step_on_block;
use crate::physics::water_movement;
use crate::physics::web_slowed;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::player::PlayerMovementInput;
use crate::random::ItemRng;
use crate::rendering::particles::effects::EffectParticles;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::difficulty::Difficulty;
use crate::world::lighting::LightCache;
use crate::world::lighting::beta_brightness;
use crate::world::lighting::combined_light;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;

/// `EntityLiving.jump`.
const JUMP_MOTION: f32 = 0.42;
/// `EntityPlayer.getEyeHeight`. A player's `posY` is already at its eyes.
pub const PLAYER_EYE_HEIGHT: f32 = 0.12;
/// `Entity.maxAir`.
pub const MAX_AIR: i16 = 300;

/// `EntityLiving` facing, steering, walk-cycle, and damage state. Angles are
/// Beta's degrees: yaw 0 faces +Z and 90 faces -X; positive pitch looks down.
///
/// Every mob carries this. Bodies with it are moved by [`tick_creatures`],
/// not by the generic `integrate_bodies`.
#[derive(Component, Clone, Debug)]
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
    /// `hurtTime`: counts down from 10 after a hit; the body flashes red.
    pub hurt_time: i16,
    /// `deathTime`: ticks since health ran out. The body tips over meanwhile.
    pub death_time: i16,
    /// `attackedAtYaw`.
    pub attacked_at_yaw: f32,
    /// `entityAge`: ticks spent idle, which gates despawning.
    pub(crate) entity_age: u32,
    /// `heartsLife` and `field_9346_af`: the invulnerability window.
    pub(crate) hearts_life: i16,
    pub(crate) last_damage: i16,
    /// `playerToAttack`: a player is this creature's target.
    pub(crate) chasing: bool,
    /// Which player that is. `None` while chasing means whoever is nearest,
    /// which is what code that only sets `chasing` gets.
    pub(crate) target: Option<Entity>,
    move_forward: f32,
    move_strafing: f32,
    random_yaw_velocity: f32,
    jumping: bool,
    /// `Entity.inWater`, from this tick's `handleWaterMovement`.
    in_water: bool,
    /// `currentTarget` and `numTicksToChaseTarget`: the player being looked
    /// at, and for how many more ticks.
    looking: Option<i32>,
    path: Option<Path>,
    /// The last search for the player's feet, chasing them or following them
    /// as a wolf's owner.
    last_chase: LastSearch,
    /// `attackTime`: the cooldown between attacks.
    attack_time: i16,
    fall_distance: f32,
    air: i16,
    /// `EntityCreature.hasAttacked`: this tick's attack holds the creature in
    /// place instead of letting it wander.
    has_attacked: bool,
    /// `EntityWolf.isWolfShaking`: wet, and waiting to shake dry.
    pub(crate) wolf_shaking: bool,
    /// `EntityWolf.field_25052_g`: shaking right now.
    pub(crate) wolf_drying: bool,
    /// `EntityWolf.timeWolfIsShaking`, the shake's progress from 0 to 2.
    pub(crate) wolf_shake_time: f32,
    /// `World.sendTrackedEntityStatusUpdatePacket`: something a client has
    /// to be told to draw, such as a wolf's taming hearts (7), its smoke
    /// when that fails (6) or the start of its shake (8). A server takes it;
    /// the game draws these for itself and leaves it.
    pub status: Option<u8>,
}

impl Default for Living {
    fn default() -> Self {
        Self::facing(0.0)
    }
}

impl Living {
    pub fn facing(yaw: f32) -> Self {
        Self {
            yaw,
            prev_yaw: yaw,
            pitch: 0.0,
            prev_pitch: 0.0,
            body_yaw: yaw,
            prev_body_yaw: yaw,
            limb_swing: 0.0,
            limb_amount: 0.0,
            prev_limb_amount: 0.0,
            hurt_time: 0,
            death_time: 0,
            attacked_at_yaw: 0.0,
            entity_age: 0,
            hearts_life: 0,
            last_damage: 0,
            chasing: false,
            target: None,
            move_forward: 0.0,
            move_strafing: 0.0,
            random_yaw_velocity: 0.0,
            jumping: false,
            in_water: false,
            looking: None,
            path: None,
            last_chase: LastSearch::default(),
            attack_time: 0,
            fall_distance: 0.0,
            air: MAX_AIR,
            has_attacked: false,
            wolf_shaking: false,
            wolf_drying: false,
            wolf_shake_time: 0.0,
            status: None,
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

    /// Whether the creature has a player as its target.
    pub fn chasing(&self) -> bool {
        self.chasing
    }

    /// The player it is chasing, once it has picked one out.
    pub fn target(&self) -> Option<Entity> {
        self.target.filter(|_| self.chasing)
    }

    /// Whether the creature is still within its post-hit invulnerability.
    pub fn invulnerable(&self) -> bool {
        self.hearts_life > 10
    }
}

/// How a creature's tick ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fate {
    Alive,
    /// `setEntityDead` without dying: despawned, or a creeper that exploded.
    Despawned,
    /// Its death animation finished.
    Removed,
}

/// A player, as creatures see it this frame.
#[derive(Clone, Copy)]
pub(crate) struct Target {
    pub entity: Entity,
    /// Beta's player `posY` sits at the eyes, so this is the player's `pos`.
    pub eye: Vec3,
    pub feet: Vec3,
    pub alive: bool,
}

impl Target {
    fn aabb(&self) -> Aabb {
        EntitySize::PLAYER.aabb(self.eye)
    }
}

/// A player, when it is riding a creature.
#[derive(Clone, Copy)]
pub(crate) struct Rider {
    pub player: Entity,
    /// The creature it sits on.
    pub vehicle: Entity,
    /// Beta `rotationYaw`, in degrees.
    pub yaw: f32,
    /// The forward key is held.
    pub forward: bool,
}

/// World state shared by every creature this frame.
pub(crate) struct Surroundings<'a> {
    chunks: &'a WorldChunks,
    light: &'a LightCache,
    raining: bool,
    skylight_subtracted: u8,
    /// `Dimension::ambient_light`.
    ambient: f32,
    difficulty: Difficulty,
    /// `World.playerEntities`.
    players: &'a [Target],
    /// Players' mounts (`riddenByEntity`, from the creature's side).
    riders: &'a [Rider],
    /// `GameSettings::pig_steering`.
    pig_steering: bool,
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
            Some(
                Block::StoneSlab | Block::Farmland | Block::CobblestoneStairs | Block::WoodenStairs
            )
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

    /// `World.getLightBrightness`.
    fn brightness(&self, x: i32, y: i32, z: i32) -> f32 {
        beta_brightness(self.light_value(x, y, z), self.ambient)
    }

    /// `World.isDaytime`.
    fn daytime(&self) -> bool {
        self.skylight_subtracted < 4
    }

    /// `World.canBlockSeeTheSky`: at or above the column's light height.
    fn can_see_sky(&self, x: i32, y: i32, z: i32) -> bool {
        self.chunks
            .get(ChunkPosition::from_block(x, z))
            .is_some_and(|chunk| {
                let local_x = x.rem_euclid(CHUNK_SIZE as i32) as usize;
                let local_z = z.rem_euclid(CHUNK_SIZE as i32) as usize;
                y >= i32::from(chunk.heightmap.get(local_x, local_z))
            })
    }

    /// `World.canBlockBeRainedOn`, the rain half of `Entity.isWet`.
    fn rained_on(&self, cell: IVec3) -> bool {
        self.raining && self.can_see_sky(cell.x, cell.y, cell.z)
    }

    /// `rayTraceBlocks(from, to) == null`.
    fn clear_line(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        raycast_blocks(self.chunks, from, delta, delta.length()).is_none()
    }
}

/// The mutable world a creature's tick reaches beyond itself.
pub(crate) struct Effects<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub loot: &'a mut ItemRng,
    pub explosions: &'a mut Vec<Explosion>,
    pub block_ticks: Option<&'a mut BlockTicks>,
    /// Each player that can take damage.
    pub victims: Vec<(Entity, Victim<'a>)>,
    pub particles: Option<&'a mut EffectParticles>,
}

/// The optional, kind-specific components a creature carries.
struct Traits<'a> {
    wings: Option<&'a mut Wings>,
    swim: Option<&'a mut Swim>,
    fuse: Option<&'a mut Fuse>,
    bounce: Option<&'a mut Bounce>,
    hover: Option<&'a mut Hover>,
    steps: Option<&'a mut StepDistance>,
}

/// One creature's simulated state while its ticks run.
pub(crate) struct Body<'a> {
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

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct CreatureItem {
    entity: Entity,
    mob: &'static mut Mob,
    living: &'static mut Living,
    transform: &'static mut Transform,
    velocity: &'static mut Velocity,
    collision: &'static mut CollisionState,
    size: &'static EntitySize,
    step_height: &'static StepHeight,
    previous: &'static mut PreviousTick,
    wings: Option<&'static mut Wings>,
    swim: Option<&'static mut Swim>,
    fuse: Option<&'static mut Fuse>,
    bounce: Option<&'static mut Bounce>,
    hover: Option<&'static mut Hover>,
    steps: Option<&'static mut StepDistance>,
}

/// The client-side resources a creature tick feeds when they exist.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CreatureExtras<'w> {
    diagnostics: Option<ResMut<'w, EntityDiagnostics>>,
    particles: Option<ResMut<'w, EffectParticles>>,
}

/// Run each mob's Beta update once per world tick this frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick_creatures(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Res<LightCache>,
    environment: crate::world::dimension::Environment,
    settings: Option<Res<GameSettings>>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
    mut explosion_writer: MessageWriter<Explosion>,
    mut players: Query<
        (
            Entity,
            (
                &Transform,
                Option<&mut PlayerHealth>,
                Option<&mut PlayerCombat>,
                &mut Velocity,
                Option<&mut Inventory>,
            ),
        ),
        With<Player>,
    >,
    (riders, mut creatures): (
        Query<(Entity, &Transform, &Mounted, Option<&PlayerMovementInput>), With<Player>>,
        Query<CreatureItem, Without<Player>>,
    ),
    mut pathfinder: Local<Pathfinder>,
    mut crowd: Local<Vec<(Entity, Aabb)>>,
    mut loot: Local<ItemRng>,
    mut explosions: Local<Vec<Explosion>>,
    mut spare_armor: Local<Vec<[Option<ItemStack>; 4]>>,
    mut extras: CreatureExtras,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    let start = Instant::now();
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |settings| settings.difficulty);
    let mut players: Vec<_> = players.iter_mut().collect();
    let targets: Vec<Target> = players
        .iter()
        .map(|(entity, (transform, health, combat, ..))| Target {
            entity: *entity,
            eye: transform.translation,
            feet: transform.translation - Vec3::Y * EntitySize::PLAYER.y_offset,
            // Mobs leave a creative or spectating player alone, as they do a
            // dead one.
            alive: health.as_ref().is_none_or(|health| health.current > 0)
                && combat.as_ref().is_none_or(|combat| !combat.invulnerable),
        })
        .collect();
    let riders: Vec<Rider> = riders
        .iter()
        .map(|(player, look, mounted, input)| {
            // Bevy's camera looks along local -Z; Beta's yaw 0 points
            // along +Z.
            let (bevy_yaw, _, _) = look.rotation.to_euler(EulerRot::YXZ);
            Rider {
                player,
                vehicle: mounted.vehicle,
                yaw: (std::f32::consts::PI - bevy_yaw).to_degrees(),
                forward: input.is_some_and(|input| input.forward > 0.0),
            }
        })
        .collect();
    crowd.clear();
    crowd.extend(
        creatures
            .iter()
            .map(|item| (item.entity, item.size.aabb(item.transform.translation))),
    );
    crowd.extend(targets.iter().map(|target| (target.entity, target.aabb())));
    let world = Surroundings {
        chunks: &chunks,
        light: &light,
        raining: environment.is_raining(),
        skylight_subtracted: environment.skylight_subtracted(0.0),
        ambient: environment.ambient_light(),
        difficulty,
        players: &targets,
        riders: &riders,
        pig_steering: settings
            .as_ref()
            .is_none_or(|settings| settings.pig_steering),
        crowd: crowd.as_slice(),
    };
    spare_armor.resize_with(players.len(), Default::default);
    let victims = players
        .iter_mut()
        .zip(spare_armor.iter_mut())
        .filter_map(|((entity, parts), spare)| Some((*entity, victim_of(parts, spare)?)))
        .collect();
    explosions.clear();
    let mut fx = Effects {
        commands: &mut commands,
        loot: &mut loot,
        explosions: &mut explosions,
        block_ticks: block_ticks.as_deref_mut(),
        victims,
        particles: extras.particles.as_deref_mut(),
    };

    for mut item in &mut creatures {
        let entity = item.entity;
        if difficulty == Difficulty::Peaceful && item.mob.kind.hostile()
            || !chunks.contains(ChunkPosition::from_world(
                item.transform.translation.x,
                item.transform.translation.z,
            ))
        {
            fx.commands.entity(entity).despawn();
            continue;
        }
        let mut body = Body {
            entity,
            mob: &mut item.mob,
            living: &mut item.living,
            size: *item.size,
            step_height: item.step_height.0,
            feet: item.transform.translation,
            previous: item.transform.translation,
            motion: item.velocity.0 * TICK_SECONDS,
            collision: *item.collision,
            dead: false,
            teleported: false,
        };
        let mut traits = Traits {
            wings: item.wings.as_deref_mut(),
            swim: item.swim.as_deref_mut(),
            fuse: item.fuse.as_deref_mut(),
            bounce: item.bounce.as_deref_mut(),
            hover: item.hover.as_deref_mut(),
            steps: item.steps.as_deref_mut(),
        };
        let mut fate = Fate::Alive;
        for _ in 0..ticks {
            item.previous.0 = body.feet;
            fate = body.tick(&world, &mut pathfinder, &mut traits, &mut fx);
            if fate != Fate::Alive {
                break;
            }
        }
        let teleported = body.teleported;
        let feet = body.feet;
        let motion = body.motion;
        let collision = body.collision;
        if teleported {
            item.previous.0 = feet;
        }
        item.velocity.0 = motion / TICK_SECONDS;
        *item.collision = collision;
        item.transform.translation = feet;
        match fate {
            Fate::Alive => {}
            Fate::Despawned => fx.commands.entity(entity).despawn(),
            Fate::Removed => remove_dead(fx.commands, entity, &mut item.mob, feet),
        }
    }
    explosion_writer.write_batch(explosions.drain(..));
    if let Some(diagnostics) = extras.diagnostics.as_deref_mut() {
        diagnostics.creatures.record(start.elapsed());
        diagnostics.ticks += u64::from(ticks);
        diagnostics.searches.add(pathfinder.take_stats());
        diagnostics.mobs = crowd.len() - targets.len();
    }
}

impl Body<'_> {
    fn aabb(&self) -> Aabb {
        self.size.aabb(self.feet)
    }

    fn is(&self, kind: MobType) -> bool {
        self.mob.kind == kind
    }

    fn alive(&self) -> bool {
        self.mob.health > 0
    }

    /// `getEyeHeight`, which `EntityWolf` lowers.
    fn eye_height(&self) -> f32 {
        self.size.height * if self.is(MobType::Wolf) { 0.8 } else { 0.85 }
    }

    fn eye(&self) -> Vec3 {
        self.feet + Vec3::Y * self.eye_height()
    }

    /// `Entity.getEntityBrightness`: the light two-thirds of the way up.
    fn brightness(&self, world: &Surroundings) -> f32 {
        world.brightness(
            self.feet.x.floor() as i32,
            (self.feet.y + self.size.height * 0.66).floor() as i32,
            self.feet.z.floor() as i32,
        )
    }

    /// `EntityLiving.canEntityBeSeen`, eye to eye.
    fn can_see(&self, world: &Surroundings, player: &Target) -> bool {
        world.clear_line(self.eye(), player.eye + Vec3::Y * PLAYER_EYE_HEIGHT)
    }

    /// `Entity.isImmuneToFire`.
    fn fire_immune(&self) -> bool {
        matches!(self.mob.kind, MobType::Ghast | MobType::PigZombie)
    }

    /// `attackEntityFrom` against this creature, dropping its loot if the
    /// hit kills it.
    fn hurt(&mut self, hit: Hit, fx: &mut Effects) -> bool {
        let mut velocity = Velocity(self.motion / TICK_SECONDS);
        let wound = hurt_creature(self.mob, self.living, &mut velocity, self.feet, hit);
        self.motion = velocity.0 * TICK_SECONDS;
        if wound.died {
            drop_loot(fx.commands, fx.loot, self.mob, self.feet);
        }
        wound.landed
    }

    fn tick(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
        traits: &mut Traits,
        fx: &mut Effects,
    ) -> Fate {
        self.previous = self.feet;
        let living = &mut *self.living;
        living.prev_yaw = living.yaw;
        living.prev_pitch = living.pitch;
        living.prev_body_yaw = living.body_yaw;
        self.mob.age = self.mob.age.saturating_add(1);
        let was_on_ground = self.collision.on_ground;
        if let Some(fuse) = traits.fuse.as_deref_mut() {
            fuse.prev = self.mob.fuse;
        }
        if let Some(bounce) = traits.bounce.as_deref_mut() {
            bounce.prev_squish = bounce.squish;
        }

        // `Entity.onEntityUpdate`.
        let band = grow(self.aabb(), Vec3::new(-0.001, -0.401, -0.001));
        let (in_water, current) = water_movement(band, world.chunks);
        let entered_water = in_water && !self.living.in_water && self.mob.age > 1;
        self.living.in_water = in_water;
        if entered_water && let Some(particles) = fx.particles.as_deref_mut() {
            let motion = self.motion;
            particles.water_entry(self.feet, self.feet.y.floor(), self.size.width, motion);
        }
        self.motion += current * WATER_CURRENT_PER_TICK;
        if in_water {
            self.living.fall_distance = 0.0;
            self.mob.fire_ticks = 0;
        }
        if self.mob.fire_ticks > 0 {
            if self.fire_immune() {
                self.mob.fire_ticks = (self.mob.fire_ticks - 4).max(0);
            } else {
                if self.mob.fire_ticks % 20 == 0 {
                    self.hurt(Hit::environment(1), fx);
                }
                self.mob.fire_ticks -= 1;
            }
        }
        if lava_contains(self.aabb(), world.chunks) && !self.fire_immune() {
            self.hurt(Hit::environment(4), fx);
            self.mob.fire_ticks = 600;
        }
        if self.feet.y < -64.0 {
            self.hurt(Hit::environment(4), fx);
        }

        // `EntityLiving.onEntityUpdate`.
        if self.alive() && self.inside_opaque_block(world) {
            self.hurt(Hit::environment(1), fx);
        }
        if self.alive() && !self.is(MobType::Squid) && self.head_in_water(world) {
            self.living.air -= 1;
            if self.living.air == -20 {
                self.living.air = 0;
                if let Some(particles) = fx.particles.as_deref_mut() {
                    particles.drown(self.feet, self.motion);
                }
                self.hurt(Hit::environment(2), fx);
            }
            self.mob.fire_ticks = 0;
        } else {
            self.living.air = MAX_AIR;
        }
        let alive = self.alive();
        let living = &mut *self.living;
        living.attack_time = (living.attack_time - 1).max(0);
        living.hurt_time = (living.hurt_time - 1).max(0);
        living.hearts_life = (living.hearts_life - 1).max(0);
        if !alive {
            living.death_time += 1;
            if living.death_time > DEATH_TICKS {
                if let Some(particles) = fx.particles.as_deref_mut() {
                    particles.death_puffs(self.feet, self.size.width, self.size.height);
                }
                return Fate::Removed;
            }
        }

        // `EntityLiving.onLivingUpdate`, with each kind's prelude.
        self.living_prelude(world);
        if !self.alive() {
            // `isMovementBlocked`.
            let living = &mut *self.living;
            living.jumping = false;
            living.move_strafing = 0.0;
            living.move_forward = 0.0;
            living.random_yaw_velocity = 0.0;
        } else {
            match self.mob.kind {
                MobType::Squid => self.squid_action(world, traits.swim.as_deref_mut()),
                MobType::Slime => self.slime_action(world, traits.bounce.as_deref_mut()),
                MobType::Ghast => self.ghast_action(world, traits.hover.as_deref_mut(), fx),
                MobType::Pig if world.pig_steering && self.rider(world).is_some() => {
                    self.steered_action(world);
                }
                _ => {
                    let has_attacked = self.creature_action(world, pathfinder, traits, fx);
                    if self.is(MobType::Wolf) {
                        self.wolf_action(world, pathfinder, has_attacked, fx);
                    }
                }
            }
        }
        let in_lava = lava_contains(self.aabb(), world.chunks);
        let in_water = if self.is(MobType::Squid) {
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
        match self.mob.kind {
            // `EntitySquid.moveEntityWithHeading` only applies its motion.
            MobType::Squid => self.move_entity(world, traits.steps.as_deref_mut(), fx),
            MobType::Ghast => self.fly_with_heading(world, in_lava, fx),
            _ => self.move_with_heading(world, in_lava, traits.steps.as_deref_mut(), fx),
        }
        self.push_apart(world);
        self.living_epilogue(world, traits, fx);

        // The tail of `EntityLiving.onUpdate`, then each kind's `onUpdate`.
        self.update_body_yaw();
        self.update_epilogue(was_on_ground, traits, fx);
        if self.dead {
            Fate::Despawned
        } else {
            Fate::Alive
        }
    }

    /// The player riding this creature.
    fn rider(&self, world: &Surroundings) -> Option<Rider> {
        world
            .riders
            .iter()
            .copied()
            .find(|rider| rider.vehicle == self.entity)
    }

    /// `World.getClosestPlayerToEntity(this, -1)`.
    fn closest_player(&self, world: &Surroundings) -> Option<Target> {
        let distance = |player: &Target| player.eye.distance_squared(self.feet);
        world
            .players
            .iter()
            .copied()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
    }

    /// The closest player a mob would go for: one that can be hurt, or
    /// failing that the closest of all.
    fn closest_prey(&self, world: &Surroundings) -> Option<Target> {
        let distance = |player: &Target| player.eye.distance_squared(self.feet);
        world.players.iter().copied().min_by(|a, b| {
            b.alive
                .cmp(&a.alive)
                .then(distance(a).total_cmp(&distance(b)))
        })
    }

    /// `playerToAttack` while chasing, and otherwise the player this creature
    /// would turn on.
    fn quarry(&self, world: &Surroundings) -> Option<Target> {
        self.living
            .target
            .filter(|_| self.living.chasing)
            .and_then(|target| world.players.iter().copied().find(|p| p.entity == target))
            .or_else(|| self.closest_prey(world))
    }

    /// The Pig Steering feature, in place of `updatePlayerActionState`: a
    /// ridden pig turns to where its rider looks and walks while the rider
    /// holds forward, hopping up what blocks its way. Beta's pig wanders on
    /// regardless of its rider.
    fn steered_action(&mut self, world: &Surroundings) {
        let Some(rider) = self.rider(world) else {
            return;
        };
        let speed = self.move_speed();
        let blocked = self.collision.collided_x || self.collision.collided_z;
        let in_liquid = self.living.in_water || lava_contains(self.aabb(), world.chunks);
        let living = &mut *self.living;
        living.path = None;
        living.looking = None;
        living.random_yaw_velocity = 0.0;
        living.yaw = update_rotation(living.yaw, rider.yaw, 20.0);
        living.pitch = 0.0;
        living.move_strafing = 0.0;
        living.move_forward = if rider.forward { speed } else { 0.0 };
        living.jumping = in_liquid || rider.forward && blocked;
    }

    /// `Entity.isEntityInsideOpaqueBlock`.
    fn inside_opaque_block(&self, world: &Surroundings) -> bool {
        inside_opaque_block(self.eye(), self.size.width, world.chunks)
    }

    /// `Entity.isInsideOfMaterial(Material.water)` at eye level.
    fn head_in_water(&self, world: &Surroundings) -> bool {
        eye_in_water(self.eye(), world.chunks)
    }

    /// `EntityCreature.updatePlayerActionState`. Returns `hasAttacked`: the
    /// creature stood still to attack or was told to sit.
    fn creature_action(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
        traits: &mut Traits,
        fx: &mut Effects,
    ) -> bool {
        self.living.has_attacked =
            self.is(MobType::Wolf) && (self.mob.sitting || self.living.wolf_drying);
        if self.mob.sitting {
            // `EntityWolf.interact` drops the path when told to sit.
            self.living.path = None;
        }
        if !self.living.chasing {
            if let Some(found) = self.find_player_to_attack(world) {
                self.living.chasing = true;
                self.living.target = Some(found);
                self.living.path = self.path_to_player(world, pathfinder);
            }
        } else if !self.quarry(world).is_some_and(|player| player.alive) {
            self.living.chasing = false;
            self.living.target = None;
        } else if let Some(player) = self.quarry(world) {
            let distance = player.eye.distance(self.feet);
            if self.can_see(world, &player) {
                self.attack_player(world, &player, distance, traits, fx);
            } else {
                self.attack_blocked_player(distance, traits);
            }
        }

        let has_attacked = self.living.has_attacked;
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
                && let Some(player) = self.quarry(world)
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
            && let Some(player) = self.quarry(world)
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
            if self.closest_player(world).as_ref().is_some_and(near) {
                self.living.looking = Some(10 + self.mob.rng.next_int(20) as i32);
            } else {
                self.living.random_yaw_velocity = (self.mob.rng.next_float() - 0.5) * 20.0;
            }
        }
        if let Some(ticks) = self.living.looking
            && let Some(player) = self.closest_player(world)
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
            let weight = self.path_weight(world, x, y, z);
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

    /// `getBlockPathWeight`: monsters seek the dark, animals grass and light.
    fn path_weight(&self, world: &Surroundings, x: i32, y: i32, z: i32) -> f32 {
        if self.mob.kind.is_monster() {
            0.5 - world.brightness(x, y, z)
        } else if world.chunks.block_at(x, y - 1, z) == Some(Block::Grass) {
            10.0
        } else {
            world.brightness(x, y, z) - 0.5
        }
    }

    /// `EntityLiving.despawnEntity`. Every mob despawns far from the player,
    /// except a tamed wolf.
    fn despawn(&mut self, world: &Surroundings) {
        let Some(player) = self.closest_player(world) else {
            return;
        };
        if self.is(MobType::Wolf) && self.mob.tamed {
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

    /// `World.getPathToEntity(this, playerToAttack, 16)`. Beta repeats this
    /// every tick while the creature has no path, so the answer to an
    /// unchanged question is reused rather than searched again.
    fn path_to_player(
        &mut self,
        world: &Surroundings,
        pathfinder: &mut Pathfinder,
    ) -> Option<Path> {
        let player = self.quarry(world)?;
        pathfinder.path_to_feet_reusing(
            world.chunks,
            self.feet,
            self.size,
            player.feet,
            16.0,
            &mut self.living.last_chase,
        )
    }

    /// `EntityLiving.moveSpeed` and its overrides.
    fn move_speed(&self) -> f32 {
        match self.mob.kind {
            MobType::Wolf => 1.1,
            MobType::Zombie => 0.5,
            // `EntityPigZombie.onUpdate` quickens once it has a target.
            MobType::PigZombie if self.living.chasing => 0.95,
            MobType::PigZombie => 0.5,
            MobType::Spider => 0.8,
            _ => 0.7,
        }
    }

    /// `getVerticalFaceSpeed`, which a sitting wolf halves.
    fn vertical_face_speed(&self) -> f32 {
        if self.is(MobType::Wolf) && self.mob.sitting {
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

    /// `EntityLiving.moveEntityWithHeading`.
    fn move_with_heading(
        &mut self,
        world: &Surroundings,
        in_lava: bool,
        steps: Option<&mut StepDistance>,
        fx: &mut Effects,
    ) {
        let strafe = self.living.move_strafing;
        let forward = self.living.move_forward;
        let chunks = world.chunks;
        if self.living.in_water || in_lava {
            let start_y = self.feet.y;
            self.move_flying(strafe, forward, 0.02);
            self.move_entity(world, steps, fx);
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
            if self.on_ladder(chunks) {
                self.motion.x = self.motion.x.clamp(-0.15, 0.15);
                self.motion.z = self.motion.z.clamp(-0.15, 0.15);
                self.living.fall_distance = 0.0;
                self.motion.y = self.motion.y.max(-0.15);
            }
            self.move_entity(world, steps, fx);
            if (self.collision.collided_x || self.collision.collided_z) && self.on_ladder(chunks) {
                self.motion.y = 0.2;
            }
            self.motion.y = (self.motion.y - 0.08) * 0.98;
            self.motion.x *= friction;
            self.motion.z *= friction;
        }
        self.swing_limbs();
    }

    /// `EntityFlying.moveEntityWithHeading`: the same drag, but no gravity.
    fn fly_with_heading(&mut self, world: &Surroundings, in_lava: bool, fx: &mut Effects) {
        let strafe = self.living.move_strafing;
        let forward = self.living.move_forward;
        if self.living.in_water || in_lava {
            self.move_flying(strafe, forward, 0.02);
            self.move_entity(world, None, fx);
            self.motion *= if self.living.in_water { 0.8 } else { 0.5 };
        } else {
            let friction = self.ground_friction(world.chunks);
            let acceleration = if self.collision.on_ground {
                0.1 * (0.162_771_36 / (friction * friction * friction))
            } else {
                0.02
            };
            self.move_flying(strafe, forward, acceleration);
            let friction = self.ground_friction(world.chunks);
            self.move_entity(world, None, fx);
            self.motion *= friction;
        }
        self.swing_limbs();
    }

    /// The walk-cycle bookkeeping at the end of `moveEntityWithHeading`.
    fn swing_limbs(&mut self) {
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
            Some(block) if block != Block::Air => block.slipperiness() * 0.91,
            _ => 0.546,
        }
    }

    /// `EntityLiving.isOnLadder`. A spider treats any wall it bumps as one,
    /// and a ghast never climbs.
    fn on_ladder(&self, chunks: &WorldChunks) -> bool {
        match self.mob.kind {
            MobType::Spider => self.collision.collided_x || self.collision.collided_z,
            MobType::Ghast => false,
            _ => chunks
                .block_at(
                    self.feet.x.floor() as i32,
                    self.aabb().min.y.floor() as i32,
                    self.feet.z.floor() as i32,
                )
                .is_some_and(|block| block.is_ladder()),
        }
    }

    /// `Entity.isOffsetPositionInLiquid`, which despite its name is true when
    /// the moved box would touch neither a solid block nor any liquid.
    fn offset_position_clear(&self, chunks: &WorldChunks, offset: Vec3) -> bool {
        let moved = self.aabb().offset(offset);
        !collides(chunks, moved) && !intersects_liquid(moved, chunks)
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

    /// `Entity.moveEntity`: collide, step up, stop on blocked axes, take fall
    /// damage on landing, count steps for `onEntityWalking`, and catch fire
    /// from fire or lava the body touches.
    fn move_entity(
        &mut self,
        world: &Surroundings,
        steps: Option<&mut StepDistance>,
        fx: &mut Effects,
    ) {
        let step = web_slowed(self.aabb(), world.chunks, &mut self.motion);
        let mut movement = move_entity(
            self.aabb(),
            step,
            self.step_height,
            self.collision.on_ground,
            world.chunks,
        );
        // The edge of the loaded world is a wall. Unloaded chunks have no
        // collision, and a mob that walked into one would be dropped without
        // being saved with any chunk.
        let moved = self.size.position_from_aabb(movement.aabb);
        if !world
            .chunks
            .contains(ChunkPosition::from_world(moved.x, moved.z))
        {
            movement = move_entity(
                self.aabb(),
                Vec3::new(0.0, self.motion.y, 0.0),
                self.step_height,
                self.collision.on_ground,
                world.chunks,
            );
            movement.collision.collided_x = true;
            movement.collision.collided_z = true;
        }
        self.feet = self.size.position_from_aabb(movement.aabb);
        self.collision = movement.collision;
        // `updateFallState`.
        if movement.collision.on_ground {
            if self.living.fall_distance > 0.0 {
                self.fall(world, fx);
            }
        } else if movement.displacement.y < 0.0 {
            self.living.fall_distance -= movement.displacement.y;
        }
        if movement.collision.collided_x {
            self.motion.x = 0.0;
        }
        if movement.collision.collided_y {
            self.motion.y = 0.0;
        }
        if movement.collision.collided_z {
            self.motion.z = 0.0;
        }
        step_on_block(
            steps,
            fx.block_ticks.as_deref_mut(),
            world.chunks,
            movement,
            false,
        );

        let foot = self.feet.floor().as_ivec3();
        let wet = self.living.in_water || world.rained_on(foot);
        if burning_in(grow(self.aabb(), Vec3::splat(-0.001)), world.chunks) {
            if !self.fire_immune() {
                self.hurt(Hit::environment(1), fx);
            }
            if !wet {
                self.mob.fire_ticks += 1;
                if self.mob.fire_ticks == 0 {
                    self.mob.fire_ticks = 300;
                }
            }
        } else if self.mob.fire_ticks <= 0 {
            // `-fireResistance`.
            self.mob.fire_ticks = -1;
        }
        if wet && self.mob.fire_ticks > 0 {
            self.mob.fire_ticks = -1;
        }
    }

    /// `EntityLiving.fall`: three blocks free, then a point per block.
    /// Chickens and ghasts never take it.
    /// A rider lands with its mount (`Entity.fall`).
    fn fall(&mut self, world: &Surroundings, fx: &mut Effects) {
        let distance = std::mem::take(&mut self.living.fall_distance);
        if let Some(rider) = self.rider(world) {
            let damage = (distance - 3.0).ceil() as i16;
            if damage > 0 {
                self.strike(world, rider.player, Hit::environment(damage), fx);
            }
        }
        if matches!(self.mob.kind, MobType::Chicken | MobType::Ghast) {
            return;
        }
        let damage = (distance - 3.0).ceil() as i16;
        if damage > 0 {
            self.hurt(Hit::environment(damage), fx);
        }
    }

    /// `applyEntityCollision` against every pushable body nearby. Both bodies'
    /// updates push the pair apart, so each feels the push twice a tick.
    fn push_apart(&mut self, world: &Surroundings) {
        let area = grow(self.aabb(), Vec3::new(0.2, 0.0, 0.2));
        // A mount and its rider do not push each other.
        let rider = self.rider(world).map(|rider| rider.player);
        for &(other, aabb) in world.crowd {
            if other == self.entity || rider == Some(other) || !aabb.intersects(area) {
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

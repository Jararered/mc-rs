//! `EntityBoat`: floating, the rider's push, wrecks, and damage.
//!
//! [`step_boat`] is `EntityBoat.onUpdate` for one world tick (the server
//! branch), in Beta's order. `Transform.translation` is Beta's
//! `posX/posY/posZ`: the centre of the box.
//!
//! Not copied: the `splash` particles of a fast boat (there is no such
//! particle yet), and the boat's solid collision box, so a body cannot stand
//! on one.
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::drops::items::spawn_entity_drop;
use crate::entity::minecart::push_offset;
use crate::entity::mobs::Mob;
use crate::entity::mount::Mounted;
use crate::entity::mount::Seat;
use crate::entity::mount::dismount;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::move_entity;
use crate::physics::water_within;
use crate::player::Player;
use crate::player::PlayerMovementInput;
use crate::random::ItemRng;
use crate::rendering::particles::effects::EffectParticles;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

pub const BOAT_SIZE: EntitySize = EntitySize {
    width: 1.5,
    height: 0.6,
    y_offset: 0.3,
};

/// `EntityBoat.getMountedYOffset`: a rider sits this far from the centre.
pub const MOUNTED_OFFSET: f32 = -0.3;
/// How far along the boat's yaw `updateRiderPosition` seats the rider.
pub const SEAT_DISTANCE: f32 = 0.4;
/// `boatCurrentDamage` above which a hit breaks the boat.
const BREAK_DAMAGE: i32 = 40;
/// The horizontal speed above which running into something wrecks the boat.
const WRECK_SPEED: f32 = 0.15;
const MAX_SPEED: f32 = 0.4;
/// Vertical drag on a floating boat with `BoatRules::steady`.
const STEADY_WATER_DRAG: f32 = 0.8;

#[derive(Component, Clone, Debug, PartialEq)]
pub struct Boat {
    /// Blocks per world tick (`motionX/Y/Z`).
    pub motion: Vec3,
    /// `rotationYaw` in degrees. It points back along the boat's travel.
    pub yaw: f32,
    /// `prevRotationYaw`, for drawing between ticks.
    pub prev_yaw: f32,
    /// `boatCurrentDamage`.
    pub damage: i32,
    /// `boatTimeSinceHit`.
    pub time_since_hit: i32,
    /// `boatRockDirection`.
    pub rock_direction: i32,
    /// `onGround`, from the last sweep.
    pub on_ground: bool,
}

impl Default for Boat {
    fn default() -> Self {
        Self {
            motion: Vec3::ZERO,
            yaw: 0.0,
            prev_yaw: 0.0,
            damage: 0,
            time_since_hit: 0,
            rock_direction: 1,
            on_ground: false,
        }
    }
}

impl Boat {
    /// `attackEntityFrom`: rock, add `amount * 10` damage, and report whether
    /// the boat now breaks.
    pub fn hurt(&mut self, amount: i32) -> bool {
        self.rock_direction = -self.rock_direction;
        self.time_since_hit = 10;
        self.damage += amount * 10;
        self.damage > BREAK_DAMAGE
    }

    /// `updateRiderPosition`: where the rider sits, from the boat's centre.
    pub fn seat(&self) -> Vec3 {
        Self::seat_at(self.yaw)
    }

    /// The seat as it was at the start of the last tick, which the rider's
    /// view is drawn from on the way to [`Self::seat`].
    pub fn previous_seat(&self) -> Vec3 {
        Self::seat_at(self.prev_yaw)
    }

    fn seat_at(yaw: f32) -> Vec3 {
        let (sin, cos) = yaw.to_radians().sin_cos();
        Vec3::new(cos * SEAT_DISTANCE, MOUNTED_OFFSET, sin * SEAT_DISTANCE)
    }
}

/// `ItemBoat.onItemRightClick`: a boat resting on top of `cell`.
pub fn spawn_boat(commands: &mut Commands, cell: IVec3) -> Entity {
    let center = Vec3::new(
        cell.x as f32 + 0.5,
        cell.y as f32 + 1.0 + BOAT_SIZE.y_offset,
        cell.z as f32 + 0.5,
    );
    spawn_boat_at(commands, center, Boat::default())
}

pub fn spawn_boat_at(commands: &mut Commands, center: Vec3, boat: Boat) -> Entity {
    commands
        .spawn((
            Name::new("Boat"),
            boat,
            Seat::default(),
            BOAT_SIZE,
            PreviousTick(center),
            Transform::from_translation(center),
            Visibility::default(),
        ))
        .id()
}

/// What Beta's rider carries in `motionX/Z` when its boat reads them:
/// `updateRidden` zeroes its motion, then one airborne `moveFlying` (0.02)
/// and the air drag (0.91) of `moveEntityWithHeading` run. `yaw` is Beta's,
/// in radians; `strafe` and `forward` already carry the sneaking factor.
pub fn rider_motion(strafe: f32, forward: f32, yaw: f32) -> Vec2 {
    const AIR_ACCELERATION: f32 = 0.02;
    const AIR_DRAG: f32 = 0.91;
    let strafe = strafe * 0.98;
    let forward = forward * 0.98;
    let magnitude = strafe.hypot(forward);
    if magnitude < 0.01 {
        return Vec2::ZERO;
    }
    let scale = AIR_ACCELERATION / magnitude.max(1.0);
    let (sin, cos) = yaw.sin_cos();
    Vec2::new(
        (strafe * cos - forward * sin) * scale,
        (forward * cos + strafe * sin) * scale,
    ) * AIR_DRAG
}

/// The Features toggles that depart from Beta's boat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoatRules {
    /// `GameSettings::steady_boats`: buoyancy follows how deep the hull sits
    /// instead of counting wet fifths, so a floating boat comes to rest.
    /// Beta's hunts a few centimetres up and down for ever.
    pub steady: bool,
    /// `GameSettings::boat_crashes`: the speed that wrecks a boat is read
    /// before the move, so a square hit breaks it too. Beta reads it after
    /// the blocked axis has been stopped, and only a glancing hit counts.
    pub crashes: bool,
}

impl BoatRules {
    pub const BETA: Self = Self {
        steady: false,
        crashes: false,
    };
    pub const FEATURES: Self = Self {
        steady: true,
        crashes: true,
    };
}

/// How much of the hull is under water, as `onUpdate` counts it: fifths of
/// its height, lowered by an eighth, each wet if any water reaches it.
fn wet_fifths(aabb: Aabb, chunks: &WorldChunks) -> f32 {
    const SLICES: i32 = 5;
    let height = aabb.max.y - aabb.min.y;
    let level = |slice: i32| aabb.min.y + height * slice as f32 / SLICES as f32 - 0.125;
    let wet = (0..SLICES)
        .filter(|&slice| {
            let layer = Aabb::new(
                Vec3::new(aabb.min.x, level(slice), aabb.min.z),
                Vec3::new(aabb.max.x, level(slice + 1), aabb.max.z),
            );
            water_within(layer, chunks)
        })
        .count();
    wet as f32 / SLICES as f32
}

/// The same measure without the steps: the depth of water over the lowered
/// hull bottom as a share of its height. A tenth is added below full so the
/// boat rides at Beta's waterline, where a partly wet fifth counts whole.
fn wet_share(aabb: Aabb, chunks: &WorldChunks) -> f32 {
    let height = aabb.max.y - aabb.min.y;
    let bottom = aabb.min.y - 0.125;
    let top = bottom + height;
    // Water whose surface is at or above `level`, anywhere under the hull.
    let reaches = |level: f32| {
        let layer = Aabb::new(
            Vec3::new(aabb.min.x, level, aabb.min.z),
            Vec3::new(aabb.max.x, top, aabb.max.z),
        );
        water_within(layer, chunks)
    };
    if !reaches(bottom) {
        return 0.0;
    }
    if reaches(top) {
        return 1.0;
    }
    let (mut wet, mut dry) = (bottom, top);
    for _ in 0..10 {
        let middle = (wet + dry) * 0.5;
        if reaches(middle) {
            wet = middle;
        } else {
            dry = middle;
        }
    }
    ((wet - bottom) / height + 0.1).min(1.0)
}

/// What one tick asks of the world around the boat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoatStep {
    /// The boat ran into something too fast and breaks apart.
    pub wrecked: bool,
    /// Snow layers under the boat's corners, which it clears away.
    pub snow: Vec<IVec3>,
}

/// `EntityBoat.onUpdate` for one world tick. `rider_motion` is the rider's
/// `motionX/Z`, when there is a rider; [`BoatRules::BETA`] is Beta's boat.
pub fn step_boat(
    boat: &mut Boat,
    center: &mut Vec3,
    rider_motion: Option<Vec2>,
    rules: BoatRules,
    chunks: &WorldChunks,
) -> BoatStep {
    if boat.time_since_hit > 0 {
        boat.time_since_hit -= 1;
    }
    if boat.damage > 0 {
        boat.damage -= 1;
    }
    let previous = *center;
    boat.prev_yaw = boat.yaw;
    let aabb = BOAT_SIZE.aabb(*center);
    let submerged = if rules.steady {
        wet_share(aabb, chunks)
    } else {
        wet_fifths(aabb, chunks)
    };
    if submerged < 1.0 {
        boat.motion.y += 0.04 * (submerged * 2.0 - 1.0);
        if rules.steady && submerged > 0.0 {
            // The water's drag on the hull, which stills the bob within a
            // second or two of a drop.
            boat.motion.y *= STEADY_WATER_DRAG;
        }
    } else {
        if boat.motion.y < 0.0 {
            boat.motion.y /= 2.0;
        }
        boat.motion.y += 0.007;
    }
    if let Some(rider) = rider_motion {
        boat.motion.x += rider.x * 0.2;
        boat.motion.z += rider.y * 0.2;
    }
    boat.motion.x = boat.motion.x.clamp(-MAX_SPEED, MAX_SPEED);
    boat.motion.z = boat.motion.z.clamp(-MAX_SPEED, MAX_SPEED);
    if boat.on_ground {
        boat.motion *= 0.5;
    }
    // `Entity.moveEntity`: the blocked axes lose their motion before Beta
    // measures the speed, so a square hit on a wall does not wreck the boat
    // but a glancing one does.
    let delta = boat.motion;
    let arriving = delta.xz().length();
    let moved = move_entity(aabb, delta, 0.0, boat.on_ground, chunks);
    *center = BOAT_SIZE.position_from_aabb(moved.aabb);
    boat.on_ground = delta.y < 0.0 && moved.collision.collided_y;
    if moved.collision.collided_x {
        boat.motion.x = 0.0;
    }
    if moved.collision.collided_y {
        boat.motion.y = 0.0;
    }
    if moved.collision.collided_z {
        boat.motion.z = 0.0;
    }
    let speed = if rules.crashes {
        arriving
    } else {
        boat.motion.xz().length()
    };
    if (moved.collision.collided_x || moved.collision.collided_z) && speed > WRECK_SPEED {
        return BoatStep {
            wrecked: true,
            snow: Vec::new(),
        };
    }
    boat.motion.x *= 0.99;
    boat.motion.y *= 0.95;
    boat.motion.z *= 0.99;

    let mut heading = boat.yaw;
    let dx = previous.x - center.x;
    let dz = previous.z - center.z;
    if dx * dx + dz * dz > 0.001 {
        heading = dz.atan2(dx).to_degrees();
    }
    let mut turn = heading - boat.yaw;
    while turn >= 180.0 {
        turn -= 360.0;
    }
    while turn < -180.0 {
        turn += 360.0;
    }
    boat.yaw += turn.clamp(-20.0, 20.0);

    let mut snow = Vec::new();
    for corner in 0..4 {
        let cell = IVec3::new(
            (center.x + ((corner % 2) as f32 - 0.5) * 0.8).floor() as i32,
            center.y.floor() as i32,
            (center.z + ((corner / 2) as f32 - 0.5) * 0.8).floor() as i32,
        );
        if chunks.block_at(cell.x, cell.y, cell.z) == Some(Block::SnowLayer)
            && !snow.contains(&cell)
        {
            snow.push(cell);
        }
    }
    BoatStep {
        wrecked: false,
        snow,
    }
}

/// A wrecked or beaten boat: the rider steps off and three planks and two
/// sticks are left.
pub fn break_boat(
    commands: &mut Commands,
    rng: &mut ItemRng,
    entity: Entity,
    rider: Option<Entity>,
    center: Vec3,
) {
    if let Some(rider) = rider {
        dismount(commands, rider);
    }
    commands.entity(entity).despawn();
    let planks = ItemStack::from_block(Block::WoodenPlanks, 1);
    let sticks = ItemStack::new(Item::Stick, 1);
    for (stack, count) in [(planks, 3), (sticks, 2)] {
        let Ok(stack) = stack else {
            continue;
        };
        for _ in 0..count {
            spawn_entity_drop(commands, rng, center, stack);
        }
    }
}

/// `Entity.applyEntityCollision` between two boats: each is pushed away from
/// the other.
pub fn collide_boats(this: &mut Boat, at: Vec3, other: &mut Boat, other_at: Vec3) {
    let Some(offset) = push_offset(at, other_at) else {
        return;
    };
    this.motion.x -= offset.x;
    this.motion.z -= offset.y;
    other.motion.x += offset.x;
    other.motion.z += offset.y;
}

fn grow(aabb: Aabb, amount: Vec3) -> Aabb {
    Aabb::new(aabb.min - amount, aabb.max + amount)
}

/// Seconds per world tick, to turn a per-tick change of motion into
/// [`Velocity`]'s blocks per second.
const TICK_SECONDS: f32 = 0.05;

/// The creatures and the player that overlap a boat push it and are nudged
/// back (`canBePushed`). A boat's own rider does not push it.
pub(crate) fn bump_boats(
    tick: Res<WorldTick>,
    mut boats: Query<(&Transform, &mut Boat), (Without<Living>, Without<Player>)>,
    mut bodies: Query<
        (&Transform, &EntitySize, &mut Velocity, Option<&Mob>),
        (Or<(With<Living>, With<Player>)>, Without<Mounted>),
    >,
) {
    for _ in 0..tick.ticks_this_frame() {
        for (boat_transform, mut boat) in &mut boats {
            let center = boat_transform.translation;
            let boat_box = BOAT_SIZE.aabb(center);
            for (transform, size, mut velocity, mob) in &mut bodies {
                if mob.is_some_and(|mob| mob.health <= 0) {
                    continue;
                }
                let reach = grow(size.aabb(transform.translation), Vec3::new(0.2, 0.0, 0.2));
                if !reach.intersects(boat_box) {
                    continue;
                }
                if let Some(offset) = push_offset(center, transform.translation) {
                    boat.motion.x -= offset.x;
                    boat.motion.z -= offset.y;
                    velocity.0.x += offset.x / TICK_SECONDS;
                    velocity.0.z += offset.y / TICK_SECONDS;
                }
            }
        }
    }
}

/// Run the boats' Beta update once per world tick, in a stable order, with
/// the boat-to-boat bumps of `EntityBoat.onUpdate` after each boat moves.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick_boats(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: ResMut<BlockTicks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut boats: Query<
        (Entity, &mut Boat, &Seat, &mut Transform, &mut PreviousTick),
        Without<Player>,
    >,
    riders: Query<(&Transform, &PlayerMovementInput), With<Player>>,
    mut particles: Option<ResMut<EffectParticles>>,
    mut rng: Local<ItemRng>,
    mut order: Local<Vec<Entity>>,
    mut wrecked: Local<Vec<Entity>>,
) {
    let count = tick.ticks_this_frame();
    if count == 0 {
        return;
    }
    let rules = settings
        .as_ref()
        .map_or(BoatRules::FEATURES, |settings| BoatRules {
            steady: settings.steady_boats,
            crashes: settings.boat_crashes,
        });
    order.clear();
    order.extend(boats.iter().map(|(entity, ..)| entity));
    order.sort();
    wrecked.clear();
    for _ in 0..count {
        for &entity in order.iter() {
            if wrecked.contains(&entity) {
                continue;
            }
            let Ok((_, mut boat, seat, mut transform, mut previous)) = boats.get_mut(entity) else {
                continue;
            };
            if !chunks.contains(ChunkPosition::from_world(
                transform.translation.x,
                transform.translation.z,
            )) {
                continue;
            }
            let push = seat.rider.map(|rider| {
                riders.get(rider).map_or(Vec2::ZERO, |(look, input)| {
                    // Bevy's camera looks along local -Z; Beta's yaw 0
                    // points along +Z.
                    let (bevy_yaw, _, _) = look.rotation.to_euler(EulerRot::YXZ);
                    rider_motion(input.strafe, input.forward, std::f32::consts::PI - bevy_yaw)
                })
            });
            previous.0 = transform.translation;
            let mut center = transform.translation;
            let step = step_boat(&mut boat, &mut center, push, rules, &chunks);
            transform.translation = center;
            if !step.wrecked
                && let Some(particles) = particles.as_deref_mut()
            {
                // The speed Beta measures is before the 0.99 drag.
                let speed = boat.motion.xz().length() / 0.99;
                particles.boat_wake(center, boat.yaw, speed, boat.motion);
            }
            if step.wrecked {
                break_boat(&mut commands, &mut rng, entity, seat.rider, center);
                wrecked.push(entity);
                continue;
            }
            for cell in step.snow {
                let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
                if let Some(old) = chunks.set_block(cell.x, cell.y, cell.z, Block::Air) {
                    ticks.block_changed(cell, old, metadata);
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                    }
                    if let Some(streaming) = streaming.as_deref_mut() {
                        streaming.request_block_update(cell.x, cell.y, cell.z);
                    }
                }
            }
            let reach = grow(BOAT_SIZE.aabb(center), Vec3::new(0.2, 0.0, 0.2));
            for &other in order.iter() {
                if other == entity || wrecked.contains(&other) {
                    continue;
                }
                let Ok([(_, mut this, _, this_at, _), (_, mut mover, _, mover_at, _)]) =
                    boats.get_many_mut([other, entity])
                else {
                    continue;
                };
                if !BOAT_SIZE.aabb(this_at.translation).intersects(reach) {
                    continue;
                }
                collide_boats(
                    &mut this,
                    this_at.translation,
                    &mut mover,
                    mover_at.translation,
                );
            }
        }
    }
}

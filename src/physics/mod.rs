//! Voxel AABB collision, matching Beta 1.7.3 `Entity.moveEntity`.
//!
//! Bodies are Bevy entities with [`Velocity`] and [`EntitySize`]. The player
//! and creatures ([`Living`]) run Beta's per-tick movement on [`move_entity`];
//! `integrate_bodies` moves the remaining bodies on frame time.

mod raycast;

pub use raycast::BLOCK_REACH;
pub use raycast::BlockFace;
pub use raycast::BlockHit;
pub use raycast::block_hit_distance;
pub use raycast::raycast_blocks;
pub use raycast::raycast_blocks_or_liquid;
pub use raycast::raycast_collision;
pub use raycast::segment_entry;

use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::block::fluids::Fluid;
use crate::block::fluids::flow_vector;
use crate::block::fluids::is_lava;
use crate::block::fluids::is_water;
use crate::block::fluids::percent_air;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::Gravity;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::player::PlayerMovementInput;
use crate::player::PlayerSurvival;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;

/// Fall speed cap, from Beta's `(v - 0.08) * 0.98` terminal velocity.
const TERMINAL_VELOCITY: f32 = 78.4;
/// Longest single integration step, so a body cannot tunnel through more than
/// this many seconds of motion at once. A longer frame is split into steps.
const MAX_STEP_SECS: f32 = 0.05;
/// Frame time integrated at most, so a long hitch does not replay as a burst.
const MAX_FRAME_SECS: f32 = 0.25;
/// Covers the f32 error from converting a player's feet to eye height and back.
const CONTACT_EPSILON: f32 = 1e-4;
/// `World.handleMaterialAcceleration`: current added to motion each world tick.
pub const WATER_CURRENT_PER_TICK: f32 = 0.014;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhysicsSet {
    ApplyInput,
    Integrate,
}

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTick>()
            .configure_sets(Update, PhysicsSet::Integrate.after(PhysicsSet::ApplyInput))
            .add_systems(
                Update,
                (integrate_bodies, integrate_player)
                    .in_set(PhysicsSet::Integrate)
                    .run_if(physics_should_run),
            );
    }
}

fn physics_should_run(screen: Option<Res<State<AppScreen>>>) -> bool {
    screen.is_none_or(|screen| *screen.get() == AppScreen::Playing)
}

/// Axis-aligned box used for entity-vs-block collision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_block(x: i32, y: i32, z: i32) -> Self {
        let min = Vec3::new(x as f32, y as f32, z as f32);
        Self {
            min,
            max: min + Vec3::ONE,
        }
    }

    pub fn offset(self, delta: Vec3) -> Self {
        Self {
            min: self.min + delta,
            max: self.max + delta,
        }
    }

    /// Grow the box to cover this volume plus a movement sweep.
    pub fn expand(self, delta: Vec3) -> Self {
        Self {
            min: Vec3::new(
                if delta.x < 0.0 {
                    self.min.x + delta.x
                } else {
                    self.min.x
                },
                if delta.y < 0.0 {
                    self.min.y + delta.y
                } else {
                    self.min.y
                },
                if delta.z < 0.0 {
                    self.min.z + delta.z
                } else {
                    self.min.z
                },
            ),
            max: Vec3::new(
                if delta.x > 0.0 {
                    self.max.x + delta.x
                } else {
                    self.max.x
                },
                if delta.y > 0.0 {
                    self.max.y + delta.y
                } else {
                    self.max.y
                },
                if delta.z > 0.0 {
                    self.max.z + delta.z
                } else {
                    self.max.z
                },
            ),
        }
    }

    pub fn intersects(self, other: Self) -> bool {
        other.max.x > self.min.x
            && other.min.x < self.max.x
            && other.max.y > self.min.y
            && other.min.y < self.max.y
            && other.max.z > self.min.z
            && other.min.z < self.max.z
    }

    /// Clip `delta` so `other` does not move into this box along X.
    pub fn calculate_x_offset(self, other: Self, mut delta: f32) -> f32 {
        if other.max.y <= self.min.y || other.min.y >= self.max.y {
            return delta;
        }
        if other.max.z <= self.min.z || other.min.z >= self.max.z {
            return delta;
        }
        if delta > 0.0 && other.max.x <= self.min.x {
            let gap = self.min.x - other.max.x;
            if gap < delta {
                delta = gap;
            }
        }
        if delta < 0.0 && other.min.x >= self.max.x {
            let gap = self.max.x - other.min.x;
            if gap > delta {
                delta = gap;
            }
        }
        delta
    }

    pub fn calculate_y_offset(self, other: Self, mut delta: f32) -> f32 {
        if other.max.x <= self.min.x || other.min.x >= self.max.x {
            return delta;
        }
        if other.max.z <= self.min.z || other.min.z >= self.max.z {
            return delta;
        }
        if delta > 0.0 && other.max.y <= self.min.y {
            let gap = self.min.y - other.max.y;
            if gap < delta {
                delta = gap;
            }
        }
        if delta < 0.0 && other.min.y >= self.max.y - CONTACT_EPSILON {
            let gap = self.max.y - other.min.y;
            if gap > delta {
                delta = gap;
            }
        }
        delta
    }

    pub fn calculate_z_offset(self, other: Self, mut delta: f32) -> f32 {
        if other.max.x <= self.min.x || other.min.x >= self.max.x {
            return delta;
        }
        if other.max.y <= self.min.y || other.min.y >= self.max.y {
            return delta;
        }
        if delta > 0.0 && other.max.z <= self.min.z {
            let gap = self.min.z - other.max.z;
            if gap < delta {
                delta = gap;
            }
        }
        if delta < 0.0 && other.min.z >= self.max.z {
            let gap = self.max.z - other.min.z;
            if gap > delta {
                delta = gap;
            }
        }
        delta
    }
}

impl EntitySize {
    pub fn aabb(self, position: Vec3) -> Aabb {
        let half = self.width * 0.5;
        let min_y = position.y - self.y_offset;
        Aabb {
            min: Vec3::new(position.x - half, min_y, position.z - half),
            max: Vec3::new(position.x + half, min_y + self.height, position.z + half),
        }
    }

    pub fn position_from_aabb(self, aabb: Aabb) -> Vec3 {
        Vec3::new(
            (aabb.min.x + aabb.max.x) * 0.5,
            aabb.min.y + self.y_offset,
            (aabb.min.z + aabb.max.z) * 0.5,
        )
    }
}

/// Outcome of [`move_entity`].
#[derive(Clone, Copy, Debug)]
pub struct Movement {
    pub aabb: Aabb,
    pub displacement: Vec3,
    pub collision: CollisionState,
}

/// Sweep `aabb` by `delta` against solid world blocks. Y is resolved first,
/// then X, then Z, then a half-block step-up if the body was blocked while
/// standing on the ground — the Beta `Entity.moveEntity` order.
pub fn move_entity(
    aabb: Aabb,
    delta: Vec3,
    step_height: f32,
    was_on_ground: bool,
    chunks: &WorldChunks,
) -> Movement {
    move_entity_with_sneak(aabb, delta, step_height, was_on_ground, false, chunks)
}

/// Beta `Entity.moveEntity`, including sneak edge braking before collision resolution.
pub fn move_entity_with_sneak(
    mut aabb: Aabb,
    mut delta: Vec3,
    step_height: f32,
    was_on_ground: bool,
    sneaking: bool,
    chunks: &WorldChunks,
) -> Movement {
    let original = delta;
    let before = aabb;
    let sneak_edge = was_on_ground && sneaking;
    if sneak_edge {
        (delta.x, _) = clip_sneak_edge(aabb, delta.x, 0.0, chunks);
        (_, delta.z) = clip_sneak_edge(aabb, 0.0, delta.z, chunks);
    }
    let requested = delta;
    let mut colliders = colliding_aabbs(chunks, aabb.expand(delta));

    for collider in &colliders {
        delta.y = collider.calculate_y_offset(aabb, delta.y);
    }
    aabb = aabb.offset(Vec3::new(0.0, delta.y, 0.0));

    for collider in &colliders {
        delta.x = collider.calculate_x_offset(aabb, delta.x);
    }
    aabb = aabb.offset(Vec3::new(delta.x, 0.0, 0.0));

    for collider in &colliders {
        delta.z = collider.calculate_z_offset(aabb, delta.z);
    }
    aabb = aabb.offset(Vec3::new(0.0, 0.0, delta.z));

    let blocked_horizontally = requested.x != delta.x || requested.z != delta.z;
    let landed = original.y != delta.y && original.y < 0.0;
    if step_height > 0.0 && (was_on_ground || landed) && blocked_horizontally {
        let stepped = try_step(before, requested, step_height, chunks, &mut colliders);
        let stepped_h = stepped.displacement.x.hypot(stepped.displacement.z);
        let current_h = delta.x.hypot(delta.z);
        if stepped_h > current_h {
            aabb = stepped.aabb;
            delta = stepped.displacement;
        }
    }

    Movement {
        aabb,
        displacement: delta,
        collision: CollisionState {
            on_ground: original.y != delta.y && original.y < 0.0,
            collided_x: requested.x != delta.x,
            collided_y: original.y != delta.y,
            collided_z: requested.z != delta.z,
        },
    }
}

fn clip_sneak_edge(aabb: Aabb, mut x: f32, mut z: f32, chunks: &WorldChunks) -> (f32, f32) {
    while x != 0.0 && !collides(chunks, aabb.offset(Vec3::new(x, -1.0, 0.0))) {
        if x.abs() <= 0.05 {
            x = 0.0;
        } else {
            x -= x.signum() * 0.05;
        }
    }
    while z != 0.0 && !collides(chunks, aabb.offset(Vec3::new(0.0, -1.0, z))) {
        if z.abs() <= 0.05 {
            z = 0.0;
        } else {
            z -= z.signum() * 0.05;
        }
    }
    (x, z)
}

/// The step-up attempt of `Entity.moveEntity`. `colliders` is the caller's
/// buffer, refilled for the raised sweep.
fn try_step(
    start: Aabb,
    original: Vec3,
    step_height: f32,
    chunks: &WorldChunks,
    colliders: &mut Vec<Aabb>,
) -> Movement {
    let mut aabb = start;
    let mut delta = Vec3::new(original.x, step_height, original.z);
    colliders.clear();
    visit_colliders(chunks, aabb.expand(delta), |collider| {
        colliders.push(collider);
        false
    });
    let colliders = &*colliders;

    for collider in colliders {
        delta.y = collider.calculate_y_offset(aabb, delta.y);
    }
    aabb = aabb.offset(Vec3::new(0.0, delta.y, 0.0));

    for collider in colliders {
        delta.x = collider.calculate_x_offset(aabb, delta.x);
    }
    aabb = aabb.offset(Vec3::new(delta.x, 0.0, 0.0));

    for collider in colliders {
        delta.z = collider.calculate_z_offset(aabb, delta.z);
    }
    aabb = aabb.offset(Vec3::new(0.0, 0.0, delta.z));

    let mut down = -step_height;
    for collider in colliders {
        down = collider.calculate_y_offset(aabb, down);
    }
    aabb = aabb.offset(Vec3::new(0.0, down, 0.0));
    delta.y += down;

    Movement {
        aabb,
        displacement: delta,
        collision: CollisionState::default(),
    }
}

/// Solid boxes overlapping `area`, including a full-cube floor below y = 0.
pub fn colliding_aabbs(chunks: &WorldChunks, area: Aabb) -> Vec<Aabb> {
    let mut boxes = Vec::new();
    visit_colliders(chunks, area, |collider| {
        boxes.push(collider);
        false
    });
    boxes
}

/// Whether any solid box overlaps `area`: `colliding_aabbs` without the list.
pub fn collides(chunks: &WorldChunks, area: Aabb) -> bool {
    visit_colliders(chunks, area, |_| true)
}

/// Visit the solid boxes overlapping `area` until `visit` returns true, and
/// report whether it did.
fn visit_colliders(chunks: &WorldChunks, area: Aabb, mut visit: impl FnMut(Aabb) -> bool) -> bool {
    let min_x = area.min.x.floor() as i32;
    let max_x = (area.max.x + 1.0).floor() as i32;
    let min_y = area.min.y.floor() as i32;
    let max_y = (area.max.y + 1.0).floor() as i32;
    let min_z = area.min.z.floor() as i32;
    let max_z = (area.max.z + 1.0).floor() as i32;

    for x in min_x..max_x {
        for z in min_z..max_z {
            for y in (min_y - 1)..max_y {
                if y >= CHUNK_HEIGHT as i32 {
                    continue;
                }
                if y < 0 {
                    let floor = Aabb::from_block(x, y, z);
                    if area.intersects(floor) && visit(floor) {
                        return true;
                    }
                    continue;
                }
                for block in block_collision_boxes(chunks, x, y, z).into_iter().flatten() {
                    if area.intersects(block) && visit(block) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// `Block.getCollisionBoundingBoxFromPool` for a world cell, in world space.
/// Deep snow layers collide as a half slab.
/// Stairs collide as two boxes; every other block has at most one.
pub(crate) fn block_collision_boxes(
    chunks: &WorldChunks,
    x: i32,
    y: i32,
    z: i32,
) -> [Option<Aabb>; 2] {
    let Some(block) = chunks.block_at(x, y, z) else {
        return [None; 2];
    };
    let origin = Vec3::new(x as f32, y as f32, z as f32);
    block
        .collision_boxes_for(chunks.metadata_at(x, y, z))
        .map(|bounds| {
            bounds.map(|(min, max)| {
                Aabb::new(
                    origin + Vec3::from_array(min),
                    origin + Vec3::from_array(max),
                )
            })
        })
}

/// Run Beta's living-entity ground movement for each emitted world tick.
/// `Velocity` remains expressed in blocks per second elsewhere in the game;
/// this system converts to and from Beta's blocks-per-tick motion values.
fn integrate_player(
    tick: Res<WorldTick>,
    time: Res<Time>,
    chunks: Res<WorldChunks>,
    mut block_ticks: Option<ResMut<BlockTicks>>,
    mut players: Query<
        (
            &mut Transform,
            &mut Velocity,
            &EntitySize,
            &mut CollisionState,
            &StepHeight,
            &mut PlayerMovementInput,
            &mut PlayerInterpolation,
            Option<&Flying>,
            Option<&mut StepDistance>,
            Option<&mut PlayerSurvival>,
            Option<&mut crate::player::portal::PortalTravel>,
        ),
        With<Player>,
    >,
) {
    const DEFAULT_AIR_DRAG: f32 = 0.91;
    const WALK_ACCELERATION: f32 = 0.1;
    const AIR_ACCELERATION: f32 = 0.02;
    const MOVE_FLYING_FRICTION: f32 = 0.162_771_36;
    const GRAVITY_PER_TICK: f32 = 0.08;
    const VERTICAL_DRAG: f32 = 0.98;
    const JUMP_IMPULSE: f32 = 0.419_999_99;

    for (
        mut transform,
        mut velocity,
        size,
        mut collision,
        step_height,
        input,
        mut interpolation,
        flying,
        mut steps,
        mut survival,
        mut portal,
    ) in &mut players
    {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }

        if flying.is_some() {
            // Noclip flight cannot tunnel, so it needs no step limit.
            transform.translation += velocity.0 * time.delta_secs().min(MAX_FRAME_SECS);
            interpolation.previous_position = transform.translation;
            *collision = CollisionState::default();
            if let Some(survival) = survival.as_deref_mut() {
                survival.clear_fall();
            }
            let mut unslowed = Vec3::ZERO;
            touch_blocks(
                size.aabb(transform.translation),
                &chunks,
                &mut unslowed,
                portal.as_deref_mut(),
            );
            continue;
        }
        if tick.ticks_this_frame() == 0 {
            continue;
        }

        // Bevy's camera looks along local -Z; Beta's yaw 0 points along +Z.
        let (bevy_yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let yaw = std::f32::consts::PI - bevy_yaw;
        let (sin_yaw, cos_yaw) = yaw.sin_cos();
        let mut motion = velocity.0 * TICK_SECONDS;

        for _ in 0..tick.ticks_this_frame() {
            interpolation.previous_position = transform.translation;
            let aabb = size.aabb(transform.translation);
            let (in_water, water_flow) = water_state(aabb, &chunks);
            let in_lava = !in_water && lava_contains(aabb, &chunks);
            if in_water {
                motion += water_flow * WATER_CURRENT_PER_TICK;
                if let Some(survival) = survival.as_deref_mut() {
                    survival.fall_distance = 0.0;
                }
            }

            if in_water || in_lava {
                const LIQUID_ACCELERATION: f32 = 0.02;
                const LIQUID_JUMP_ACCELERATION: f32 = 0.04;
                let old_y = transform.translation.y;
                accelerate_player(
                    &mut motion,
                    input.strafe,
                    input.forward,
                    sin_yaw,
                    cos_yaw,
                    LIQUID_ACCELERATION
                        * if input.sprinting {
                            crate::player::SPRINT_ACCELERATION_MULTIPLIER
                        } else {
                            1.0
                        },
                );
                if input.jumping {
                    motion.y += LIQUID_JUMP_ACCELERATION;
                }

                let sneaking_on_ground = collision.on_ground && input.sneaking;
                let movement = move_entity_with_sneak(
                    aabb,
                    motion,
                    step_height.0,
                    collision.on_ground,
                    input.sneaking,
                    &chunks,
                );
                transform.translation = size.position_from_aabb(movement.aabb);
                *collision = movement.collision;
                if let Some(survival) = survival.as_deref_mut() {
                    survival.update_fall(motion.y, movement.collision.on_ground);
                }
                cancel_collided_motion(&mut motion, movement.collision);
                step_on_block(
                    steps.as_deref_mut(),
                    block_ticks.as_deref_mut(),
                    &chunks,
                    movement,
                    sneaking_on_ground,
                );
                touch_blocks(movement.aabb, &chunks, &mut motion, portal.as_deref_mut());

                let drag = if in_water { 0.8 } else { 0.5 };
                motion *= drag;
                motion.y -= 0.02;

                if movement.collision.collided_x || movement.collision.collided_z {
                    let escape_offset = Vec3::new(
                        motion.x,
                        motion.y + 0.6 + old_y - transform.translation.y,
                        motion.z,
                    );
                    let escape_box = movement.aabb.offset(escape_offset);
                    if !collides(&chunks, escape_box) && !intersects_liquid(escape_box, &chunks) {
                        motion.y = 0.3;
                    }
                }
                continue;
            }

            let support = block_under_player(&chunks, aabb);
            let mut drag = DEFAULT_AIR_DRAG;
            if collision.on_ground {
                drag = support.map_or(0.6, |block| block.slipperiness()) * DEFAULT_AIR_DRAG;
            }

            let acceleration = if collision.on_ground {
                WALK_ACCELERATION * MOVE_FLYING_FRICTION / drag.powi(3)
            } else {
                AIR_ACCELERATION
            };
            accelerate_player(
                &mut motion,
                input.strafe,
                input.forward,
                sin_yaw,
                cos_yaw,
                acceleration
                    * if input.sprinting {
                        crate::player::SPRINT_ACCELERATION_MULTIPLIER
                    } else {
                        1.0
                    },
            );

            if input.jumping && collision.on_ground {
                motion.y = JUMP_IMPULSE;
            }

            let on_ladder = player_is_on_ladder(aabb, &chunks);
            if on_ladder {
                motion.x = motion.x.clamp(-0.15, 0.15);
                motion.z = motion.z.clamp(-0.15, 0.15);
                motion.y = motion.y.max(-0.15);
                if input.sneaking && motion.y < 0.0 {
                    motion.y = 0.0;
                }
                if let Some(survival) = survival.as_deref_mut() {
                    survival.fall_distance = 0.0;
                }
            }

            let sneaking_on_ground = collision.on_ground && input.sneaking;
            let step = web_slowed(aabb, &chunks, &mut motion);
            let movement = move_entity_with_sneak(
                aabb,
                step,
                step_height.0,
                collision.on_ground,
                input.sneaking,
                &chunks,
            );
            transform.translation = size.position_from_aabb(movement.aabb);
            *collision = movement.collision;
            if let Some(survival) = survival.as_deref_mut() {
                survival.update_fall(step.y, movement.collision.on_ground);
            }

            cancel_collided_motion(&mut motion, movement.collision);
            step_on_block(
                steps.as_deref_mut(),
                block_ticks.as_deref_mut(),
                &chunks,
                movement,
                sneaking_on_ground,
            );
            touch_blocks(movement.aabb, &chunks, &mut motion, portal.as_deref_mut());

            if (movement.collision.collided_x || movement.collision.collided_z)
                && player_is_on_ladder(movement.aabb, &chunks)
            {
                motion.y = 0.2;
            }

            motion.y = ((motion.y - GRAVITY_PER_TICK) * VERTICAL_DRAG)
                .max(-TERMINAL_VELOCITY * TICK_SECONDS);
            // Beta works its friction out before `moveEntity`, so the tick
            // that leaves the ground is still slowed by the block it left.
            motion.x *= drag;
            motion.z *= drag;
        }

        velocity.0 = motion / TICK_SECONDS;
    }
}

/// The end of `Entity.moveEntity`: every block the box overlaps, shrunk by a
/// thousandth, gets `onEntityCollidedWithBlock`. Soul sand, whose collision
/// box stops an eighth short so feet sink into its cell, slows the player
/// (`BlockSoulSand`), and a portal starts charging (`BlockPortal`).
fn touch_blocks(
    aabb: Aabb,
    chunks: &WorldChunks,
    motion: &mut Vec3,
    mut portal: Option<&mut crate::player::portal::PortalTravel>,
) {
    let low = (aabb.min + Vec3::splat(0.001)).floor().as_ivec3();
    let high = (aabb.max - Vec3::splat(0.001)).floor().as_ivec3();
    for x in low.x..=high.x {
        for y in low.y..=high.y {
            for z in low.z..=high.z {
                match chunks.block_at(x, y, z) {
                    Some(crate::block::blocks::Block::SoulSand) => {
                        motion.x *= 0.4;
                        motion.z *= 0.4;
                    }
                    Some(crate::block::blocks::Block::NetherPortal) => {
                        if let Some(portal) = portal.as_deref_mut() {
                            portal.set_in_portal();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// `Entity.moveEntity`'s step tracking: a finished step calls
/// `onEntityWalking` on the block 0.2 below the feet.
pub(crate) fn step_on_block(
    steps: Option<&mut StepDistance>,
    block_ticks: Option<&mut BlockTicks>,
    chunks: &WorldChunks,
    movement: Movement,
    sneaking_on_ground: bool,
) {
    let Some(steps) = steps else {
        return;
    };
    let aabb = movement.aabb;
    let underfoot = IVec3::new(
        ((aabb.min.x + aabb.max.x) * 0.5).floor() as i32,
        (aabb.min.y - 0.2).floor() as i32,
        ((aabb.min.z + aabb.max.z) * 0.5).floor() as i32,
    );
    let block = chunks
        .block_at(underfoot.x, underfoot.y, underfoot.z)
        .unwrap_or(crate::block::blocks::Block::Air);
    let stepped = steps.advance(
        movement.displacement,
        sneaking_on_ground,
        block == crate::block::blocks::Block::Air,
    );
    if stepped && let Some(block_ticks) = block_ticks {
        block_ticks.push_event(BlockEvent::Walked {
            position: underfoot,
        });
    }
}

fn player_is_on_ladder(aabb: Aabb, chunks: &WorldChunks) -> bool {
    let x = ((aabb.min.x + aabb.max.x) * 0.5).floor() as i32;
    let y = aabb.min.y.floor() as i32;
    let z = ((aabb.min.z + aabb.max.z) * 0.5).floor() as i32;
    chunks
        .block_at(x, y, z)
        .is_some_and(|block| block.is_ladder())
}

fn block_under_player(chunks: &WorldChunks, aabb: Aabb) -> Option<crate::block::blocks::Block> {
    let x = ((aabb.min.x + aabb.max.x) * 0.5).floor() as i32;
    let y = aabb.min.y.floor() as i32 - 1;
    let z = ((aabb.min.z + aabb.max.z) * 0.5).floor() as i32;
    chunks.block_at(x, y, z)
}

fn accelerate_player(
    motion: &mut Vec3,
    strafe: f32,
    forward: f32,
    sin_yaw: f32,
    cos_yaw: f32,
    acceleration: f32,
) {
    let strafe = strafe * 0.98;
    let forward = forward * 0.98;
    let magnitude = strafe.hypot(forward);
    if magnitude < 0.01 {
        return;
    }
    let scale = acceleration / magnitude.max(1.0);
    let strafe = strafe * scale;
    let forward = forward * scale;
    motion.x += strafe * cos_yaw - forward * sin_yaw;
    motion.z += forward * cos_yaw + strafe * sin_yaw;
}

fn cancel_collided_motion(motion: &mut Vec3, collision: CollisionState) {
    if collision.collided_x {
        motion.x = 0.0;
    }
    if collision.collided_y {
        motion.y = 0.0;
    }
    if collision.collided_z {
        motion.z = 0.0;
    }
}

/// Water immersion over the player's trimmed central body band, as Beta's
/// `Entity.handleWaterMovement` ([`water_movement`]), so shallow flowing water
/// under the band still counts. Other physical entities use their entire AABB
/// in [`water_current`].
fn water_state(aabb: Aabb, chunks: &WorldChunks) -> (bool, Vec3) {
    let band = Aabb::new(
        aabb.min + Vec3::new(0.001, 0.401, 0.001),
        aabb.max - Vec3::new(0.001, 0.401, 0.001),
    );
    water_movement(band, chunks)
}

/// `World.handleMaterialAcceleration` for water: whether the AABB reaches a
/// water surface and the normalized sum of its cells' flow directions. Pass
/// the player's trimmed band or a non-player entity's full collision box.
pub fn water_current(area: Aabb, chunks: &WorldChunks) -> (bool, Vec3) {
    let (min_x, max_x, min_y, max_y, min_z, max_z) = block_range(area);
    let mut immersed = false;
    let mut flow = Vec3::ZERO;

    for x in min_x..max_x {
        for y in min_y..max_y {
            for z in min_z..max_z {
                if !chunks.block_at(x, y, z).is_some_and(is_water) {
                    continue;
                }
                let surface = liquid_surface_y(chunks.metadata_at(x, y, z), y);
                // A body entirely *below* the surface is submerged too;
                // comparing its top against the surface would miss items.
                if area.min.y >= surface || area.max.y <= y as f32 {
                    continue;
                }
                immersed = true;
                flow += water_flow_vector(x, y, z, chunks);
            }
        }
    }

    (immersed, flow.normalize_or_zero())
}

/// `World.handleMaterialAcceleration` for water, exactly as
/// `Entity.handleWaterMovement` and `EntitySquid.isInWater` call it. Beta
/// compares each water surface with the top of the scanned cell range rather
/// than with the box, so any water in the cells `area` spans counts. A box
/// shrunk past zero height can span no cells at all.
pub fn water_movement(area: Aabb, chunks: &WorldChunks) -> (bool, Vec3) {
    let (min_x, max_x, min_y, max_y, min_z, max_z) = block_range(area);
    let mut immersed = false;
    let mut flow = Vec3::ZERO;
    for x in min_x..max_x {
        for y in min_y..max_y {
            for z in min_z..max_z {
                if chunks.block_at(x, y, z).is_some_and(is_water) {
                    immersed = true;
                    flow += water_flow_vector(x, y, z, chunks);
                }
            }
        }
    }
    (immersed, flow.normalize_or_zero())
}

/// `BlockFluid.getFlowVector` for water, normalized.
fn water_flow_vector(x: i32, y: i32, z: i32, chunks: &WorldChunks) -> Vec3 {
    let [flow_x, flow_z] = flow_vector(Fluid::Water, x, y, z, |x, y, z| {
        (
            chunks
                .block_at(x, y, z)
                .unwrap_or(crate::block::blocks::Block::Air),
            chunks.metadata_at(x, y, z),
        )
    });
    Vec3::new(flow_x, 0.0, flow_z).normalize_or_zero()
}

/// The top of the fluid in cell `y`, from its level.
fn liquid_surface_y(metadata: u8, y: i32) -> f32 {
    y as f32 + 1.0 - percent_air(metadata)
}

/// `Entity.handleLavaMovement`: lava within the box, inset 0.1 at the sides
/// and 0.4 at the top and bottom.
pub fn lava_contains(aabb: Aabb, chunks: &WorldChunks) -> bool {
    let area = Aabb::new(
        aabb.min + Vec3::new(0.1, 0.4, 0.1),
        aabb.max - Vec3::new(0.1, 0.4, 0.1),
    );
    contains_liquid_material(area, chunks, is_lava)
}

/// `Entity.isInsideOfMaterial(Material.water)`: `eye` is below the surface of
/// the water in its cell.
pub fn eye_in_water(eye: Vec3, chunks: &WorldChunks) -> bool {
    let (x, y, z) = (
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    );
    chunks.block_at(x, y, z).is_some_and(is_water) && {
        let surface = (y + 1) as f32 - (percent_air(chunks.metadata_at(x, y, z)) - 0.111_111_11);
        eye.y < surface
    }
}

/// `Entity.isEntityInsideOpaqueBlock`: eight points around the eyes of a body
/// `width` wide.
pub fn inside_opaque_block(eye: Vec3, width: f32, chunks: &WorldChunks) -> bool {
    (0..8).any(|i| {
        let dx = ((i & 1) as f32 - 0.5) * width * 0.9;
        let dy = (((i >> 1) & 1) as f32 - 0.5) * 0.1;
        let dz = (((i >> 2) & 1) as f32 - 0.5) * width * 0.9;
        let at = eye + Vec3::new(dx, dy, dz);
        chunks
            .block_at(
                at.x.floor() as i32,
                at.y.floor() as i32,
                at.z.floor() as i32,
            )
            .is_some_and(crate::block::blocks::Block::is_opaque_cube)
    })
}

/// `World.isBoundingBoxBurning`: fire or lava in any cell the box spans.
pub fn burning_in(area: Aabb, chunks: &WorldChunks) -> bool {
    use crate::block::blocks::Block;
    let min = area.min.floor().as_ivec3();
    let max = (area.max + Vec3::ONE).floor().as_ivec3();
    (min.x..max.x).any(|x| {
        (min.y..max.y).any(|y| {
            (min.z..max.z).any(|z| {
                matches!(
                    chunks.block_at(x, y, z),
                    Some(Block::Fire | Block::Lava | Block::FlowingLava)
                )
            })
        })
    })
}

/// The `onEntityCollidedWithBlock` loop of `Entity.moveEntity` for
/// `BlockCactus`: a cactus in any cell the body, inset 0.001, reaches into.
/// A cactus's collision box is a sixteenth short of its cell, which is what
/// lets a body pressed against one overlap it.
pub fn touches_cactus(aabb: Aabb, chunks: &WorldChunks) -> bool {
    let min = (aabb.min + Vec3::splat(0.001)).floor().as_ivec3();
    let max = (aabb.max - Vec3::splat(0.001)).floor().as_ivec3();
    (min.x..=max.x).any(|x| {
        (min.y..=max.y).any(|y| {
            (min.z..=max.z)
                .any(|z| chunks.block_at(x, y, z) == Some(crate::block::blocks::Block::Cactus))
        })
    })
}

/// `World.getIsAnyLiquid`.
pub fn intersects_liquid(aabb: Aabb, chunks: &WorldChunks) -> bool {
    contains_liquid_material(aabb, chunks, |block| is_water(block) || is_lava(block))
}

fn contains_liquid_material(
    area: Aabb,
    chunks: &WorldChunks,
    matches: impl Fn(crate::block::blocks::Block) -> bool,
) -> bool {
    let (min_x, max_x, min_y, max_y, min_z, max_z) = block_range(area);
    for x in min_x..max_x {
        for y in min_y..max_y {
            for z in min_z..max_z {
                if chunks.block_at(x, y, z).is_some_and(&matches) {
                    return true;
                }
            }
        }
    }
    false
}

/// `BlockWeb.onEntityCollidedWithBlock` and the `isInWeb` branch of
/// `Entity.moveEntity`: a body touching a cobweb moves a fraction of its
/// motion this tick and loses the motion itself. Returns the displacement to
/// sweep.
pub fn web_slowed(aabb: Aabb, chunks: &WorldChunks, motion: &mut Vec3) -> Vec3 {
    use crate::block::blocks::Block;
    let inner = Aabb::new(aabb.min + Vec3::splat(0.001), aabb.max - Vec3::splat(0.001));
    let (min_x, max_x, min_y, max_y, min_z, max_z) = block_range(inner);
    let in_web = (min_x..max_x).any(|x| {
        (min_y..max_y)
            .any(|y| (min_z..max_z).any(|z| chunks.block_at(x, y, z) == Some(Block::Cobweb)))
    });
    if !in_web {
        return *motion;
    }
    let step = *motion * Vec3::new(0.25, 0.05, 0.25);
    *motion = Vec3::ZERO;
    step
}

fn block_range(area: Aabb) -> (i32, i32, i32, i32, i32, i32) {
    (
        area.min.x.floor() as i32,
        area.max.x.floor() as i32 + 1,
        area.min.y.floor() as i32,
        area.max.y.floor() as i32 + 1,
        area.min.z.floor() as i32,
        area.max.z.floor() as i32 + 1,
    )
}

fn integrate_bodies(
    time: Res<Time>,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    mut bodies: Query<
        (
            &mut Transform,
            &mut Velocity,
            &EntitySize,
            &mut CollisionState,
            Option<&StepHeight>,
            Option<&Gravity>,
            Option<&Flying>,
        ),
        (Without<DroppedItem>, Without<Player>, Without<Living>),
    >,
) {
    let frame = time.delta_secs().min(MAX_FRAME_SECS);
    if frame <= 0.0 {
        return;
    }
    // A slow frame takes several short steps instead of running slow.
    let steps = (frame / MAX_STEP_SECS).ceil().max(1.0);
    let dt = frame / steps;

    for (mut transform, mut velocity, size, mut collision, step_height, gravity, flying) in
        &mut bodies
    {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }

        // Flying: noclip, no gravity, no collision resolution
        if flying.is_some() {
            transform.translation += velocity.0 * frame;
            *collision = CollisionState::default();
            continue;
        }

        // These bodies keep blocks/second. Convert Beta's per-tick current to
        // that unit, once per emitted world tick.
        if tick.ticks_this_frame() > 0 {
            velocity.0 += water_current(size.aabb(transform.translation), &chunks).1
                * (WATER_CURRENT_PER_TICK * tick.ticks_this_frame() as f32 / TICK_SECONDS);
        }

        for _ in 0..steps as u32 {
            if let Some(gravity) = gravity {
                velocity.0.y -= gravity.0 * dt;
                velocity.0.y = velocity.0.y.max(-TERMINAL_VELOCITY);
            }

            let movement = move_entity(
                size.aabb(transform.translation),
                velocity.0 * dt,
                step_height.map(|step| step.0).unwrap_or(0.0),
                collision.on_ground,
                &chunks,
            );
            transform.translation = size.position_from_aabb(movement.aabb);
            *collision = movement.collision;
            if collision.collided_x {
                velocity.0.x = 0.0;
            }
            if collision.collided_y {
                velocity.0.y = 0.0;
            }
            if collision.collided_z {
                velocity.0.z = 0.0;
            }
        }
    }
}

//! Voxel AABB collision, matching Beta 1.7.3 `Entity.moveEntity`.
//!
//! Bodies are Bevy entities with [`Velocity`] and [`EntitySize`]. The player is
//! the first user; later mobs reuse the same move.

mod raycast;

pub use raycast::BLOCK_REACH;
pub use raycast::BlockFace;
pub use raycast::BlockHit;
pub use raycast::raycast_blocks;

use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::Flying;
use crate::entity::Gravity;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::world::block::properties::blocks_movement;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;

/// Fall speed cap, from Beta's `(v - 0.08) * 0.98` terminal velocity.
const TERMINAL_VELOCITY: f32 = 78.4;
/// Clamp a lagged frame so a body cannot tunnel through more than this many
/// seconds of motion at once.
const MAX_STEP_SECS: f32 = 0.05;
/// Covers the f32 error from converting a player's feet to eye height and back.
const CONTACT_EPSILON: f32 = 1e-4;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhysicsSet {
    ApplyInput,
    Integrate,
}

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, PhysicsSet::Integrate.after(PhysicsSet::ApplyInput))
            .add_systems(
                Update,
                integrate_bodies
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
    mut aabb: Aabb,
    mut delta: Vec3,
    step_height: f32,
    was_on_ground: bool,
    chunks: &WorldChunks,
) -> Movement {
    let original = delta;
    let before = aabb;
    let colliders = colliding_aabbs(chunks, aabb.expand(delta));

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

    let blocked_horizontally = original.x != delta.x || original.z != delta.z;
    let landed = original.y != delta.y && original.y < 0.0;
    if step_height > 0.0 && (was_on_ground || landed) && blocked_horizontally {
        let stepped = try_step(before, original, step_height, chunks);
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
            collided_x: original.x != delta.x,
            collided_y: original.y != delta.y,
            collided_z: original.z != delta.z,
        },
    }
}

fn try_step(start: Aabb, original: Vec3, step_height: f32, chunks: &WorldChunks) -> Movement {
    let mut aabb = start;
    let mut delta = Vec3::new(original.x, step_height, original.z);
    let colliders = colliding_aabbs(chunks, aabb.expand(delta));

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

    let mut down = -step_height;
    for collider in &colliders {
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
    let min_x = area.min.x.floor() as i32;
    let max_x = (area.max.x + 1.0).floor() as i32;
    let min_y = area.min.y.floor() as i32;
    let max_y = (area.max.y + 1.0).floor() as i32;
    let min_z = area.min.z.floor() as i32;
    let max_z = (area.max.z + 1.0).floor() as i32;

    let mut boxes = Vec::new();
    for x in min_x..max_x {
        for z in min_z..max_z {
            for y in (min_y - 1)..max_y {
                if y >= CHUNK_HEIGHT as i32 {
                    continue;
                }
                let solid = if y < 0 {
                    true
                } else {
                    chunks.block_at(x, y, z).is_some_and(blocks_movement)
                };
                if !solid {
                    continue;
                }
                let block = Aabb::from_block(x, y, z);
                if area.intersects(block) {
                    boxes.push(block);
                }
            }
        }
    }
    boxes
}

fn integrate_bodies(
    time: Res<Time>,
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
        Without<DroppedItem>,
    >,
) {
    let dt = time.delta_secs().min(MAX_STEP_SECS);
    if dt <= 0.0 {
        return;
    }

    for (mut transform, mut velocity, size, mut collision, step_height, gravity, flying) in
        &mut bodies
    {
        if !chunks.contains(ChunkPos::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }

        // Flying: noclip, no gravity, no collision resolution
        if flying.is_some() {
            transform.translation += velocity.0 * dt;
            *collision = CollisionState::default();
            continue;
        }

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

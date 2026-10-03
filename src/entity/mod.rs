//! Independently simulated objects: mobs, items, and the player.
//!
//! These are Bevy entities. `Transform.translation` is the Minecraft-style
//! position (eyes for the player, because of [`EntitySize::y_offset`]).

use bevy::prelude::*;

use crate::item::ItemStack;

pub mod combat;
pub mod creature;
pub mod drops;
pub mod explosion;
pub mod falling_block;
pub mod mobs;
pub mod particles;
pub mod pathfinding;
pub mod projectiles;
pub mod shadow;

/// An independently simulated inventory stack lying in the world.
#[derive(Component, Clone, Copy, Debug)]
pub struct DroppedItem(pub ItemStack);

/// Linear velocity in blocks per second.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct Velocity(pub Vec3);

/// `Transform.translation` at the start of the current tick. Rendering lerps
/// from here to the current translation with [`WorldTick::partial`], Beta's
/// `lastTickPosX/Y/Z` interpolation for anything simulated slower than the
/// frame rate (the item mesh, and now its shadow).
///
/// [`WorldTick::partial`]: crate::world::tick::WorldTick::partial
#[derive(Component, Clone, Copy, Debug)]
pub struct PreviousTick(pub Vec3);

/// Width, height, and the Y offset from [`Transform`] down to the feet.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EntitySize {
    pub width: f32,
    pub height: f32,
    /// Subtracted from `Transform.translation.y` to get the AABB floor.
    /// Player uses `1.62` so the camera can live on the same entity.
    pub y_offset: f32,
}

impl EntitySize {
    /// Beta player: 0.6×1.8, eyes 1.62 above the feet.
    pub const PLAYER: Self = Self {
        width: 0.6,
        height: 1.8,
        y_offset: 1.62,
    };

    /// Beta `EntityItem`: a 0.25 cube whose transform is the center
    /// (`yOffset = height / 2`).
    pub const DROPPED_ITEM: Self = Self {
        width: 0.25,
        height: 0.25,
        y_offset: 0.125,
    };
}

impl Default for EntitySize {
    fn default() -> Self {
        Self {
            width: 0.6,
            height: 1.8,
            y_offset: 0.0,
        }
    }
}

/// Result of the last voxel move: grounded and which axes were clipped.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct CollisionState {
    pub on_ground: bool,
    pub collided_x: bool,
    pub collided_y: bool,
    pub collided_z: bool,
}

/// How tall a step this body can walk up, in blocks. The player uses `0.5`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StepHeight(pub f32);

impl StepHeight {
    pub const PLAYER: Self = Self(0.5);
}

impl Default for StepHeight {
    fn default() -> Self {
        Self(0.0)
    }
}

/// Downward acceleration in blocks per second squared.
#[derive(Component, Clone, Copy, Debug)]
pub struct Gravity(pub f32);

impl Gravity {
    /// Close to Beta's discrete `(v - 0.08) * 0.98` per tick.
    pub const DEFAULT: Self = Self(32.0);
}

impl Default for Gravity {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Beta `Entity.distanceWalkedModified` and `nextStepDistance`. Every whole
/// block walked is a step onto the block underfoot, which runs that block's
/// `onEntityWalking` (trampling farmland, lighting redstone ore).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StepDistance {
    pub walked: f32,
    pub next_step: u32,
}

impl Default for StepDistance {
    fn default() -> Self {
        Self {
            walked: 0.0,
            next_step: 1,
        }
    }
}

impl StepDistance {
    /// Add one move's horizontal displacement, as `Entity.moveEntity` does
    /// unless the entity is sneaking on the ground. Returns whether the move
    /// finished a step onto a non-air block.
    pub fn advance(
        &mut self,
        displacement: Vec3,
        sneaking_on_ground: bool,
        underfoot_is_air: bool,
    ) -> bool {
        if sneaking_on_ground {
            return false;
        }
        self.walked += displacement.x.hypot(displacement.z) * 0.6;
        if self.walked > self.next_step as f32 && !underfoot_is_air {
            self.next_step += 1;
            return true;
        }
        false
    }
}

/// Marker for entities that are flying (no gravity, noclip movement).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Flying;

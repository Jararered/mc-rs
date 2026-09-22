//! Independently simulated objects: the player now, later mobs and items.
//!
//! These are Bevy entities. `Transform.translation` is the Minecraft-style
//! position (eyes for the player, because of [`EntitySize::y_offset`]).

use bevy::prelude::*;

use crate::item::ItemStack;

pub mod block_drops;
pub mod dropped_items;
pub mod particle_registry;
pub mod particles;

/// An independently simulated inventory stack lying in the world.
#[derive(Component, Clone, Copy, Debug)]
pub struct DroppedItem(pub ItemStack);

/// Linear velocity in blocks per second.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct Velocity(pub Vec3);

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

/// Marker for entities that are flying (no gravity, noclip movement).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Flying;

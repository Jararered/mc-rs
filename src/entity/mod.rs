//! Independently simulated objects: mobs, items, and the player.
//!
//! These are Bevy entities. `Transform.translation` is the Minecraft-style
//! position (eyes for the player, because of [`EntitySize::y_offset`]).

use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::block::blocks::Block;
use crate::entity::explosion::PrimedTnt;
use crate::entity::explosion::prime_tnt;
use crate::entity::falling_block::FallingBlock;
use crate::entity::falling_block::spawn_falling;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::CartKind;
use crate::entity::minecart::Minecart;
use crate::entity::pathfinding::SearchStats;
use crate::item::ItemStack;
use crate::world::streaming::TimingStats;

pub mod combat;
pub mod creature;
pub mod drops;
pub mod explosion;
pub mod falling_block;
pub mod fishing;
pub mod minecart;
pub mod mobs;
pub mod mount;
pub mod pathfinding;
pub mod projectiles;
pub mod thrown;

/// Mob simulation and rendering costs collected since the last performance
/// print.
#[derive(Resource, Default)]
pub struct EntityDiagnostics {
    /// `tick_creatures`, once per frame that runs world ticks.
    pub creatures: TimingStats,
    /// World ticks those frames ran.
    pub ticks: u64,
    pub spawning: TimingStats,
    pub posing: TimingStats,
    pub searches: SearchStats,
    /// Mobs and model boxes at the last sample.
    pub mobs: usize,
    pub parts: usize,
}

impl EntityDiagnostics {
    pub fn take(&mut self) -> Self {
        Self {
            mobs: self.mobs,
            parts: self.parts,
            ..std::mem::take(self)
        }
    }
}

/// A falling block or primed TNT stored with the chunk it is in. Both have
/// already taken their block out of the world, so losing the entity would
/// lose the block. Arrows, fireballs, thrown snowballs and eggs, and fishing
/// bobbers are not saved, so an arrow left stuck in a wall is gone on reload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SavedBody {
    FallingBlock {
        /// The raw block id.
        block: u8,
        center: [f32; 3],
        motion: [f32; 3],
        fall_ticks: u32,
        on_ground: bool,
    },
    PrimedTnt {
        feet: [f32; 3],
        velocity: [f32; 3],
        fuse: u16,
    },
    /// A minecart on or off its rail, with its speed in blocks per tick.
    Minecart {
        center: [f32; 3],
        motion: [f32; 3],
        /// Absent in saves from before chest and furnace carts.
        #[serde(default)]
        kind: CartKind,
        #[serde(default)]
        fuel: i32,
        #[serde(default)]
        push: [f32; 2],
        /// The occupied slots of a chest cart.
        #[serde(default)]
        cargo: Vec<SavedSlot>,
    },
}

/// One occupied slot of a chest cart's cargo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedSlot {
    pub slot: u8,
    pub item: u16,
    pub count: u8,
    #[serde(default)]
    pub data: u16,
}

impl SavedSlot {
    pub fn pack(cargo: &Cargo) -> Vec<Self> {
        cargo
            .0
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| {
                let stack = (*stack)?;
                Some(Self {
                    slot: slot as u8,
                    item: stack.item().as_u16(),
                    count: stack.count(),
                    data: stack.data(),
                })
            })
            .collect()
    }

    pub fn unpack(slots: &[Self]) -> Cargo {
        let mut cargo = Cargo::default();
        for saved in slots {
            if let Some(item) = crate::item::Item::from_u16(saved.item)
                && let Ok(stack) = ItemStack::with_data(item, saved.count, saved.data)
                && let Some(slot) = cargo.0.get_mut(usize::from(saved.slot))
            {
                *slot = Some(stack);
            }
        }
        cargo
    }
}

/// The components [`SavedBody::capture`] reads, for a query filtered by
/// [`SavedBodyFilter`].
pub type SavedBodyData = (
    Entity,
    &'static Transform,
    Option<&'static FallingBlock>,
    Option<&'static PrimedTnt>,
    Option<&'static Velocity>,
    Option<&'static Minecart>,
    Option<&'static Cargo>,
);
pub type SavedBodyFilter = Or<(With<FallingBlock>, With<PrimedTnt>, With<Minecart>)>;

impl SavedBody {
    pub fn capture(
        transform: &Transform,
        falling: Option<&FallingBlock>,
        tnt: Option<&PrimedTnt>,
        velocity: Option<&Velocity>,
        minecart: Option<&Minecart>,
        cargo: Option<&Cargo>,
    ) -> Option<Self> {
        let position = transform.translation.to_array();
        if let Some(cart) = minecart {
            return Some(Self::Minecart {
                center: position,
                motion: cart.motion.to_array(),
                kind: cart.kind,
                fuel: cart.fuel,
                push: cart.push.to_array(),
                cargo: cargo.map(SavedSlot::pack).unwrap_or_default(),
            });
        }
        if let Some(falling) = falling {
            return Some(Self::FallingBlock {
                block: falling.block.as_u8(),
                center: position,
                motion: falling.motion.to_array(),
                fall_ticks: falling.fall_ticks,
                on_ground: falling.on_ground,
            });
        }
        tnt.map(|tnt| Self::PrimedTnt {
            feet: position,
            velocity: velocity
                .map_or(Vec3::ZERO, |velocity| velocity.0)
                .to_array(),
            fuse: tnt.fuse,
        })
    }

    pub fn spawn(self, commands: &mut Commands) {
        match self {
            Self::FallingBlock {
                block,
                center,
                motion,
                fall_ticks,
                on_ground,
            } => spawn_falling(
                commands,
                Vec3::from_array(center),
                FallingBlock {
                    block: Block::from(block),
                    fall_ticks,
                    motion: Vec3::from_array(motion),
                    on_ground,
                },
            ),
            Self::PrimedTnt {
                feet,
                velocity,
                fuse,
            } => {
                let entity = prime_tnt(commands, Vec3::from_array(feet), fuse);
                commands
                    .entity(entity)
                    .insert(Velocity(Vec3::from_array(velocity)));
            }
            Self::Minecart {
                center,
                motion,
                kind,
                fuel,
                push,
                cargo,
            } => {
                minecart::spawn_cart_at(
                    commands,
                    Vec3::from_array(center),
                    Minecart {
                        kind,
                        motion: Vec3::from_array(motion),
                        fuel,
                        push: Vec2::from_array(push),
                        ..Minecart::default()
                    },
                    (kind == CartKind::Chest).then(|| SavedSlot::unpack(&cargo)),
                );
            }
        }
    }
}

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

/// `Render.shadowSize`/`field_194_c` for one entity type: the blob's radius
/// in blocks, and a maximum opacity multiplier applied on top of distance
/// fade.
#[derive(Component, Clone, Copy, Debug)]
pub struct Shadow {
    pub radius: f32,
    pub opacity_scale: f32,
}

impl Shadow {
    /// `RenderItem`.
    pub const DROPPED_ITEM: Self = Self {
        radius: 0.15,
        opacity_scale: 0.75,
    };
}

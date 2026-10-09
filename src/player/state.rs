//! The player's components: what the simulation, HUD, and saves read.

use super::portal;
use super::sleep;
use super::survival::PlayerSurvival;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Gravity;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::combat::PlayerCombat;
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

/// Full player health in half-hearts. Ten hearts on the HUD.
pub const MAX_PLAYER_HEALTH: u8 = 20;

#[derive(Component)]
#[require(
    Transform,
    Velocity,
    CollisionState,
    Gravity,
    EntitySize = EntitySize::PLAYER,
    StepHeight = StepHeight::PLAYER,
    StepDistance,
    FlySpeed,
    GameMode,
    PlayerMovementInput,
    PlayerInterpolation,
    PlayerCombat,
    PlayerSurvival,
    portal::PortalTravel,
    sleep::PlayerSleep
)]
pub struct Player;

/// What the world lets the player do, set with `/gamemode`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
    /// Walks, falls, and takes damage.
    #[default]
    Survival,
    /// Takes no damage and may fly, still colliding with blocks.
    Creative,
    /// Takes no damage and always flies, passing through blocks.
    Spectator,
}

impl GameMode {
    pub const ALL: [Self; 3] = [Self::Survival, Self::Creative, Self::Spectator];

    pub fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Spectator => "spectator",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(name))
    }

    /// Hazards, mobs, and explosions can hurt the player.
    pub fn takes_damage(self) -> bool {
        self == Self::Survival
    }

    /// The player may switch [`Flying`] on and off.
    pub fn toggles_flight(self) -> bool {
        self == Self::Creative
    }

    /// Movement ignores blocks.
    pub fn noclip(self) -> bool {
        self == Self::Spectator
    }
}

/// Frame-sampled controls consumed by the tick-based player physics system.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct PlayerMovementInput {
    pub strafe: f32,
    pub forward: f32,
    pub sneaking: bool,
    pub sprinting: bool,
    pub jumping: bool,
}

/// Previous fixed-tick position used to smooth the rendered first-person view.
#[derive(Component, Default, Clone, Copy, Debug)]
pub(crate) struct PlayerInterpolation {
    pub previous_position: Vec3,
}

pub(super) const MIN_FLY_SPEED: f32 = 0.25;
pub(super) const MAX_FLY_SPEED: f32 = 50.0;

/// How fast the player moves while flying. Multiplied by the base flying speed.
#[derive(Component, Clone, Copy, Debug)]
pub struct FlySpeed(pub f32);

impl Default for FlySpeed {
    fn default() -> Self {
        Self(1.0)
    }
}

impl FlySpeed {
    pub fn clamp_value(&mut self) {
        self.0 = self.0.clamp(MIN_FLY_SPEED, MAX_FLY_SPEED);
    }
}

/// Current health in half-hearts. Each HUD heart is two points.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerHealth {
    pub current: u8,
}

impl Default for PlayerHealth {
    fn default() -> Self {
        Self {
            current: MAX_PLAYER_HEALTH,
        }
    }
}

/// How a single HUD heart should be filled from current health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartFill {
    Empty,
    Half,
    Full,
}

impl PlayerHealth {
    /// `EntityPlayer.heal`: restore half-hearts up to the maximum.
    pub fn heal(&mut self, amount: u8) {
        self.current = self.current.saturating_add(amount).min(MAX_PLAYER_HEALTH);
    }

    pub fn heart_fill(self, index: usize) -> HeartFill {
        let health = self.current.min(MAX_PLAYER_HEALTH);
        let start = (index as u8).saturating_mul(2);
        if health >= start + 2 {
            HeartFill::Full
        } else if health == start + 1 {
            HeartFill::Half
        } else {
            HeartFill::Empty
        }
    }
}

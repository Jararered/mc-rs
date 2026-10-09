//! Survival difficulty: a rule of the world, chosen on the client.

use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

/// The original four survival difficulty levels. The client's own choice lives in `settings.json`; a
/// world records the one it is played at.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Peaceful,
    Easy,
    #[default]
    Normal,
    Hard,
}

impl Difficulty {
    pub fn cycle(self) -> Self {
        match self {
            Self::Peaceful => Self::Easy,
            Self::Easy => Self::Normal,
            Self::Normal => Self::Hard,
            Self::Hard => Self::Peaceful,
        }
    }

    /// EntityMob and arrow damage in EntityPlayer.attackEntityFrom.
    pub fn mob_damage(self, damage: u8) -> u8 {
        match self {
            Self::Peaceful => 0,
            Self::Easy => damage / 3 + 1,
            Self::Normal => damage,
            Self::Hard => damage.saturating_mul(3) / 2,
        }
    }
}

/// The player's own difficulty option while a loaded world's recorded
/// difficulty stands in for it in [`GameSettings`](crate::app::settings::GameSettings). `settings.json` keeps this
/// value, so playing a Peaceful world does not change the option for others.
#[derive(Resource, Default, Debug)]
pub struct ClientDifficulty(pub Option<Difficulty>);

//! Beta `AchievementList` / `StatFileWriter` tracking. Screens and popups are
//! not drawn yet; unlocking still records the id so riding hooks can fire.

use std::collections::HashSet;

use bevy::prelude::*;

/// Achievements the game can unlock. Ids match Beta's `AchievementList` where
/// this build implements the trigger.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Achievement {
    /// Fall more than five blocks while riding a pig (`flyPig`).
    FlyPig,
}

/// Request to unlock an achievement (from creature ticks and similar).
#[derive(Message, Clone, Copy, Debug)]
pub struct UnlockAchievement(pub Achievement);

/// Unlocked achievements for the local player. Not saved yet.
#[derive(Resource, Default, Debug)]
pub struct Achievements {
    unlocked: HashSet<Achievement>,
}

impl Achievements {
    /// Mark `id` unlocked. Returns true the first time it unlocks.
    pub fn unlock(&mut self, id: Achievement) -> bool {
        self.unlocked.insert(id)
    }

    pub fn has(&self, id: Achievement) -> bool {
        self.unlocked.contains(&id)
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Achievements>()
        .add_message::<UnlockAchievement>()
        .add_systems(Update, apply_unlocks);
}

fn apply_unlocks(
    mut achievements: ResMut<Achievements>,
    mut unlocks: MessageReader<UnlockAchievement>,
) {
    for UnlockAchievement(id) in unlocks.read() {
        achievements.unlock(*id);
    }
}

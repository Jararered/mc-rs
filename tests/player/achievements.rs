//! Achievement unlock scaffolding.

use bevy::prelude::*;
use game::player::Achievement;
use game::player::Achievements;
use game::player::UnlockAchievement;

#[test]
fn unlocking_fly_pig_is_idempotent() {
    let mut app = App::new();
    app.init_resource::<Achievements>()
        .add_message::<UnlockAchievement>()
        .add_systems(
            Update,
            |mut achievements: ResMut<Achievements>,
             mut unlocks: MessageReader<UnlockAchievement>| {
                for UnlockAchievement(id) in unlocks.read() {
                    achievements.unlock(*id);
                }
            },
        );

    app.world_mut()
        .write_message(UnlockAchievement(Achievement::FlyPig));
    app.update();
    assert!(
        app.world()
            .resource::<Achievements>()
            .has(Achievement::FlyPig)
    );

    app.world_mut()
        .write_message(UnlockAchievement(Achievement::FlyPig));
    app.update();
    assert!(
        app.world()
            .resource::<Achievements>()
            .has(Achievement::FlyPig)
    );
}

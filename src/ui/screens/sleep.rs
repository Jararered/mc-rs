//! The dark wash `GuiIngame` draws over the view while the player falls
//! asleep in a bed, and fades back out after they wake.

use bevy::picking::prelude::Pickable;
use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::player::Player;
use crate::player::sleep::PlayerSleep;

pub struct SleepUiPlugin;

impl Plugin for SleepUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::Playing), spawn)
            .add_systems(OnExit(AppScreen::Playing), despawn)
            .add_systems(Update, update_fade.run_if(in_state(AppScreen::Playing)));
    }
}

/// `220 / 255`, the wash's alpha once fully asleep.
const FULL_ALPHA: f32 = 220.0 / 255.0;

#[derive(Component)]
struct SleepFade;

/// Beta's `0x101020`.
fn fade_color(alpha: f32) -> Color {
    Color::srgba_u8(0x10, 0x10, 0x20, 255).with_alpha(alpha)
}

fn spawn(mut commands: Commands) {
    // Beta draws it after the hotbar and before the chat.
    commands.spawn((
        SleepFade,
        Pickable::IGNORE,
        GlobalZIndex(5),
        Visibility::Hidden,
        BackgroundColor(fade_color(0.0)),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
    ));
}

fn despawn(mut commands: Commands, fades: Query<Entity, With<SleepFade>>) {
    for fade in &fades {
        commands.entity(fade).despawn();
    }
}

fn update_fade(
    players: Query<&PlayerSleep, With<Player>>,
    mut fades: Query<(&mut BackgroundColor, &mut Visibility), With<SleepFade>>,
) {
    let strength = players.single().map_or(0.0, PlayerSleep::fade);
    for (mut color, mut visibility) in &mut fades {
        if strength <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        let target = fade_color(FULL_ALPHA * strength.clamp(0.0, 1.0));
        if color.0 != target {
            color.0 = target;
        }
    }
}

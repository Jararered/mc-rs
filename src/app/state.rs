use bevy::prelude::States;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppScreen {
    #[default]
    Menu,
    Settings,
    /// The saved worlds, between the title screen and the game.
    WorldSelect,
    NewWorld,
    Playing,
}

/// The in-game pause menu. It is a flag inside [`AppScreen::Playing`], like the
/// inventory, so the world keeps running behind it.
#[derive(bevy::prelude::Resource, Default, Debug)]
pub struct PauseMenu {
    pub open: bool,
}

/// The screen the settings screen returns to: the title menu, or the game when
/// it was opened from the pause menu.
#[derive(bevy::prelude::Resource, Debug, Clone, Copy)]
pub struct SettingsReturn(pub AppScreen);

impl Default for SettingsReturn {
    fn default() -> Self {
        Self(AppScreen::Menu)
    }
}

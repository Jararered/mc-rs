use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

pub const MIN_RENDER_DISTANCE: i32 = 4;
pub const MAX_RENDER_DISTANCE: i32 = 32;
pub const MIN_BRIGHTNESS: f32 = 0.0;
pub const MAX_BRIGHTNESS: f32 = 1000.0;
/// Without a directional sun, the 0..=1000 slider has to light the whole
/// scene. Bevy's default camera exposure makes 1000 nits of ambient look dim
/// next to the 10_000 lux sun, so ambient-only uses this extra scale.
pub const AMBIENT_ONLY_SCALE: f32 = 10.0;
pub const MIN_FOV: f32 = 30.0;
pub const MAX_FOV: f32 = 110.0;
pub const DEFAULT_FOV: f32 = 80.0;
pub const DEFAULT_WIGGLE_LEAVES: bool = true;

/// Client options file, relative to the working directory.
pub const SETTINGS_FILE: &str = "settings.json";

const FORMAT_VERSION: u32 = 1;

/// Player-facing graphics quality. Fast and Fancy match Beta leaves; Ultra keeps
/// Fancy leaves and switches water to Bevy screen-space reflections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GraphicsQuality {
    Fast,
    #[default]
    Fancy,
    Ultra,
}

impl GraphicsQuality {
    pub fn cycle(self) -> Self {
        match self {
            Self::Fast => Self::Fancy,
            Self::Fancy => Self::Ultra,
            Self::Ultra => Self::Fast,
        }
    }

    /// Cutout leaf tiles and unculled canopy faces. Fast is the only solid mode.
    pub fn fancy_leaves(self) -> bool {
        !matches!(self, Self::Fast)
    }

    /// Glossy water with screen-space reflections. Mesh positions stay the same.
    pub fn realistic_water(self) -> bool {
        matches!(self, Self::Ultra)
    }
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct GameSettings {
    pub render_distance: i32,
    pub brightness: f32,
    pub fov: f32,
    pub old_lighting: bool,
    pub smooth_lighting: bool,
    pub directional_lighting: bool,
    pub wiggle_leaves: bool,
    pub graphics: GraphicsQuality,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: MIN_RENDER_DISTANCE,
            brightness: 300.0,
            fov: DEFAULT_FOV,
            old_lighting: true,
            smooth_lighting: true,
            directional_lighting: true,
            wiggle_leaves: DEFAULT_WIGGLE_LEAVES,
            graphics: GraphicsQuality::Fancy,
        }
    }
}

impl GameSettings {
    pub fn change_render_distance(&mut self, change: i32) {
        self.render_distance =
            (self.render_distance + change).clamp(MIN_RENDER_DISTANCE, MAX_RENDER_DISTANCE);
    }

    pub fn change_brightness(&mut self, change: f32) {
        self.brightness = (self.brightness + change).clamp(MIN_BRIGHTNESS, MAX_BRIGHTNESS);
    }

    /// Value written to Bevy's [`GlobalAmbientLight`]. Directional lighting
    /// keeps the slider as nits; without it the same numbers are scaled so
    /// max brightness can actually light the world.
    pub fn ambient_light_brightness(&self) -> f32 {
        if self.directional_lighting {
            self.brightness
        } else {
            self.brightness * AMBIENT_ONLY_SCALE
        }
    }

    pub fn change_fov(&mut self, change: f32) {
        self.fov = (self.fov + change).clamp(MIN_FOV, MAX_FOV);
    }

    pub fn fov_radians(&self) -> f32 {
        self.fov.to_radians()
    }

    pub fn cycle_graphics(&mut self) {
        self.graphics = self.graphics.cycle();
    }

    /// Keep loaded or edited values inside the same ranges the menu uses.
    pub fn clamp(&mut self) {
        self.render_distance = self
            .render_distance
            .clamp(MIN_RENDER_DISTANCE, MAX_RENDER_DISTANCE);
        self.brightness = if self.brightness.is_finite() {
            self.brightness.clamp(MIN_BRIGHTNESS, MAX_BRIGHTNESS)
        } else {
            Self::default().brightness
        };
        self.fov = if self.fov.is_finite() {
            self.fov.clamp(MIN_FOV, MAX_FOV)
        } else {
            DEFAULT_FOV
        };
    }
}

/// Loads `settings.json` at startup and writes it whenever menu options change.
pub struct SettingsPlugin {
    path: PathBuf,
}

impl Default for SettingsPlugin {
    fn default() -> Self {
        Self {
            path: PathBuf::from(SETTINGS_FILE),
        }
    }
}

impl SettingsPlugin {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SettingsPath(self.path.clone()))
            .insert_resource(load_settings(&self.path))
            .add_systems(Last, save_settings_when_changed);
    }
}

#[derive(Resource)]
struct SettingsPath(PathBuf);

/// On-disk form of the settings menu. Extra JSON fields are ignored so older
/// clients can still read a newer file, and missing fields use defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    format_version: u32,
    render_distance: i32,
    brightness: f32,
    fov: f32,
    old_lighting: bool,
    smooth_lighting: bool,
    directional_lighting: bool,
    wiggle_leaves: bool,
    graphics: GraphicsQuality,
}

impl Default for StoredSettings {
    fn default() -> Self {
        Self::from(&GameSettings::default())
    }
}

impl From<&GameSettings> for StoredSettings {
    fn from(settings: &GameSettings) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            render_distance: settings.render_distance,
            brightness: settings.brightness,
            fov: settings.fov,
            old_lighting: settings.old_lighting,
            smooth_lighting: settings.smooth_lighting,
            directional_lighting: settings.directional_lighting,
            wiggle_leaves: settings.wiggle_leaves,
            graphics: settings.graphics,
        }
    }
}

impl From<StoredSettings> for GameSettings {
    fn from(stored: StoredSettings) -> Self {
        let mut settings = Self {
            render_distance: stored.render_distance,
            brightness: stored.brightness,
            fov: stored.fov,
            old_lighting: stored.old_lighting,
            smooth_lighting: stored.smooth_lighting,
            directional_lighting: stored.directional_lighting,
            wiggle_leaves: stored.wiggle_leaves,
            graphics: stored.graphics,
        };
        settings.clamp();
        settings
    }
}

/// Read `path`, or return defaults when the file is missing or unreadable.
pub fn load_settings(path: impl AsRef<Path>) -> GameSettings {
    let path = path.as_ref();
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<StoredSettings>(&bytes) {
            Ok(stored) => GameSettings::from(stored),
            Err(error) => {
                warn!("Ignoring unreadable settings {}: {error}", path.display());
                GameSettings::default()
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => GameSettings::default(),
        Err(error) => {
            warn!("Ignoring unreadable settings {}: {error}", path.display());
            GameSettings::default()
        }
    }
}

/// Write the current menu options as pretty-printed JSON.
pub fn save_settings(path: impl AsRef<Path>, settings: &GameSettings) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let stored = StoredSettings::from(settings);
    let bytes = serde_json::to_vec_pretty(&stored).map_err(io::Error::other)?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

fn save_settings_when_changed(settings: Res<GameSettings>, path: Res<SettingsPath>) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    if let Err(error) = save_settings(&path.0, &settings) {
        error!("Cannot save {}: {error}", path.0.display());
    }
}

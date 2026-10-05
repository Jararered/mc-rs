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
pub const MIN_MOUSE_SENSITIVITY: f32 = 0.1;
pub const MAX_MOUSE_SENSITIVITY: f32 = 3.0;
pub const DEFAULT_MAX_FPS: u32 = 60;
pub const MIN_MAX_FPS: u32 = 30;
pub const MAX_MAX_FPS: u32 = 240;
pub const DEFAULT_WIGGLE_LEAVES: bool = true;
/// Beta's `WorldProvider.getCloudHeight()`, which the client only used for the
/// cloud sheet. The range reaches above the 128-block world so the clouds can
/// clear the tallest terrain.
pub const MIN_CLOUD_HEIGHT: f32 = 16.0;
pub const MAX_CLOUD_HEIGHT: f32 = 256.0;
/// `WorldProvider.getCloudHeight()` for the sky provider.
pub const DEFAULT_CLOUD_HEIGHT: f32 = 144.0;

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

    /// Beta's `doRenderShadowAndFire` draws entity shadows only under
    /// `gameSettings.fancyGraphics`, the same flag Fast disables here.
    pub fn entity_shadows(self) -> bool {
        !matches!(self, Self::Fast)
    }

    /// Glossy water with screen-space reflections. Mesh positions stay the same.
    pub fn realistic_water(self) -> bool {
        matches!(self, Self::Ultra)
    }
}

/// The original four survival difficulty levels. Client selection lives in settings.json.
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
/// difficulty stands in for it in [`GameSettings`]. `settings.json` keeps this
/// value, so playing a Peaceful world does not change the option for others.
#[derive(Resource, Default, Debug)]
pub struct ClientDifficulty(pub Option<Difficulty>);

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct GameSettings {
    pub render_distance: i32,
    pub difficulty: Difficulty,
    /// Zero follows VSync without an additional application frame cap.
    pub max_fps: u32,
    pub brightness: f32,
    pub fov: f32,
    pub cloud_height: f32,
    pub old_lighting: bool,
    pub smooth_lighting: bool,
    pub directional_lighting: bool,
    pub wiggle_leaves: bool,
    pub graphics: GraphicsQuality,
    /// 4x MSAA. Beta had none; off drops the window-sized multisampled
    /// colour and depth targets, the largest textures the renderer holds.
    pub anti_aliasing: bool,
    pub mouse_sensitivity: f32,
    pub view_bobbing: bool,
    pub fullscreen: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: MIN_RENDER_DISTANCE,
            difficulty: Difficulty::Normal,
            max_fps: DEFAULT_MAX_FPS,
            brightness: 300.0,
            fov: DEFAULT_FOV,
            cloud_height: DEFAULT_CLOUD_HEIGHT,
            old_lighting: true,
            smooth_lighting: true,
            directional_lighting: true,
            wiggle_leaves: DEFAULT_WIGGLE_LEAVES,
            graphics: GraphicsQuality::Fancy,
            anti_aliasing: true,
            mouse_sensitivity: 1.0,
            view_bobbing: true,
            fullscreen: false,
        }
    }
}

impl GameSettings {
    pub fn change_mouse_sensitivity(&mut self, change: f32) {
        self.mouse_sensitivity =
            (self.mouse_sensitivity + change).clamp(MIN_MOUSE_SENSITIVITY, MAX_MOUSE_SENSITIVITY);
    }

    pub fn cycle_max_fps(&mut self) {
        self.max_fps = match self.max_fps {
            0..30 => 30,
            30..60 => 60,
            60..90 => 90,
            90..120 => 120,
            120..144 => 144,
            144..240 => 240,
            _ => 0,
        };
    }

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

    /// Old lighting draws every material unlit, so nothing would sample a
    /// shadow map; rendering one would only redraw the terrain for no effect.
    pub fn sun_shadows(&self) -> bool {
        self.directional_lighting && !self.old_lighting
    }

    /// Sample count for every camera. Ultra's screen-space reflections run on
    /// the deferred path, which cannot be multisampled.
    pub fn msaa(&self) -> Msaa {
        if self.anti_aliasing && !self.graphics.realistic_water() {
            Msaa::Sample4
        } else {
            Msaa::Off
        }
    }

    pub fn change_fov(&mut self, change: f32) {
        self.fov = (self.fov + change).clamp(MIN_FOV, MAX_FOV);
    }

    pub fn fov_radians(&self) -> f32 {
        self.fov.to_radians()
    }

    /// Beta fixes the cloud sheet in `WorldProvider`; here the menu moves it.
    pub fn change_cloud_height(&mut self, change: f32) {
        self.cloud_height = (self.cloud_height + change).clamp(MIN_CLOUD_HEIGHT, MAX_CLOUD_HEIGHT);
    }

    pub fn cycle_graphics(&mut self) {
        self.graphics = self.graphics.cycle();
    }

    /// Keep loaded or edited values inside the same ranges the menu uses.
    pub fn clamp(&mut self) {
        self.mouse_sensitivity = if self.mouse_sensitivity.is_finite() {
            self.mouse_sensitivity
                .clamp(MIN_MOUSE_SENSITIVITY, MAX_MOUSE_SENSITIVITY)
        } else {
            1.0
        };
        if self.max_fps != 0 {
            self.max_fps = self.max_fps.clamp(MIN_MAX_FPS, MAX_MAX_FPS);
        }
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
        self.cloud_height = if self.cloud_height.is_finite() {
            self.cloud_height.clamp(MIN_CLOUD_HEIGHT, MAX_CLOUD_HEIGHT)
        } else {
            DEFAULT_CLOUD_HEIGHT
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
        app.insert_resource(SettingsFile {
            path: self.path.clone(),
            save_in: None,
        })
        .insert_resource(load_settings(&self.path))
        .init_resource::<ClientDifficulty>()
        .add_systems(Last, save_settings_when_changed);
    }
}

/// Seconds the options must stay unchanged before they are written, so a
/// dragged slider is one write rather than one per frame.
pub const SAVE_DELAY_SECONDS: f32 = 0.5;

#[derive(Resource)]
struct SettingsFile {
    path: PathBuf,
    /// Seconds until unwritten changes are saved.
    save_in: Option<f32>,
}

/// On-disk form of the settings menu. Extra JSON fields are ignored so older
/// clients can still read a newer file, and missing fields use defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct StoredSettings {
    format_version: u32,
    render_distance: i32,
    difficulty: Difficulty,
    max_fps: u32,
    brightness: f32,
    fov: f32,
    cloud_height: f32,
    old_lighting: bool,
    smooth_lighting: bool,
    directional_lighting: bool,
    wiggle_leaves: bool,
    graphics: GraphicsQuality,
    anti_aliasing: bool,
    mouse_sensitivity: f32,
    view_bobbing: bool,
    fullscreen: bool,
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
            difficulty: settings.difficulty,
            max_fps: settings.max_fps,
            brightness: settings.brightness,
            fov: settings.fov,
            cloud_height: settings.cloud_height,
            old_lighting: settings.old_lighting,
            smooth_lighting: settings.smooth_lighting,
            directional_lighting: settings.directional_lighting,
            wiggle_leaves: settings.wiggle_leaves,
            graphics: settings.graphics,
            anti_aliasing: settings.anti_aliasing,
            mouse_sensitivity: settings.mouse_sensitivity,
            view_bobbing: settings.view_bobbing,
            fullscreen: settings.fullscreen,
        }
    }
}

impl From<StoredSettings> for GameSettings {
    fn from(stored: StoredSettings) -> Self {
        let mut settings = Self {
            render_distance: stored.render_distance,
            difficulty: stored.difficulty,
            max_fps: stored.max_fps,
            brightness: stored.brightness,
            fov: stored.fov,
            cloud_height: stored.cloud_height,
            old_lighting: stored.old_lighting,
            smooth_lighting: stored.smooth_lighting,
            directional_lighting: stored.directional_lighting,
            wiggle_leaves: stored.wiggle_leaves,
            graphics: stored.graphics,
            anti_aliasing: stored.anti_aliasing,
            mouse_sensitivity: stored.mouse_sensitivity,
            view_bobbing: stored.view_bobbing,
            fullscreen: stored.fullscreen,
        };
        settings.clamp();
        settings
    }
}

/// Read `path`, or return defaults when the file is missing or unreadable.
///
/// A field this build cannot read, such as an option value a newer build
/// wrote, falls back to its default without discarding the rest. A file that
/// is not a JSON object at all is kept beside the original as `.bak`, since the
/// next change to the options writes over it.
pub fn load_settings(path: impl AsRef<Path>) -> GameSettings {
    let path = path.as_ref();
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                warn!("Ignoring unreadable settings {}: {error}", path.display());
            }
            return GameSettings::default();
        }
    };
    match serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes) {
        Ok(fields) => GameSettings::from(read_fields(path, fields)),
        Err(error) => {
            warn!("Ignoring unreadable settings {}: {error}", path.display());
            let backup = backup_path(path);
            if let Err(error) = fs::copy(path, &backup) {
                warn!("Cannot keep a copy at {}: {error}", backup.display());
            }
            GameSettings::default()
        }
    }
}

/// Where an unreadable settings file is copied before it is overwritten.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

/// Apply the file's fields one at a time over the defaults, skipping any that
/// do not decode.
fn read_fields(path: &Path, fields: serde_json::Map<String, serde_json::Value>) -> StoredSettings {
    let mut stored = StoredSettings::default();
    let Ok(serde_json::Value::Object(mut merged)) = serde_json::to_value(&stored) else {
        return stored;
    };
    for (name, value) in fields {
        // Fields from a newer build have no place here.
        let Some(previous) = merged.insert(name.clone(), value) else {
            merged.remove(&name);
            continue;
        };
        match serde_json::from_value(serde_json::Value::Object(merged.clone())) {
            Ok(read) => stored = read,
            Err(error) => {
                warn!("Ignoring `{name}` in {}: {error}", path.display());
                merged.insert(name, previous);
            }
        }
    }
    stored
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

fn save_settings_when_changed(
    settings: Res<GameSettings>,
    client_difficulty: Res<ClientDifficulty>,
    mut file: ResMut<SettingsFile>,
    time: Res<Time<Real>>,
    mut exit: MessageReader<AppExit>,
) {
    if settings.is_changed() && !settings.is_added() {
        file.save_in = Some(SAVE_DELAY_SECONDS);
    }
    let exiting = exit.read().next().is_some();
    let Some(remaining) = file.save_in.as_mut() else {
        return;
    };
    *remaining -= time.delta_secs();
    if *remaining > 0.0 && !exiting {
        return;
    }
    file.save_in = None;
    let mut stored = settings.clone();
    // A loaded world's difficulty belongs to that world.
    if let Some(difficulty) = client_difficulty.0 {
        stored.difficulty = difficulty;
    }
    if let Err(error) = save_settings(&file.path, &stored) {
        error!("Cannot save {}: {error}", file.path.display());
    }
}

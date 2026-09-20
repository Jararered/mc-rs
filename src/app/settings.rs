use bevy::prelude::Resource;

pub const MIN_RENDER_DISTANCE: i32 = 4;
pub const MAX_RENDER_DISTANCE: i32 = 32;
pub const MIN_BRIGHTNESS: f32 = 0.0;
pub const MAX_BRIGHTNESS: f32 = 1000.0;
pub const MIN_FOV: f32 = 30.0;
pub const MAX_FOV: f32 = 110.0;
pub const DEFAULT_FOV: f32 = 70.0;

/// Player-facing graphics quality. Fast and Fancy match Beta leaves; Ultra keeps
/// Fancy leaves and switches water to Bevy screen-space reflections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Resource, Debug, Clone)]
pub struct GameSettings {
    pub render_distance: i32,
    pub brightness: f32,
    pub fov: f32,
    pub old_lighting: bool,
    pub directional_lighting: bool,
    pub graphics: GraphicsQuality,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: MIN_RENDER_DISTANCE,
            brightness: 300.0,
            fov: DEFAULT_FOV,
            old_lighting: false,
            directional_lighting: true,
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

    pub fn change_fov(&mut self, change: f32) {
        self.fov = (self.fov + change).clamp(MIN_FOV, MAX_FOV);
    }

    pub fn fov_radians(&self) -> f32 {
        self.fov.to_radians()
    }

    pub fn cycle_graphics(&mut self) {
        self.graphics = self.graphics.cycle();
    }
}

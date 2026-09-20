use bevy::prelude::Resource;

pub const MIN_RENDER_DISTANCE: i32 = 4;
pub const MAX_RENDER_DISTANCE: i32 = 32;
pub const MIN_BRIGHTNESS: f32 = 0.0;
pub const MAX_BRIGHTNESS: f32 = 1000.0;

#[derive(Resource, Debug, Clone)]
pub struct GameSettings {
    pub render_distance: i32,
    pub brightness: f32,
    pub old_lighting: bool,
    pub directional_lighting: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            render_distance: MIN_RENDER_DISTANCE,
            brightness: 300.0,
            old_lighting: true,
            directional_lighting: true,
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
}

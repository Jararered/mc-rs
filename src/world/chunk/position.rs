use bevy::prelude::Component;

use super::CHUNK_SIZE;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const ZERO: Self = Self { x: 0, z: 0 };

    pub fn world_origin(self) -> (f32, f32) {
        (
            (self.x * CHUNK_SIZE as i32) as f32,
            (self.z * CHUNK_SIZE as i32) as f32,
        )
    }
}

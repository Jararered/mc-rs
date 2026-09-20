use bevy::prelude::Component;

use super::CHUNK_SIZE;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub const ZERO: Self = Self { x: 0, z: 0 };

    pub fn from_world(x: f32, z: f32) -> Self {
        Self {
            x: (x / CHUNK_SIZE as f32).floor() as i32,
            z: (z / CHUNK_SIZE as f32).floor() as i32,
        }
    }

    pub fn world_origin(self) -> (f32, f32) {
        (
            (self.x * CHUNK_SIZE as i32) as f32,
            (self.z * CHUNK_SIZE as i32) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ChunkPos;

    #[test]
    fn world_coordinates_cross_chunk_boundaries_in_both_directions() {
        assert_eq!(ChunkPos::from_world(15.99, 16.0), ChunkPos { x: 0, z: 1 });
        assert_eq!(ChunkPos::from_world(-0.1, -16.0), ChunkPos { x: -1, z: -1 });
        assert_eq!(ChunkPos::from_world(-16.1, 0.0), ChunkPos { x: -2, z: 0 });
    }
}

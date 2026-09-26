//! DDA voxel raycast against loaded chunks.

use bevy::prelude::Vec3;

use crate::block::id::Id;
use crate::block::properties::is_targetable;
use crate::block::properties::is_torch;
use crate::block::properties::selection_bounds;
use crate::world::chunk::WorldChunks;

/// Survival-style block reach. Creative in Beta used 5; this matches the default
/// controller distance so flying and walking both feel usable.
pub const BLOCK_REACH: f32 = 5.0;

const MAX_STEPS: u32 = 200;

/// Face of a block the ray entered, matching Beta's `sideHit` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockFace {
    /// −Y
    Down,
    /// +Y
    Up,
    /// −Z
    North,
    /// +Z
    South,
    /// −X
    West,
    /// +X
    East,
}

impl BlockFace {
    pub fn offset(self) -> (i32, i32, i32) {
        match self {
            Self::Down => (0, -1, 0),
            Self::Up => (0, 1, 0),
            Self::North => (0, 0, -1),
            Self::South => (0, 0, 1),
            Self::West => (-1, 0, 0),
            Self::East => (1, 0, 0),
        }
    }

    pub fn neighbor(self, x: i32, y: i32, z: i32) -> (i32, i32, i32) {
        let (dx, dy, dz) = self.offset();
        (x + dx, y + dy, z + dz)
    }
}

/// A targetable block along a camera ray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockHit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: BlockFace,
    pub block: Id,
}

/// Walk the voxel grid from `origin` along `direction` and return the first
/// targetable block within `max_distance`. Air, water, and lava are skipped.
pub fn raycast_blocks(
    chunks: &WorldChunks,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<BlockHit> {
    let direction = direction.normalize_or_zero();
    if direction == Vec3::ZERO || !(max_distance > 0.0) {
        return None;
    }

    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    if let Some(hit) = hit_at(
        chunks,
        x,
        y,
        z,
        entry_face(direction),
        origin,
        direction,
        max_distance,
    ) {
        return Some(hit);
    }

    let step_x = step(direction.x);
    let step_y = step(direction.y);
    let step_z = step(direction.z);
    let t_delta_x = delta(direction.x);
    let t_delta_y = delta(direction.y);
    let t_delta_z = delta(direction.z);
    let mut t_max_x = t_to_next(origin.x, direction.x, x);
    let mut t_max_y = t_to_next(origin.y, direction.y, y);
    let mut t_max_z = t_to_next(origin.z, direction.z, z);

    for _ in 0..MAX_STEPS {
        let (face, t) = if t_max_x <= t_max_y && t_max_x <= t_max_z {
            if t_max_x > max_distance {
                return None;
            }
            let face = if step_x > 0 {
                BlockFace::West
            } else {
                BlockFace::East
            };
            x += step_x;
            let t = t_max_x;
            t_max_x += t_delta_x;
            (face, t)
        } else if t_max_y <= t_max_z {
            if t_max_y > max_distance {
                return None;
            }
            let face = if step_y > 0 {
                BlockFace::Down
            } else {
                BlockFace::Up
            };
            y += step_y;
            let t = t_max_y;
            t_max_y += t_delta_y;
            (face, t)
        } else {
            if t_max_z > max_distance {
                return None;
            }
            let face = if step_z > 0 {
                BlockFace::North
            } else {
                BlockFace::South
            };
            z += step_z;
            let t = t_max_z;
            t_max_z += t_delta_z;
            (face, t)
        };
        if t > max_distance {
            return None;
        }
        if let Some(hit) = hit_at(chunks, x, y, z, face, origin, direction, max_distance) {
            return Some(hit);
        }
    }
    None
}

fn hit_at(
    chunks: &WorldChunks,
    x: i32,
    y: i32,
    z: i32,
    face: BlockFace,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<BlockHit> {
    let block = chunks.block_at(x, y, z)?;
    if !is_targetable(block) {
        return None;
    }
    if is_torch(block) || matches!(block, Id::SnowLayer | Id::Farmland | Id::Crops) {
        let (min, max) = selection_bounds(block);
        let block_origin = Vec3::new(x as f32, y as f32, z as f32);
        if !ray_intersects_box(
            origin,
            direction,
            block_origin + Vec3::from_array(min),
            block_origin + Vec3::from_array(max),
            max_distance,
        ) {
            return None;
        }
    }
    Some(BlockHit {
        x,
        y,
        z,
        face,
        block,
    })
}

fn ray_intersects_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3, reach: f32) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = reach;
    for axis in 0..3 {
        if direction[axis].abs() < f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return false;
            }
        }
    }
    true
}

fn entry_face(direction: Vec3) -> BlockFace {
    let ax = direction.x.abs();
    let ay = direction.y.abs();
    let az = direction.z.abs();
    if ay >= ax && ay >= az {
        if direction.y >= 0.0 {
            BlockFace::Down
        } else {
            BlockFace::Up
        }
    } else if ax >= az {
        if direction.x >= 0.0 {
            BlockFace::West
        } else {
            BlockFace::East
        }
    } else if direction.z >= 0.0 {
        BlockFace::North
    } else {
        BlockFace::South
    }
}

fn step(component: f32) -> i32 {
    if component > 0.0 {
        1
    } else if component < 0.0 {
        -1
    } else {
        0
    }
}

fn delta(component: f32) -> f32 {
    if component.abs() < f32::EPSILON {
        f32::INFINITY
    } else {
        (1.0 / component).abs()
    }
}

fn t_to_next(origin: f32, direction: f32, cell: i32) -> f32 {
    if direction.abs() < f32::EPSILON {
        return f32::INFINITY;
    }
    let boundary = if direction > 0.0 {
        cell as f32 + 1.0
    } else {
        cell as f32
    };
    (boundary - origin) / direction
}

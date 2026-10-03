//! DDA voxel raycast against loaded chunks.

use bevy::prelude::IVec3;
use bevy::prelude::Vec3;

use crate::block::fluids::is_liquid;
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
    raycast(chunks, origin, direction, max_distance, false)
}

/// Like [`raycast_blocks`], but also stops at water and lava, matching
/// Beta's `World.rayTraceBlocks_do(vec1, vec2, true)`. An empty bucket uses
/// this to target a fluid to pick up; other items use [`raycast_blocks`].
pub fn raycast_blocks_or_liquid(
    chunks: &WorldChunks,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
) -> Option<BlockHit> {
    raycast(chunks, origin, direction, max_distance, true)
}

fn raycast(
    chunks: &WorldChunks,
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    include_liquid: bool,
) -> Option<BlockHit> {
    let direction = direction.normalize_or_zero();
    walk(origin, direction, max_distance, |x, y, z, face| {
        hit_at(
            chunks,
            x,
            y,
            z,
            face,
            origin,
            direction,
            max_distance,
            include_liquid,
        )
    })
}

/// Distance from `origin` to where the ray enters `hit`'s selection box:
/// Beta's `objectMouseOver.hitVec.distanceTo(eye)`.
pub fn block_hit_distance(
    chunks: &WorldChunks,
    hit: &BlockHit,
    origin: Vec3,
    direction: Vec3,
) -> f32 {
    let (min, mut max) = selection_bounds(hit.block);
    if hit.block == Id::SnowLayer {
        max[1] = (f32::from(chunks.metadata_at(hit.x, hit.y, hit.z).min(7)) + 1.0) / 8.0;
    }
    let cell = Vec3::new(hit.x as f32, hit.y as f32, hit.z as f32);
    segment_entry(
        origin,
        direction.normalize_or_zero(),
        cell + Vec3::from_array(min),
        cell + Vec3::from_array(max),
        f32::INFINITY,
    )
    .unwrap_or(0.0)
}

/// `World.rayTraceBlocks_do_do(from, to, false, true)`, as arrows trace their
/// flight: the first block with a collision box that the segment enters, and
/// the point where it enters. Plants, torches, and fluids are passed through.
pub fn raycast_collision(chunks: &WorldChunks, from: Vec3, to: Vec3) -> Option<(IVec3, Vec3)> {
    let delta = to - from;
    let length = delta.length();
    let direction = delta.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    walk(from, direction, length, |x, y, z, _| {
        let collider = super::block_collision_box(chunks, x, y, z)?;
        let t = segment_entry(from, direction, collider.min, collider.max, length)?;
        Some((IVec3::new(x, y, z), from + direction * t))
    })
}

/// Where a ray from `origin` first enters a box, if within `reach`. A ray
/// starting inside the box enters at 0.
pub fn segment_entry(
    origin: Vec3,
    direction: Vec3,
    min: Vec3,
    max: Vec3,
    reach: f32,
) -> Option<f32> {
    let mut enter = 0.0_f32;
    let mut exit = reach;
    for axis in 0..3 {
        if direction[axis].abs() < f32::EPSILON {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return None;
            }
        }
    }
    Some(enter)
}

/// Visit each cell a ray crosses within `max_distance`, starting with the
/// cell holding `origin`, until `visit` returns a value.
fn walk<T>(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    mut visit: impl FnMut(i32, i32, i32, BlockFace) -> Option<T>,
) -> Option<T> {
    if direction == Vec3::ZERO || !(max_distance > 0.0) {
        return None;
    }

    let mut x = origin.x.floor() as i32;
    let mut y = origin.y.floor() as i32;
    let mut z = origin.z.floor() as i32;

    if let Some(hit) = visit(x, y, z, entry_face(direction)) {
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
        if let Some(hit) = visit(x, y, z, face) {
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
    include_liquid: bool,
) -> Option<BlockHit> {
    let block = chunks.block_at(x, y, z)?;
    if !is_targetable(block) && !(include_liquid && is_liquid(block)) {
        return None;
    }
    if is_torch(block) || matches!(block, Id::SnowLayer | Id::Farmland | Id::Crops) {
        let (min, mut max) = selection_bounds(block);
        if block == Id::SnowLayer {
            max[1] = (f32::from(chunks.metadata_at(x, y, z).min(7)) + 1.0) / 8.0;
        }
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
    segment_entry(origin, direction, min, max, reach).is_some()
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

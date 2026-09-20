//! Break and place blocks along the camera ray.
//!
//! Inventory is not implemented yet, so every placement is stone.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::entity::EntitySize;
use crate::physics::Aabb;
use crate::physics::BLOCK_REACH;
use crate::physics::BlockHit;
use crate::physics::raycast_blocks;
use crate::world::block::block::BlockId;
use crate::world::block::properties::is_breakable;
use crate::world::block::properties::is_replaceable;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;

use super::Player;

/// Held-button repeat, matching Beta's `ticksPerSecond / 4` (~4 actions/sec).
const INTERACT_REPEAT_SECS: f32 = 0.25;

/// The only block that can be placed until inventory exists.
pub const PLACED_BLOCK: BlockId = BlockId::Stone;

#[derive(Default)]
pub(super) struct BlockInteractState {
    cooldown: f32,
}

pub(super) fn interact_blocks(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    player: Query<(&Transform, &EntitySize), With<Player>>,
    mut chunks: ResMut<WorldChunks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut state: Local<BlockInteractState>,
) {
    state.cooldown = (state.cooldown - time.delta_secs()).max(0.0);

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        return;
    }

    let Ok((transform, size)) = player.single() else {
        return;
    };

    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left);
    let right_held = mouse.pressed(MouseButton::Right);
    if !left_click && !right_click && !left_held && !right_held {
        return;
    }

    let can_repeat = state.cooldown <= 0.0;
    let break_now = left_click || (left_held && can_repeat && !right_click);
    let place_now = !break_now && (right_click || (right_held && can_repeat));
    if !break_now && !place_now {
        return;
    }

    let Some(hit) = raycast_blocks(
        &chunks,
        transform.translation,
        *transform.forward(),
        BLOCK_REACH,
    ) else {
        if left_click || right_click {
            state.cooldown = INTERACT_REPEAT_SECS;
        }
        return;
    };

    let changed = if break_now {
        break_block(&mut chunks, hit)
    } else {
        place_block(&mut chunks, hit, size.aabb(transform.translation))
    };
    state.cooldown = INTERACT_REPEAT_SECS;
    if !changed {
        return;
    }

    let (x, _, z) = if break_now {
        (hit.x, hit.y, hit.z)
    } else {
        hit.face.neighbor(hit.x, hit.y, hit.z)
    };
    notify_edit(&mut streaming, &mut persistence, x, z);
}

/// Remove a targeted block. Bedrock and missing chunks are left unchanged.
pub fn break_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if !is_breakable(hit.block) {
        return false;
    }
    chunks
        .set_block(hit.x, hit.y, hit.z, BlockId::Air)
        .is_some_and(|previous| previous != BlockId::Air)
}

/// Place stone against the hit face. Fails when the cell is occupied, out of
/// the world, or overlapping the player.
pub fn place_block(chunks: &mut WorldChunks, hit: BlockHit, player: Aabb) -> bool {
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return false;
    }
    let Some(current) = chunks.block_at(x, y, z) else {
        return false;
    };
    if !is_replaceable(current) {
        return false;
    }
    if player.intersects(Aabb::from_block(x, y, z)) {
        return false;
    }
    chunks
        .set_block(x, y, z, PLACED_BLOCK)
        .is_some_and(|previous| previous != PLACED_BLOCK)
}

fn notify_edit(
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    x: i32,
    z: i32,
) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPos::from_block(x, z));
    }
    if let Some(streaming) = streaming.as_deref_mut() {
        for position in remesh_chunks_touching(x, z) {
            streaming.request_remesh(position);
        }
    }
}

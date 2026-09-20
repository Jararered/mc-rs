//! Break and place blocks along the camera ray.
//!
//! Inventory is not implemented yet, so every placement is stone and mining
//! uses the empty-hand strength from Beta.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::entity::CollisionState;
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
use super::PlayerCamera;
use super::mining::MiningState;
use super::overlay::BlockFocus;

/// Held-button place repeat, matching Beta's `ticksPerSecond / 4`.
const INTERACT_REPEAT_SECS: f32 = 0.25;
const TICK_SECS: f32 = 1.0 / 20.0;
const MAX_TICKS_PER_FRAME: u32 = 4;

/// The only block that can be placed until inventory exists.
pub const PLACED_BLOCK: BlockId = BlockId::Stone;

#[derive(Default)]
pub(super) struct BlockInteractState {
    place_cooldown: f32,
    tick_accum: f32,
    mining: MiningState,
}

pub(super) fn interact_blocks(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    player: Query<(&Transform, &EntitySize, &CollisionState), With<Player>>,
    camera: Query<&Transform, With<PlayerCamera>>,
    mut chunks: ResMut<WorldChunks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut focus: ResMut<BlockFocus>,
    mut state: Local<BlockInteractState>,
) {
    state.place_cooldown = (state.place_cooldown - time.delta_secs()).max(0.0);

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        state.mining.reset();
        state.tick_accum = 0.0;
        *focus = BlockFocus::default();
        return;
    }

    let Ok((transform, size, collision)) = player.single() else {
        *focus = BlockFocus::default();
        return;
    };

    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left);
    let right_held = mouse.pressed(MouseButton::Right);

    if !left_held {
        state.mining.reset();
        state.tick_accum = 0.0;
    }

    let camera_transform = camera.single().ok();
    let view_rotation = transform.rotation
        * camera_transform
            .map(|camera| camera.rotation)
            .unwrap_or(Quat::IDENTITY);
    let view_origin = transform.translation
        + transform.rotation
            * camera_transform
                .map(|camera| camera.translation)
                .unwrap_or(Vec3::ZERO);
    let hit = raycast_blocks(
        &chunks,
        view_origin,
        view_rotation * Vec3::NEG_Z,
        BLOCK_REACH,
    );
    let in_water = chunks.block_at(
        transform.translation.x.floor() as i32,
        transform.translation.y.floor() as i32,
        transform.translation.z.floor() as i32,
    ) == Some(BlockId::Water);
    let on_ground = collision.on_ground;

    if left_held {
        if left_click && let Some(hit) = hit {
            if let Some(broken) = state.mining.try_instant(hit, on_ground, in_water) {
                apply_break(&mut chunks, &mut streaming, &mut persistence, broken);
            }
        }
        state.tick_accum += time.delta_secs();
        let mut ticks = 0;
        while state.tick_accum >= TICK_SECS && ticks < MAX_TICKS_PER_FRAME {
            state.tick_accum -= TICK_SECS;
            ticks += 1;
            if let Some(broken) = state.mining.tick(hit, on_ground, in_water) {
                apply_break(&mut chunks, &mut streaming, &mut persistence, broken);
            }
        }
    }

    let can_place = right_click || (right_held && state.place_cooldown <= 0.0 && !left_held);
    if can_place {
        state.place_cooldown = INTERACT_REPEAT_SECS;
        if let Some(hit) = hit
            && place_block(&mut chunks, hit, size.aabb(transform.translation))
        {
            let (x, _, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
            notify_edit(&mut streaming, &mut persistence, x, z);
        }
    }

    focus.hit = hit;
    focus.mining_damage = state.mining.damage();
}

fn apply_break(
    chunks: &mut WorldChunks,
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    hit: BlockHit,
) {
    if break_block(chunks, hit) {
        notify_edit(streaming, persistence, hit.x, hit.z);
    }
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

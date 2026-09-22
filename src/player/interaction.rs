//! Break and place blocks along the camera ray.
//!
//! Selected hotbar blocks can be placed. Mining speed, drops, and tool wear
//! follow the held stack.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::block_drops::natural_drops;
use crate::entity::block_drops::player_break_drops;
use crate::entity::dropped_items::ItemRng;
use crate::entity::dropped_items::spawn_block_drop;
use crate::entity::dropped_items::spawn_thrown_item;
use crate::entity::particles::BlockParticles;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::item::tools::break_durability;
use crate::physics::Aabb;
use crate::physics::BLOCK_REACH;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::physics::raycast_blocks;
use crate::ui::InventoryScreen;
use crate::ui::WorkbenchUiSession;
use crate::ui::close_crafting_interface;
use crate::world::block::block::BlockId;
use crate::world::block::properties::is_breakable;
use crate::world::block::properties::is_crossed_plant;
use crate::world::block::properties::is_opaque_cube;
use crate::world::block::properties::is_replaceable;
use crate::world::block::properties::is_torch;
use crate::world::block::properties::plant_grows_on;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

use super::Player;
use super::PlayerCamera;
use super::mining::MiningState;
use super::overlay::BlockFocus;

/// Held-button place repeat, matching Beta's `ticksPerSecond / 4`.
const PLACE_DELAY_TICKS: i32 = 5;

/// Torch used by the standalone placement helper and legacy tests.
pub const PLACED_BLOCK: BlockId = BlockId::Torch;

#[derive(Default)]
pub(super) struct BlockInteractState {
    place_delay: i32,
    mining: MiningState,
}

pub(super) fn interact_blocks(
    mut commands: Commands,
    tick: Res<WorldTick>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut player: Query<
        (
            &Transform,
            &EntitySize,
            &CollisionState,
            &mut Hotbar,
            &mut Inventory,
        ),
        With<Player>,
    >,
    camera: Query<&Transform, With<PlayerCamera>>,
    mut chunks: ResMut<WorldChunks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut particles: Option<ResMut<BlockParticles>>,
    mut focus: ResMut<BlockFocus>,
    mut state: Local<BlockInteractState>,
    mut inventory_screen: ResMut<InventoryScreen>,
    mut workbench: ResMut<WorkbenchUiSession>,
    mut item_rng: Local<ItemRng>,
) {
    let ticks = tick.ticks_this_frame();
    for _ in 0..ticks {
        if state.place_delay > 0 {
            state.place_delay -= 1;
        }
    }

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }

    let Ok((transform, size, collision, mut hotbar, mut inventory)) = player.single_mut() else {
        *focus = BlockFocus::default();
        return;
    };

    if locked
        && !inventory_screen.open
        && keys.just_pressed(KeyCode::KeyQ)
        && let Some(stack) = hotbar.take_selected(1)
    {
        spawn_thrown_item(
            &mut commands,
            &mut item_rng,
            transform,
            *transform.forward(),
            stack,
        );
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPos::from_block(
                transform.translation.x.floor() as i32,
                transform.translation.z.floor() as i32,
            ));
        }
    }

    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left);
    let right_held = mouse.pressed(MouseButton::Right);

    if !left_held {
        state.mining.reset();
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
    if right_click
        && !inventory_screen.open
        && hit.is_some_and(|hit| hit.block == BlockId::CraftingTable)
    {
        let hit = hit.expect("checked above");
        // Do not let stale player-grid contents leak into a new workbench
        // session if an earlier interface was interrupted before its close
        // system ran.
        close_crafting_interface(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
        inventory_screen.open = true;
        inventory_screen.workbench = true;
        workbench.position = Some((hit.x, hit.y, hit.z));
        if let Ok((_, mut cursor)) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    let in_water = chunks.block_at(
        transform.translation.x.floor() as i32,
        transform.translation.y.floor() as i32,
        transform.translation.z.floor() as i32,
    ) == Some(BlockId::Water);
    let on_ground = collision.on_ground;

    if left_held {
        if left_click && let Some(hit) = hit {
            let tool = hotbar.selected_stack();
            if let Some(broken) = state.mining.try_instant(hit, tool, on_ground, in_water) {
                apply_break(
                    &mut commands,
                    &mut item_rng,
                    &mut chunks,
                    &mut streaming,
                    &mut persistence,
                    &mut particles,
                    &mut hotbar,
                    broken,
                );
            }
        }
        for _ in 0..ticks {
            let old_damage = state.mining.damage();
            let tool = hotbar.selected_stack();
            if let Some(broken) = state.mining.tick(hit, tool, on_ground, in_water) {
                apply_break(
                    &mut commands,
                    &mut item_rng,
                    &mut chunks,
                    &mut streaming,
                    &mut persistence,
                    &mut particles,
                    &mut hotbar,
                    broken,
                );
            } else if state.mining.damage() > old_damage
                && let Some(hit) = hit
                && let Some(particles) = particles.as_deref_mut()
            {
                particles.emit_hit(hit);
            }
        }
    }

    let can_place = right_click || (right_held && state.place_delay <= 0 && !left_held);
    if can_place {
        state.place_delay = PLACE_DELAY_TICKS;
        if let Some(hit) = hit
            && let Some(stack) = hotbar.selected_stack()
            && let Some(block) = stack.runtime_block()
            && place_selected_block(&mut chunks, hit, size.aabb(transform.translation), block)
        {
            let selected = hotbar.selected;
            hotbar.slots[selected] =
                ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
            let (x, _, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
            notify_edit(&mut streaming, &mut persistence, x, z, true);
        }
    }

    focus.hit = hit;
    focus.mining_damage = state.mining.damage();
}

fn apply_break(
    commands: &mut Commands,
    rng: &mut ItemRng,
    chunks: &mut WorldChunks,
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    particles: &mut Option<ResMut<BlockParticles>>,
    hotbar: &mut Hotbar,
    hit: BlockHit,
) {
    let mut attached = [
        (0, 1, 0, BlockId::Torch),
        (1, 0, 0, BlockId::TorchWest),
        (-1, 0, 0, BlockId::TorchEast),
        (0, 0, 1, BlockId::TorchNorth),
        (0, 0, -1, BlockId::TorchSouth),
    ]
    .into_iter()
    .filter_map(|(dx, dy, dz, torch)| {
        (chunks.block_at(hit.x + dx, hit.y + dy, hit.z + dz) == Some(torch)).then_some((
            hit.x + dx,
            hit.y + dy,
            hit.z + dz,
            torch,
        ))
    })
    .collect::<Vec<_>>();
    if let Some(plant) = chunks
        .block_at(hit.x, hit.y + 1, hit.z)
        .filter(|block| is_crossed_plant(*block))
    {
        attached.push((hit.x, hit.y + 1, hit.z, plant));
    }
    let light_edit = is_torch(hit.block)
        || attached
            .iter()
            .copied()
            .into_iter()
            .any(|(_, _, _, torch)| is_torch(torch));
    let tool = hotbar.selected_stack();
    // `canHarvestBlock` gates the harvest drop. The tool still takes durability
    // when the block comes out, including a block the tool cannot harvest.
    // TNT's player-destroy drop is not part of that gate.
    if break_block(chunks, hit) {
        for stack in player_break_drops(hit.block, tool, rng) {
            spawn_block_drop(commands, rng, IVec3::new(hit.x, hit.y, hit.z), stack);
        }
        for (x, y, z, torch) in attached {
            for stack in natural_drops(torch, rng) {
                spawn_block_drop(commands, rng, IVec3::new(x, y, z), stack);
            }
        }
        if let Some(particles) = particles.as_deref_mut() {
            particles.emit_break(hit);
        }
        notify_edit(streaming, persistence, hit.x, hit.z, light_edit);
        if let Some(tool) = tool {
            let cost = break_durability(tool, hit.block);
            if cost > 0 {
                hotbar.damage_selected(cost);
            }
        }
    }
}

/// Remove a targeted block. Bedrock and missing chunks are left unchanged.
pub fn break_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if !is_breakable(hit.block) {
        return false;
    }
    let broken = chunks
        .set_block(hit.x, hit.y, hit.z, BlockId::Air)
        .is_some_and(|previous| previous != BlockId::Air);
    if broken {
        for (dx, dy, dz, attached) in [
            (0, 1, 0, BlockId::Torch),
            (1, 0, 0, BlockId::TorchWest),
            (-1, 0, 0, BlockId::TorchEast),
            (0, 0, 1, BlockId::TorchNorth),
            (0, 0, -1, BlockId::TorchSouth),
        ] {
            let (x, y, z) = (hit.x + dx, hit.y + dy, hit.z + dz);
            if chunks.block_at(x, y, z) == Some(attached) {
                chunks.set_block(x, y, z, BlockId::Air);
            }
        }
        if chunks
            .block_at(hit.x, hit.y + 1, hit.z)
            .is_some_and(is_crossed_plant)
        {
            chunks.set_block(hit.x, hit.y + 1, hit.z, BlockId::Air);
        }
    }
    broken
}

/// Attach a torch to the hit face. Fails without a solid support block.
pub fn place_block(chunks: &mut WorldChunks, hit: BlockHit, player: Aabb) -> bool {
    place_selected_block(chunks, hit, player, BlockId::Torch)
}

pub fn place_selected_block(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: BlockId,
) -> bool {
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
    if is_crossed_plant(selected) && !chunks.block_at(x, y - 1, z).is_some_and(plant_grows_on) {
        return false;
    }
    if !is_opaque_cube(hit.block) && selected == BlockId::Torch {
        return false;
    }
    let block = if selected == BlockId::Torch {
        match hit.face {
            BlockFace::Up => BlockId::Torch,
            BlockFace::Down => return false,
            BlockFace::West => BlockId::TorchEast,
            BlockFace::East => BlockId::TorchWest,
            BlockFace::North => BlockId::TorchSouth,
            BlockFace::South => BlockId::TorchNorth,
        }
    } else {
        selected
    };
    if is_opaque_cube(block)
        && player.intersects(Aabb::new(
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new(x as f32 + 1.0, y as f32 + 1.0, z as f32 + 1.0),
        ))
    {
        return false;
    }
    chunks
        .set_block(x, y, z, block)
        .is_some_and(|previous| previous != block)
}

fn notify_edit(
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    x: i32,
    z: i32,
    light_edit: bool,
) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPos::from_block(x, z));
        if light_edit {
            // A detached wall torch may belong to the neighboring chunk.
            for position in remesh_chunks_touching(x, z) {
                persistence.mark_dirty(position);
            }
        }
    }
    if let Some(streaming) = streaming.as_deref_mut() {
        if light_edit {
            let center = ChunkPos::from_block(x, z);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    streaming.request_remesh(ChunkPos {
                        x: center.x + dx,
                        z: center.z + dz,
                    });
                }
            }
        } else {
            for position in remesh_chunks_touching(x, z) {
                streaming.request_remesh(position);
            }
        }
    }
}

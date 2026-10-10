//! Break and place blocks along the camera ray.
//!
//! Selected hotbar blocks can be placed. Mining speed, drops, and tool wear
//! follow the held stack.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::block::blocks::Block;
use crate::block::fluids::is_water;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::boat::BOAT_SIZE;
use crate::entity::boat::Boat;
use crate::entity::combat::bordered;
use crate::entity::combat::pick;
use crate::entity::minecart::CART_SIZE;
use crate::entity::minecart::Minecart;
use crate::entity::projectiles::FIREBALL_SIZE;
use crate::entity::projectiles::Fireball;
use crate::inventory::Hotbar;
use crate::inventory::session::InventorySession;
use crate::physics::BLOCK_REACH;
use crate::physics::block_hit_distance;
use crate::physics::raycast_blocks;
use crate::player::interaction::attack::ENTITY_REACH;
use crate::player::interaction::attack::FIREBALL_BORDER;
use crate::player::interaction::attack::MOB_BORDER;
use crate::rendering::particles::block::BlockParticles;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

use super::mining::MiningState;
use super::overlay::BlockFocus;
use crate::entity::mobs::Mob;
use crate::player::LocalPlayer;
use crate::player::PlayerCamera;
use crate::player::actions::Action;
use crate::player::actions::PlayerAction;
use crate::player::actions::Pointed;
use crate::player::actions::Window as OpenedWindow;
use crate::player::actions::WindowOpen;
use crate::player::sleep::PlayerSleep;

mod apply;
mod breaking;
mod placement;
mod tools;

pub(crate) use apply::apply_player_actions;
pub use breaking::break_block;
pub use placement::place_bed;
pub use placement::place_block;
pub use placement::place_door;
pub use placement::place_selected_block;
pub use placement::place_selected_block_facing;
pub use placement::place_sign;
pub use tools::pick_up_fluid;
pub use tools::place_fluid;
pub use tools::plant_seeds;
pub use tools::till_block;
pub use tools::till_with_selected_hoe;

/// Held-button place repeat, matching Beta's `ticksPerSecond / 4`.
const PLACE_DELAY_TICKS: i32 = 5;

/// Torch used by the standalone placement helper and legacy tests.
pub const PLACED_BLOCK: Block = Block::Torch;

#[derive(Default)]
pub(crate) struct BlockInteractState {
    place_delay: i32,
    mining: MiningState,
    /// Set while the cursor is free; the left button must be released once
    /// after the cursor is grabbed before it can mine or attack.
    wait_for_release: bool,
    /// Frame this system last ran; a gap means the world was not being played
    /// (menus, pause, chat), so the cursor grab and its click are fresh.
    last_frame: Option<u32>,
}

/// `ItemBoat`'s own reach.
const BOAT_REACH: f32 = 5.0;

pub(crate) fn interact_blocks(
    tick: Res<WorldTick>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<(&Window, &CursorOptions), With<PrimaryWindow>>,
    player: Query<
        (
            Entity,
            &Transform,
            &CollisionState,
            &Hotbar,
            Option<&PlayerSleep>,
        ),
        With<LocalPlayer>,
    >,
    camera: Query<&Transform, With<PlayerCamera>>,
    chunks: Res<WorldChunks>,
    mut particles: Option<ResMut<BlockParticles>>,
    (mobs, fireballs, carts, boats): (
        Query<(Entity, &Transform, &EntitySize), (With<Mob>, Without<LocalPlayer>)>,
        Query<(Entity, &Transform), (With<Fireball>, Without<LocalPlayer>)>,
        Query<(Entity, &Transform), (With<Minecart>, Without<LocalPlayer>)>,
        Query<(Entity, &Transform), (With<Boat>, Without<LocalPlayer>)>,
    ),
    mut focus: ResMut<BlockFocus>,
    (mut state, frame): (Local<BlockInteractState>, Res<bevy::diagnostic::FrameCount>),
    inventory_screen: Res<InventorySession>,
    mut actions: MessageWriter<PlayerAction>,
) {
    let ticks = tick.ticks_this_frame();
    for _ in 0..ticks {
        if state.place_delay > 0 {
            state.place_delay -= 1;
        }
    }

    if state
        .last_frame
        .is_none_or(|last| last.wrapping_add(1) != frame.0)
    {
        state.wait_for_release = true;
    }
    state.last_frame = Some(frame.0);

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        state.mining.reset();
        state.wait_for_release = true;
        *focus = BlockFocus::default();
        return;
    }

    // The click that grabbed the cursor must not also break a block.
    if state.wait_for_release {
        state.wait_for_release = mouse.pressed(MouseButton::Left);
    }
    let click_carried = state.wait_for_release;

    let Ok((player_entity, transform, collision, hotbar, sleep)) = player.single() else {
        *focus = BlockFocus::default();
        return;
    };
    // `isMovementBlocked`: a sleeping player's hands are still.
    if sleep.is_some_and(|sleep| sleep.sleeping) {
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    let mut send = |action: Action| {
        actions.write(PlayerAction {
            player: player_entity,
            action,
        });
    };

    if !inventory_screen.open && keys.just_pressed(KeyCode::KeyQ) {
        send(Action::DropItem);
    }

    let left_click = mouse.just_pressed(MouseButton::Left) && !click_carried;
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left) && !click_carried;
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
    let look = view_rotation * Vec3::NEG_Z;
    let hit = raycast_blocks(&chunks, view_origin, look, BLOCK_REACH);
    if !inventory_screen.open && (left_click || right_click) {
        // `getMouseOver`: an entity counts only nearer than the block in view.
        let reach = hit.map_or(ENTITY_REACH, |hit| {
            block_hit_distance(&chunks, &hit, view_origin, look).min(ENTITY_REACH)
        });
        let pointed = pick(
            view_origin,
            look,
            reach,
            mobs.iter()
                .map(|(entity, transform, size)| {
                    let aabb = size.aabb(transform.translation);
                    (Pointed::Mob(entity), bordered(aabb, MOB_BORDER))
                })
                .chain(fireballs.iter().map(|(entity, transform)| {
                    let aabb = FIREBALL_SIZE.aabb(transform.translation);
                    (Pointed::Fireball(entity), bordered(aabb, FIREBALL_BORDER))
                }))
                .chain(carts.iter().map(|(entity, transform)| {
                    let aabb = CART_SIZE.aabb(transform.translation);
                    (Pointed::Minecart(entity), bordered(aabb, MOB_BORDER))
                }))
                .chain(boats.iter().map(|(entity, transform)| {
                    let aabb = BOAT_SIZE.aabb(transform.translation);
                    (Pointed::Boat(entity), bordered(aabb, MOB_BORDER))
                })),
        );
        if let Some(target) = pointed {
            send(Action::UseEntity {
                target,
                attack: left_click,
                look,
            });
            state.mining.reset();
            *focus = BlockFocus::default();
            return;
        }
    }
    // A container's `blockActivated` opens its window and takes the click.
    if right_click
        && !inventory_screen.open
        && hit.is_some_and(|hit| {
            hit.block.is_furnace()
                || hit.block.is_chest()
                || matches!(hit.block, Block::Dispenser | Block::CraftingTable)
        })
    {
        send(Action::Use {
            hit,
            origin: view_origin,
            look,
            click: true,
        });
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    let in_water = chunks
        .block_at(
            transform.translation.x.floor() as i32,
            transform.translation.y.floor() as i32,
            transform.translation.z.floor() as i32,
        )
        .is_some_and(is_water);
    let on_ground = collision.on_ground;

    // `PlayerControllerSP`: the client times the dig and says when the block
    // goes. The tool is the one held when the frame began.
    if left_held {
        let tool = hotbar.selected_stack();
        if left_click && let Some(hit) = hit {
            send(Action::StartDig { hit });
            if let Some(broken) = state.mining.try_instant(hit, tool, on_ground, in_water) {
                send(Action::Break { hit: broken });
            }
        }
        for _ in 0..ticks {
            let old_damage = state.mining.damage();
            if let Some(broken) = state.mining.tick(hit, tool, on_ground, in_water) {
                send(Action::Break { hit: broken });
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
        send(Action::Use {
            hit,
            origin: view_origin,
            look,
            click: right_click,
        });
    }

    focus.hit = hit;
    focus.mining_damage = state.mining.damage();
}

/// `Packet100OpenWindow` for this client's player: show the screen the world
/// opened and free the cursor for it.
pub(crate) fn open_windows(
    mut opened: MessageReader<WindowOpen>,
    player: Query<Entity, With<LocalPlayer>>,
    mut inventory_screen: ResMut<InventorySession>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let local = player.single().ok();
    for WindowOpen { player, window } in opened.read().copied() {
        if Some(player) != local {
            continue;
        }
        let cell = |position: IVec3| (position.x, position.y, position.z);
        inventory_screen.open = true;
        inventory_screen.workbench = false;
        inventory_screen.furnace = false;
        inventory_screen.furnace_position = None;
        inventory_screen.chest = false;
        inventory_screen.chest_position = None;
        inventory_screen.chest_group = None;
        inventory_screen.cart = None;
        match window {
            OpenedWindow::Workbench { .. } => inventory_screen.workbench = true,
            OpenedWindow::Furnace { position } => {
                inventory_screen.furnace = true;
                inventory_screen.furnace_position = Some(cell(position));
            }
            OpenedWindow::Chest { position, group } => {
                inventory_screen.chest = true;
                inventory_screen.chest_position = Some(cell(position));
                inventory_screen.chest_group = Some(group);
            }
            OpenedWindow::CartChest { cart, cell: at } => {
                inventory_screen.chest = true;
                inventory_screen.chest_position = Some(cell(at));
                inventory_screen.chest_group = Some(ChestGroup {
                    first: cell(at),
                    second: None,
                    dispenser: false,
                    cart: true,
                });
                inventory_screen.cart = Some(cart);
            }
        }
        if let Ok(mut cursor) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
    }
}

/// Queue a block event for the next tick pass.
fn push_event(ticks: &mut Option<ResMut<BlockTicks>>, event: BlockEvent) {
    if let Some(ticks) = ticks.as_deref_mut() {
        ticks.push_event(event);
    }
}

fn notify_edit(
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    x: i32,
    y: i32,
    z: i32,
    light_edit: bool,
) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPosition::from_block(x, z));
        if light_edit {
            // A detached wall torch may belong to the neighboring chunk.
            for position in remesh_chunks_touching(x, z) {
                persistence.mark_dirty(position);
            }
        }
    }
    // Relighting decides which sections actually changed, so torches and
    // plain blocks take the same path.
    if let Some(streaming) = streaming.as_deref_mut() {
        streaming.request_block_update(x, y, z);
    }
}

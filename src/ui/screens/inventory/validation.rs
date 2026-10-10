//! Closes a container screen whose block is gone or out of reach, and keeps
//! an open chest cart in step with its entity.

use super::InventoryRoot;
use crate::block::blocks::Block;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::Minecart;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::session::close_crafting_session;
use crate::player::LocalPlayer;
use crate::random::ItemRng;
use crate::world::chunk::WorldChunks;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

pub(super) fn validate_workbench(
    mut commands: Commands,
    mut screen: ResMut<InventorySession>,
    mut session: ResMut<ActiveWorkbench>,
    chunks: Res<WorldChunks>,
    player_transform: Query<&Transform, With<LocalPlayer>>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<LocalPlayer>>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.workbench {
        return;
    }
    let Some((x, y, z)) = session.position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    let player_position = transform.translation;
    let dx = player_position.x - (x as f32 + 0.5);
    let dy = player_position.y - (y as f32 + 0.5);
    let dz = player_position.z - (z as f32 + 0.5);
    if chunks.block_at(x, y, z) == Some(Block::CraftingTable) && dx * dx + dy * dy + dz * dz <= 64.0
    {
        return;
    }
    if let Ok((mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_session(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut session,
        );
    }
    screen.open = false;
    screen.workbench = false;
    session.position = None;
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

pub(super) fn validate_furnace(
    mut commands: Commands,
    mut screen: ResMut<InventorySession>,
    chunks: Res<WorldChunks>,
    player_transform: Query<&Transform, With<LocalPlayer>>,
    mut player: Query<(&mut Hotbar, &mut Inventory), With<LocalPlayer>>,
    mut workbench: ResMut<ActiveWorkbench>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.furnace {
        return;
    }
    let Some((x, y, z)) = screen.furnace_position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    let delta = transform.translation - Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
    let block = chunks.block_at(x, y, z);
    if block.is_some_and(Block::is_furnace) && delta.length_squared() <= 64.0 {
        return;
    }
    screen.open = false;
    screen.furnace = false;
    screen.furnace_position = None;
    if let Ok((mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_session(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

/// Close the open screen when gameplay asks for it
/// ([`InventorySession::close_requested`]).
pub(super) fn close_when_requested(
    mut commands: Commands,
    mut screen: ResMut<InventorySession>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<LocalPlayer>>,
    mut workbench: ResMut<ActiveWorkbench>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.close_requested {
        return;
    }
    let was_open = screen.open;
    *screen = InventorySession::default();
    if !was_open {
        return;
    }
    if let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_session(
            &mut commands,
            player_transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

/// Carry a chest cart screen's edits back to the cart, and let go of the
/// copy once the screen is not on that cart any more.
pub(super) fn sync_open_cart(
    screen: Res<InventorySession>,
    mut chunks: ResMut<WorldChunks>,
    mut carts: Query<&mut Cargo>,
) {
    let Some(open) = chunks.open_cart.as_ref() else {
        return;
    };
    let cart = open.cart;
    if let Ok(mut cargo) = carts.get_mut(cart)
        && cargo.0 != open.slots
    {
        cargo.0 = open.slots;
    }
    if !(screen.open && screen.chest && screen.cart == Some(cart)) {
        chunks.open_cart = None;
    }
}

pub(super) fn validate_chest(
    mut commands: Commands,
    mut screen: ResMut<InventorySession>,
    chunks: Res<WorldChunks>,
    carts: Query<&Transform, (With<Minecart>, Without<LocalPlayer>)>,
    player_transform: Query<&Transform, With<LocalPlayer>>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<LocalPlayer>>,
    mut workbench: ResMut<ActiveWorkbench>,
    roots: Query<Entity, With<InventoryRoot>>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut item_rng: Local<ItemRng>,
) {
    if !screen.open || !screen.chest {
        return;
    }
    let Some((x, y, z)) = screen.chest_position else {
        return;
    };
    let Ok(transform) = player_transform.single() else {
        return;
    };
    if let Some(cart) = screen.cart {
        // `EntityMinecart.canInteractWith`: the cart is still there and within
        // eight blocks.
        if carts
            .get(cart)
            .is_ok_and(|at| at.translation.distance_squared(transform.translation) <= 64.0)
        {
            return;
        }
    } else {
        let delta =
            transform.translation - Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
        if chunks.container_group_at(x, y, z) == screen.chest_group
            && delta.length_squared() <= 64.0
        {
            return;
        }
    }
    screen.open = false;
    screen.chest = false;
    screen.chest_position = None;
    screen.chest_group = None;
    screen.cart = None;
    if let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() {
        close_crafting_session(
            &mut commands,
            player_transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if let Ok(mut cursor) = windows.single_mut() {
        cursor.visible = false;
        cursor.grab_mode = CursorGrabMode::Locked;
    }
}

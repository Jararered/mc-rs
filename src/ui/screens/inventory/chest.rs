//! Slot clicks and storage for single chests, double chests and chest carts.

use super::CHEST_HALF_SLOTS;
use super::HOTBAR_KEYS;
use super::LastInventoryClick;
use super::Slot;
use super::SlotDrag;
use super::clicks::apply_click;
use super::clicks::click_slot;
use super::clicks::to_slot_id;
use super::is_double_click;
use crate::inventory::DragPlace;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::chest_drag_place;
use crate::inventory::chest_slot_accepts_drag;
use crate::inventory::collect_matching_stacks;
use crate::inventory::hotbar_key_swap_chest;
use crate::inventory::quick_move_drag_slot;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::shift_click_chest_slot;
use crate::inventory::sort_container_slots;
use crate::inventory::sort_main_inventory;
use crate::item::ItemStack;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_chest_slots(
    screen: &InventorySession,
    mouse: &ButtonInput<MouseButton>,
    keys: &ButtonInput<KeyCode>,
    slots: &Query<(&RelativeCursorPosition, &Slot)>,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
    chunks: &mut WorldChunks,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    drag: &mut SlotDrag,
    last_click: &mut LastInventoryClick,
    now: f64,
) {
    let Some(group) = screen.chest_group else {
        return;
    };
    let hovered = slots
        .iter()
        .find_map(|(cursor, slot)| cursor.cursor_over().then_some(*slot));
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if let Some(slot) = hovered {
        for (index, key) in HOTBAR_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                *drag = SlotDrag::default();
                let mut chest_slots = read_chest_group_slots(chunks, group);
                let _ = hotbar_key_swap_chest(
                    inventory,
                    hotbar,
                    &mut chest_slots,
                    to_slot_id(slot),
                    index,
                );
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyS) && drag.button.is_none() {
        match hovered {
            Some(Slot::Chest(_)) => {
                let mut chest_slots = read_chest_group_slots(chunks, group);
                sort_container_slots(&mut chest_slots);
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
            Some(Slot::Main(_)) => sort_main_inventory(inventory),
            _ => {}
        }
        return;
    }
    if drag.button.is_none() {
        let left = mouse.just_pressed(MouseButton::Left);
        let right = mouse.just_pressed(MouseButton::Right);
        if !(left || right) {
            return;
        }
        let Some(slot) = hovered else {
            return;
        };
        let right = right && !left;
        if !right && !shift && is_double_click(last_click, screen, slot, now) {
            if matches!(slot, Slot::Chest(_)) {
                let mut chest_slots = read_chest_group_slots(chunks, group);
                collect_matching_stacks(&mut inventory.carried, &mut chest_slots);
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            } else {
                collect_storage_stacks(inventory, hotbar);
            }
            *drag = SlotDrag::default();
            return;
        }
        if right || shift {
            last_click.at = None;
        }
        if shift && !right {
            drag.button = Some(MouseButton::Left);
            drag.origin = Some(slot);
            drag.slots.clear();
            drag.quick_move = true;
            drag.quick_move_visited.clear();
        } else if shift {
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let _ = shift_click_chest_slot(inventory, hotbar, &mut chest_slots, to_slot_id(slot));
            write_chest_group_slots(chunks, group, &chest_slots);
            mark_chest_dirty(persistence, group);
            return;
        }
        if !drag.quick_move && inventory.carried.is_none() {
            apply_chest_click(slot, right, group, chunks, hotbar, inventory, workbench);
            mark_chest_dirty(persistence, group);
            return;
        }
        if !drag.quick_move {
            drag.button = Some(if right {
                MouseButton::Right
            } else {
                MouseButton::Left
            });
            drag.origin = Some(slot);
            drag.slots.clear();
        }
    }
    let Some(button) = drag.button else {
        return;
    };
    if drag.quick_move {
        if shift && let Some(slot) = hovered {
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let moved = quick_move_drag_slot(
                &mut drag.quick_move_visited,
                inventory,
                hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                Some(&mut chest_slots),
                None,
                to_slot_id(slot),
            );
            if moved {
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
        }
        if mouse.pressed(button) {
            return;
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        let chest_slots = read_chest_group_slots(chunks, group);
        remember_chest_drag_slot(drag, slot, inventory, hotbar, &chest_slots);
    }
    if mouse.pressed(button) {
        return;
    }
    let mode = if button == MouseButton::Right {
        DragPlace::OneEach
    } else {
        DragPlace::Split
    };
    let painted = drag.slots.clone();
    let origin = drag.origin;
    *drag = SlotDrag::default();
    if painted.len() >= 2 {
        let ids: Vec<SlotId> = painted.into_iter().map(to_slot_id).collect();
        let mut chest_slots = read_chest_group_slots(chunks, group);
        let _ = chest_drag_place(inventory, hotbar, &mut chest_slots, &ids, mode);
        write_chest_group_slots(chunks, group, &chest_slots);
    } else if let Some(slot) = hovered.or(if painted.len() == 1 {
        painted.first().copied()
    } else {
        origin
    }) {
        apply_chest_click(
            slot,
            mode == DragPlace::OneEach,
            group,
            chunks,
            hotbar,
            inventory,
            workbench,
        );
    }
    mark_chest_dirty(persistence, group);
}

pub(super) fn remember_chest_drag_slot(
    drag: &mut SlotDrag,
    slot: Slot,
    inventory: &Inventory,
    hotbar: &Hotbar,
    chest: &[Option<ItemStack>],
) {
    let Some(carried) = inventory.carried else {
        return;
    };
    if drag.slots.contains(&slot) || (carried.count() as usize) <= drag.slots.len() {
        return;
    }
    if chest_slot_accepts_drag(inventory, hotbar, chest, to_slot_id(slot), carried) {
        drag.slots.push(slot);
    }
}

pub(super) fn apply_chest_click(
    slot: Slot,
    right: bool,
    group: ChestGroup,
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
) {
    if let Slot::Chest(index) = slot {
        let mut chest_slots = read_chest_group_slots(chunks, group);
        if let Some(stack) = chest_slots.get_mut(index) {
            click_slot(stack, &mut inventory.carried, right);
        }
        write_chest_group_slots(chunks, group, &chest_slots);
    } else {
        apply_click(slot, right, false, hotbar, inventory, workbench);
    }
}

pub(super) fn collect_player_stacks(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    workbench: &mut ActiveWorkbench,
    workbench_open: bool,
) {
    collect_storage_stacks(inventory, hotbar);
    let Inventory {
        crafting,
        armor,
        carried,
        ..
    } = inventory;
    if workbench_open {
        collect_matching_stacks(carried, workbench.grid.slots_mut());
    } else {
        collect_matching_stacks(carried, crafting);
    }
    collect_matching_stacks(carried, armor);
}

pub(super) fn collect_storage_stacks(inventory: &mut Inventory, hotbar: &mut Hotbar) {
    let Inventory { main, carried, .. } = inventory;
    collect_matching_stacks(carried, &mut hotbar.slots);
    collect_matching_stacks(carried, main);
}

pub(super) fn collect_open_inventory(
    screen: &InventorySession,
    clicked: Slot,
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
) {
    if screen.furnace {
        if matches!(clicked, Slot::Furnace(_)) {
            if let Some(position) = screen.furnace_position
                && let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2)
            {
                collect_matching_stacks(&mut inventory.carried, &mut furnace.slots);
            }
        } else {
            collect_storage_stacks(inventory, hotbar);
        }
    } else {
        collect_player_stacks(inventory, hotbar, workbench, screen.workbench);
    }
}

pub(super) fn read_chest_group_slots(
    chunks: &WorldChunks,
    group: ChestGroup,
) -> Vec<Option<ItemStack>> {
    let mut slots = vec![None; group.slot_count()];
    if group.cart {
        if let Some(open) = &chunks.open_cart {
            slots.copy_from_slice(&open.slots);
        }
        return slots;
    }
    if group.dispenser {
        if let Some(dispenser) = chunks.dispenser_at(group.first.0, group.first.1, group.first.2) {
            slots.copy_from_slice(&dispenser.slots);
        }
        return slots;
    }
    if let Some(chest) = chunks.chest_at(group.first.0, group.first.1, group.first.2) {
        slots[..CHEST_HALF_SLOTS].copy_from_slice(&chest.slots);
    }
    if let Some((x, y, z)) = group.second
        && let Some(chest) = chunks.chest_at(x, y, z)
    {
        slots[CHEST_HALF_SLOTS..].copy_from_slice(&chest.slots);
    }
    slots
}

pub(super) fn write_chest_group_slots(
    chunks: &mut WorldChunks,
    group: ChestGroup,
    slots: &[Option<ItemStack>],
) {
    if group.cart {
        if let Some(open) = &mut chunks.open_cart {
            open.slots.copy_from_slice(slots);
        }
        return;
    }
    if group.dispenser {
        if let Some(dispenser) =
            chunks.dispenser_at_mut(group.first.0, group.first.1, group.first.2)
        {
            dispenser.slots.copy_from_slice(slots);
        }
        return;
    }
    if let Some(chest) = chunks.chest_at_mut(group.first.0, group.first.1, group.first.2) {
        chest.slots.copy_from_slice(&slots[..CHEST_HALF_SLOTS]);
    }
    if let Some((x, y, z)) = group.second
        && let Some(chest) = chunks.chest_at_mut(x, y, z)
    {
        chest.slots.copy_from_slice(&slots[CHEST_HALF_SLOTS..]);
    }
}

pub(super) fn mark_chest_dirty(
    persistence: &mut Option<ResMut<WorldPersistence>>,
    group: ChestGroup,
) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPosition::from_block(group.first.0, group.first.2));
        if let Some((x, _, z)) = group.second {
            persistence.mark_dirty(ChunkPosition::from_block(x, z));
        }
    }
}

//! Slot clicks for the player inventory, workbench and furnace: Beta's
//! `Container.slotClick` rules, drags, double clicks, and dropping.

use super::HOTBAR_KEYS;
use super::InventoryPanel;
use super::LastInventoryClick;
use super::Slot;
use super::SlotDrag;
use super::chest::collect_open_inventory;
use super::chest::handle_chest_slots;
use super::chest::mark_chest_dirty;
use super::chest::read_chest_group_slots;
use super::chest::write_chest_group_slots;
use super::is_double_click;
use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::entity::drops::items::spawn_thrown_item;
use crate::inventory::DragPlace;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::SlotId;
use crate::inventory::armor_slot_accepts;
use crate::inventory::drag_place;
use crate::inventory::hotbar_key_swap;
use crate::inventory::quick_move_drag_slot;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::shift_click_furnace_slot;
use crate::inventory::shift_click_slot;
use crate::inventory::slot_accepts_drag;
use crate::inventory::sort_main_inventory;
use crate::inventory::take_from_stack;
use crate::inventory::take_matching_stacks;
use crate::item::ItemStack;
use crate::player::Player;
use crate::random::ItemRng;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::PrimaryWindow;

pub(super) fn click_slot(
    target: &mut Option<ItemStack>,
    carried: &mut Option<ItemStack>,
    right: bool,
) {
    if right {
        match (*carried, *target) {
            (None, Some(stack)) => {
                let take = (stack.count() + 1) / 2;
                *carried = ItemStack::with_data(stack.item(), take, stack.data()).ok();
                *target =
                    ItemStack::with_data(stack.item(), stack.count() - take, stack.data()).ok();
            }
            (Some(stack), None) => {
                *target = ItemStack::with_data(stack.item(), 1, stack.data()).ok();
                *carried = ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
            }
            (Some(stack), Some(existing))
                if stack.item() == existing.item()
                    && stack.data() == existing.data()
                    && existing.count() < existing.definition().max_stack_size =>
            {
                *target =
                    ItemStack::with_data(existing.item(), existing.count() + 1, existing.data())
                        .ok();
                *carried = ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
            }
            _ => {}
        }
    } else {
        if let (Some(stack), Some(existing)) = (*carried, target.as_mut()) {
            if existing.item() == stack.item() && existing.data() == stack.data() {
                *carried = existing.merge(stack);
                return;
            }
        }
        std::mem::swap(target, carried);
    }
}

pub(super) fn take_workbench_result(
    session: &mut ActiveWorkbench,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
) -> bool {
    let grid = session.grid.clone();
    let Some(output) = beta_recipe_book().find(&grid) else {
        return false;
    };
    let mut simulated_grid = grid.clone();
    let Some(remainders) = beta_recipe_book().consume_one(&mut simulated_grid) else {
        return false;
    };
    let mut simulated_inventory = inventory.clone();
    let mut simulated_hotbar = hotbar.clone();
    for remainder in remainders {
        if simulated_inventory
            .insert(&mut simulated_hotbar, remainder)
            .is_some()
        {
            return false;
        }
    }
    match simulated_inventory.carried {
        None => simulated_inventory.carried = Some(output),
        Some(mut carried) if carried.item() == output.item() && carried.data() == output.data() => {
            if carried.merge(output).is_some() {
                return false;
            }
            simulated_inventory.carried = Some(carried);
        }
        Some(_) => return false,
    }
    session.grid = simulated_grid;
    *inventory = simulated_inventory;
    *hotbar = simulated_hotbar;
    true
}

pub(super) fn handle_slots(
    mut commands: Commands,
    mut item_rng: Local<ItemRng>,
    panels: Query<&RelativeCursorPosition, With<InventoryPanel>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    screen: Res<InventorySession>,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    slots: Query<(&RelativeCursorPosition, &Slot)>,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut workbench: ResMut<ActiveWorkbench>,
    mut chunks: ResMut<WorldChunks>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut drag: ResMut<SlotDrag>,
    mut last_click: ResMut<LastInventoryClick>,
) {
    if !screen.open {
        *drag = SlotDrag::default();
        *last_click = LastInventoryClick::default();
        return;
    }
    let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    let cursor_outside_panel = windows
        .single()
        .is_ok_and(|window| window.cursor_position().is_some())
        && panels.single().is_ok_and(|panel| !panel.cursor_over());
    let drop_input = DropInput {
        key: keys.just_pressed(KeyCode::KeyQ),
        left: cursor_outside_panel && mouse.just_pressed(MouseButton::Left),
        right: cursor_outside_panel && mouse.just_pressed(MouseButton::Right),
        shift: keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
        whole_stack: keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
    };
    if drag.button.is_none() && drop_input.any() {
        let hovered = slots
            .iter()
            .find_map(|(cursor, slot)| cursor.cursor_over().then_some(*slot));
        if drop_items(
            &mut commands,
            &mut item_rng,
            player_transform,
            &screen,
            hovered,
            drop_input,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
            &mut chunks,
            &mut persistence,
        ) {
            last_click.at = None;
            return;
        }
    }
    if screen.chest {
        handle_chest_slots(
            &screen,
            &mouse,
            &keys,
            &slots,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
            &mut chunks,
            &mut persistence,
            &mut drag,
            &mut last_click,
            time.elapsed_secs_f64(),
        );
        return;
    }
    let hovered = slots
        .iter()
        .find_map(|(cursor, slot)| cursor.cursor_over().then_some(*slot));
    if keys.just_pressed(KeyCode::KeyS) && drag.button.is_none() {
        if matches!(hovered, Some(Slot::Main(_))) {
            sort_main_inventory(&mut inventory);
        }
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if screen.furnace {
        let Some(position) = screen.furnace_position else {
            return;
        };
        if let Some(Slot::Furnace(index @ 0..=1)) = hovered {
            for (hotbar_index, key) in HOTBAR_KEYS.iter().enumerate() {
                if !keys.just_pressed(*key) {
                    continue;
                }
                if let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) {
                    std::mem::swap(&mut furnace.slots[index], &mut hotbar.slots[hotbar_index]);
                }
            }
        } else if let Some(slot) = hovered.filter(|slot| !matches!(slot, Slot::Furnace(_))) {
            for (index, key) in HOTBAR_KEYS.iter().enumerate() {
                if keys.just_pressed(*key) {
                    let _ = hotbar_key_swap(
                        &mut inventory,
                        &mut hotbar,
                        Some(&mut workbench.grid),
                        false,
                        to_slot_id(slot),
                        index,
                    );
                }
            }
        }
        if drag.button.is_none()
            && shift
            && mouse.just_pressed(MouseButton::Left)
            && let Some(slot) = hovered
        {
            drag.button = Some(MouseButton::Left);
            drag.origin = Some(slot);
            drag.slots.clear();
            drag.quick_move = true;
            drag.quick_move_visited.clear();
            last_click.at = None;
        }
        if drag.quick_move {
            if shift && let Some(slot) = hovered {
                let moved = if let Some(furnace) =
                    chunks.furnace_at_mut(position.0, position.1, position.2)
                {
                    quick_move_drag_slot(
                        &mut drag.quick_move_visited,
                        &mut inventory,
                        &mut hotbar,
                        None,
                        false,
                        None,
                        Some(&mut furnace.slots),
                        to_slot_id(slot),
                    )
                } else {
                    false
                };
                if moved && let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(position.0, position.2));
                }
            }
            if mouse.pressed(MouseButton::Left) {
                return;
            }
            *drag = SlotDrag::default();
            return;
        }
        if let Some(slot) = hovered
            && (mouse.just_pressed(MouseButton::Left) || mouse.just_pressed(MouseButton::Right))
        {
            let right =
                mouse.just_pressed(MouseButton::Right) && !mouse.just_pressed(MouseButton::Left);
            if !right
                && !shift
                && is_double_click(&mut last_click, &screen, slot, time.elapsed_secs_f64())
            {
                collect_open_inventory(
                    &screen,
                    slot,
                    &mut chunks,
                    &mut hotbar,
                    &mut inventory,
                    &mut workbench,
                );
                if matches!(slot, Slot::Furnace(_))
                    && let Some(persistence) = persistence.as_deref_mut()
                {
                    persistence.mark_dirty(ChunkPosition::from_block(position.0, position.2));
                }
                *drag = SlotDrag::default();
                return;
            }
            if right || shift {
                last_click.at = None;
            }
            if shift {
                shift_click_furnace(slot, position, &mut chunks, &mut hotbar, &mut inventory);
            } else {
                apply_furnace_click(
                    slot,
                    right,
                    position,
                    &mut chunks,
                    &mut hotbar,
                    &mut inventory,
                    &mut workbench,
                );
            }
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPosition::from_block(position.0, position.2));
            }
        } else if hovered.is_some() && HOTBAR_KEYS.iter().any(|key| keys.just_pressed(*key)) {
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPosition::from_block(position.0, position.2));
            }
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        for (index, key) in HOTBAR_KEYS.iter().enumerate() {
            if keys.just_pressed(*key) {
                *drag = SlotDrag::default();
                let _ = hotbar_key_swap(
                    &mut inventory,
                    &mut hotbar,
                    Some(&mut workbench.grid),
                    screen.workbench,
                    to_slot_id(slot),
                    index,
                );
            }
        }
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
        if !right
            && !shift
            && is_double_click(&mut last_click, &screen, slot, time.elapsed_secs_f64())
        {
            collect_open_inventory(
                &screen,
                slot,
                &mut chunks,
                &mut hotbar,
                &mut inventory,
                &mut workbench,
            );
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
            let _ = shift_click_slot(
                &mut inventory,
                &mut hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                to_slot_id(slot),
            );
            return;
        }
        if !drag.quick_move && inventory.carried.is_none() {
            apply_click(
                slot,
                right,
                screen.workbench,
                &mut hotbar,
                &mut inventory,
                &mut workbench,
            );
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
            let _ = quick_move_drag_slot(
                &mut drag.quick_move_visited,
                &mut inventory,
                &mut hotbar,
                Some(&mut workbench.grid),
                screen.workbench,
                None,
                None,
                to_slot_id(slot),
            );
        }
        if mouse.pressed(button) {
            return;
        }
        *drag = SlotDrag::default();
        return;
    }
    if let Some(slot) = hovered {
        remember_drag_slot(&mut drag, slot, &inventory, &hotbar, &workbench.grid);
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
        let _ = drag_place(
            &mut inventory,
            &mut hotbar,
            Some(&mut workbench.grid),
            &ids,
            mode,
        );
        return;
    }
    let click = hovered.or(if painted.len() == 1 {
        painted.first().copied()
    } else {
        origin
    });
    if let Some(slot) = click {
        apply_click(
            slot,
            mode == DragPlace::OneEach,
            screen.workbench,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct DropInput {
    /// The drop key was pressed this frame.
    pub(super) key: bool,
    /// A mouse button went down with the cursor outside the inventory image.
    pub(super) left: bool,
    pub(super) right: bool,
    pub(super) shift: bool,
    /// Control is held: the drop key takes the slot's whole stack.
    pub(super) whole_stack: bool,
}

impl DropInput {
    pub(super) fn any(self) -> bool {
        self.key || self.left || self.right
    }
}

/// Throws items out of the open inventory. The drop key takes one item from the
/// hovered slot, its whole stack with Control held, or every stack of that item
/// with Control and Shift held. A click outside the inventory image throws the carried stack
/// (left) or one item of it (right). Returns whether input was consumed.
#[allow(clippy::too_many_arguments)]
pub(super) fn drop_items(
    commands: &mut Commands,
    rng: &mut ItemRng,
    player: &Transform,
    screen: &InventorySession,
    hovered: Option<Slot>,
    input: DropInput,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
    chunks: &mut WorldChunks,
    persistence: &mut Option<ResMut<WorldPersistence>>,
) -> bool {
    let thrown: Vec<ItemStack> = if input.key {
        let Some(slot) = hovered else {
            return false;
        };
        if input.whole_stack && input.shift {
            take_all_matching(
                screen,
                slot,
                hotbar,
                inventory,
                workbench,
                chunks,
                persistence,
            )
        } else {
            let count = if input.whole_stack { u8::MAX } else { 1 };
            take_from_slot(
                screen,
                slot,
                count,
                hotbar,
                inventory,
                workbench,
                chunks,
                persistence,
            )
            .into_iter()
            .collect()
        }
    } else if input.shift || inventory.carried.is_none() || hovered.is_some() {
        return false;
    } else if input.left {
        inventory.carried.take().into_iter().collect()
    } else if input.right {
        take_from_stack(&mut inventory.carried, 1)
            .into_iter()
            .collect()
    } else {
        return false;
    };
    if thrown.is_empty() {
        // The key over an empty slot (or the result slot) is still handled.
        return input.key;
    }
    for stack in thrown {
        spawn_thrown_item(commands, rng, player, *player.forward(), stack);
    }
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPosition::from_block(
            player.translation.x.floor() as i32,
            player.translation.z.floor() as i32,
        ));
    }
    true
}

/// Every stack of the hovered slot's item: across the hotbar and main storage
/// for those slots, or across the container for a chest slot. Other slots give
/// up just their own stack.
pub(super) fn take_all_matching(
    screen: &InventorySession,
    slot: Slot,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
    chunks: &mut WorldChunks,
    persistence: &mut Option<ResMut<WorldPersistence>>,
) -> Vec<ItemStack> {
    match slot {
        Slot::Hotbar(_) | Slot::Main(_) => {
            let template = match slot {
                Slot::Hotbar(i) => hotbar.slots.get(i).copied().flatten(),
                Slot::Main(i) => inventory.main.get(i).copied().flatten(),
                _ => None,
            };
            let Some(template) = template else {
                return Vec::new();
            };
            take_matching_stacks(
                hotbar.slots.iter_mut().chain(inventory.main.iter_mut()),
                template,
            )
        }
        Slot::Chest(i) => {
            let Some(group) = screen.chest_group else {
                return Vec::new();
            };
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let Some(template) = chest_slots.get(i).copied().flatten() else {
                return Vec::new();
            };
            let taken = take_matching_stacks(chest_slots.iter_mut(), template);
            write_chest_group_slots(chunks, group, &chest_slots);
            mark_chest_dirty(persistence, group);
            taken
        }
        _ => take_from_slot(
            screen,
            slot,
            u8::MAX,
            hotbar,
            inventory,
            workbench,
            chunks,
            persistence,
        )
        .into_iter()
        .collect(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn take_from_slot(
    screen: &InventorySession,
    slot: Slot,
    count: u8,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
    chunks: &mut WorldChunks,
    persistence: &mut Option<ResMut<WorldPersistence>>,
) -> Option<ItemStack> {
    match slot {
        Slot::Hotbar(i) => take_from_stack(hotbar.slots.get_mut(i)?, count),
        Slot::Main(i) => take_from_stack(inventory.main.get_mut(i)?, count),
        Slot::Craft(i) => take_from_stack(inventory.crafting.get_mut(i)?, count),
        Slot::Armor(i) => take_from_stack(inventory.armor.get_mut(i)?, count),
        Slot::Workbench(i) if screen.workbench => {
            let (x, y) = (i % 3, i / 3);
            let mut value = workbench.grid.get(x, y);
            let taken = take_from_stack(&mut value, count);
            workbench.grid.set(x, y, value);
            taken
        }
        Slot::Furnace(i) if screen.furnace => {
            let position = screen.furnace_position?;
            let furnace = chunks.furnace_at_mut(position.0, position.1, position.2)?;
            let taken = take_from_stack(furnace.slots.get_mut(i)?, count);
            if taken.is_some()
                && let Some(persistence) = persistence.as_deref_mut()
            {
                persistence.mark_dirty(ChunkPosition::from_block(position.0, position.2));
            }
            taken
        }
        Slot::Chest(i) => {
            let group = screen.chest_group?;
            let mut chest_slots = read_chest_group_slots(chunks, group);
            let taken = take_from_stack(chest_slots.get_mut(i)?, count);
            if taken.is_some() {
                write_chest_group_slots(chunks, group, &chest_slots);
                mark_chest_dirty(persistence, group);
            }
            taken
        }
        // Throwing the result would skip consuming the ingredients.
        Slot::CraftResult | Slot::Workbench(_) | Slot::Furnace(_) => None,
    }
}

pub(super) fn remember_drag_slot(
    drag: &mut SlotDrag,
    slot: Slot,
    inventory: &Inventory,
    hotbar: &Hotbar,
    workbench: &CraftingGrid,
) {
    let Some(carried) = inventory.carried else {
        return;
    };
    if drag.slots.contains(&slot) || (carried.count() as usize) <= drag.slots.len() {
        return;
    }
    if slot_accepts_drag(
        inventory,
        hotbar,
        Some(workbench),
        to_slot_id(slot),
        carried,
    ) {
        drag.slots.push(slot);
    }
}

pub(super) fn to_slot_id(slot: Slot) -> SlotId {
    match slot {
        Slot::Hotbar(index) => SlotId::Hotbar(index),
        Slot::Main(index) => SlotId::Main(index),
        Slot::Craft(index) => SlotId::Craft(index),
        Slot::CraftResult => SlotId::CraftResult,
        Slot::Workbench(index) => SlotId::Workbench(index),
        Slot::Chest(index) => SlotId::Chest(index),
        Slot::Furnace(index) => SlotId::Furnace(index),
        Slot::Armor(index) => SlotId::Armor(index),
    }
}

pub(super) fn apply_furnace_click(
    slot: Slot,
    right: bool,
    position: (i32, i32, i32),
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
) {
    let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) else {
        return;
    };
    match slot {
        Slot::Furnace(index) if index < 2 => {
            click_slot(&mut furnace.slots[index], &mut inventory.carried, right);
        }
        Slot::Furnace(2) => {
            if inventory.carried.is_some() {
                return;
            }
            let Some(stack) = furnace.slots[2] else {
                return;
            };
            let count = if right {
                stack.count().div_ceil(2)
            } else {
                stack.count()
            };
            inventory.carried = ItemStack::with_data(stack.item(), count, stack.data()).ok();
            furnace.slots[2] =
                ItemStack::with_data(stack.item(), stack.count() - count, stack.data()).ok();
        }
        Slot::Furnace(_) => {}
        _ => apply_click(slot, right, false, hotbar, inventory, workbench),
    }
}

pub(super) fn shift_click_furnace(
    slot: Slot,
    position: (i32, i32, i32),
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
) -> bool {
    let Some(furnace) = chunks.furnace_at_mut(position.0, position.1, position.2) else {
        return false;
    };
    shift_click_furnace_slot(inventory, hotbar, &mut furnace.slots, to_slot_id(slot))
}

pub(super) fn apply_click(
    slot: Slot,
    right: bool,
    workbench_open: bool,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
) {
    match slot {
        Slot::Hotbar(i) => click_slot(&mut hotbar.slots[i], &mut inventory.carried, right),
        Slot::Main(i) => {
            let Inventory { main, carried, .. } = &mut *inventory;
            click_slot(&mut main[i], carried, right);
        }
        Slot::Craft(i) => {
            let Inventory {
                crafting, carried, ..
            } = &mut *inventory;
            click_slot(&mut crafting[i], carried, right);
        }
        Slot::CraftResult if !right => {
            if workbench_open {
                let _ = take_workbench_result(workbench, hotbar, inventory);
            } else if inventory.carried.is_none()
                || inventory.carried.is_some_and(|carried| {
                    inventory.crafting_result().is_some_and(|result| {
                        carried.item() == result.item()
                            && carried.data() == result.data()
                            && carried.count() + result.count()
                                <= carried.definition().max_stack_size
                    })
                })
            {
                let _ = inventory.take_crafting_result(hotbar);
            }
        }
        Slot::CraftResult => {}
        Slot::Workbench(i) => {
            let x = i % 3;
            let y = i / 3;
            let mut value = workbench.grid.get(x, y);
            click_slot(&mut value, &mut inventory.carried, right);
            workbench.grid.set(x, y, value);
        }
        Slot::Chest(_) => {}
        Slot::Armor(i) => {
            let Inventory { armor, carried, .. } = &mut *inventory;
            // `SlotArmor`: one piece, and only the kind worn in this slot.
            match *carried {
                None => click_slot(&mut armor[i], carried, false),
                Some(stack) if !armor_slot_accepts(i, stack) => {}
                // An empty slot takes one piece off the cursor.
                Some(_) if armor[i].is_none() => click_slot(&mut armor[i], carried, true),
                Some(stack) if stack.count() == 1 => click_slot(&mut armor[i], carried, false),
                Some(_) => {}
            }
        }
        Slot::Furnace(_) => {}
    }
}

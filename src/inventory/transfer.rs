//! Moving stacks between inventory slots.
//!
//! Shift-click follows Beta 1.7.3 `ContainerPlayer` and `ContainerWorkbench`:
//! the hotbar and main storage exchange stacks, crafting inputs move into
//! main storage and then the hotbar, and a crafting result is crafted again
//! until another output will not fit. Results fill the hotbar from the right,
//! then main storage from the bottom-right.
//!
//! Drag placement runs when the cursor is released over more than one accepting
//! slot. A left drag splits the carried stack evenly. A right drag places one
//! item in each slot. The remainder stays on the cursor.

use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::item::ItemStack;
use crate::world::chest::Chest;

use super::HOTBAR_SLOTS;
use super::Hotbar;
use super::Inventory;
use super::MAIN_SLOTS;

/// A slot the inventory screen can address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotId {
    Hotbar(usize),
    Main(usize),
    Craft(usize),
    CraftResult,
    Workbench(usize),
    Chest(usize),
    Armor(usize),
}

/// Shift-click between a single chest and player storage.
pub fn shift_click_chest_slot(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    chest: &mut Chest,
    slot: SlotId,
) -> bool {
    match slot {
        SlotId::Chest(index) if index < chest.slots.len() => {
            let Some(stack) = chest.slots[index].take() else {
                return false;
            };
            let before = stack.count();
            chest.slots[index] = place_stack(inventory, hotbar, stack, &player_slots_forward());
            chest.slots[index].is_none_or(|rest| rest.count() != before)
        }
        SlotId::Main(index) if index < inventory.main.len() => {
            let Some(stack) = inventory.main[index].take() else {
                return false;
            };
            let before = stack.count();
            let rest = place_in_chest(chest, stack);
            let moved = rest.is_none_or(|rest| rest.count() != before);
            inventory.main[index] = rest;
            moved
        }
        SlotId::Hotbar(index) if index < hotbar.slots.len() => {
            let Some(stack) = hotbar.slots[index].take() else {
                return false;
            };
            let before = stack.count();
            let rest = place_in_chest(chest, stack);
            let moved = rest.is_none_or(|rest| rest.count() != before);
            hotbar.slots[index] = rest;
            moved
        }
        _ => false,
    }
}

/// Swap a chest or player slot with a hotbar key destination.
pub fn hotbar_key_swap_chest(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    chest: &mut Chest,
    slot: SlotId,
    hotbar_index: usize,
) -> bool {
    if hotbar_index >= HOTBAR_SLOTS
        || matches!(slot, SlotId::Hotbar(index) if index == hotbar_index)
    {
        return false;
    }
    match slot {
        SlotId::Chest(index) => chest.slots.get_mut(index).is_some_and(|source| {
            std::mem::swap(source, &mut hotbar.slots[hotbar_index]);
            true
        }),
        SlotId::Main(index) => inventory.main.get_mut(index).is_some_and(|source| {
            std::mem::swap(source, &mut hotbar.slots[hotbar_index]);
            true
        }),
        SlotId::Hotbar(index) if index < hotbar.slots.len() => {
            hotbar.slots.swap(index, hotbar_index);
            true
        }
        _ => false,
    }
}

/// Whether a chest or player slot can receive a drag-painted stack.
pub fn chest_slot_accepts_drag(
    inventory: &Inventory,
    hotbar: &Hotbar,
    chest: &Chest,
    slot: SlotId,
    carried: ItemStack,
) -> bool {
    let existing = match slot {
        SlotId::Chest(index) => chest.slots.get(index).copied().flatten(),
        SlotId::Main(index) => inventory.main.get(index).copied().flatten(),
        SlotId::Hotbar(index) => hotbar.slots.get(index).copied().flatten(),
        _ => return false,
    };
    accepts_stack(existing, carried)
}

/// Preview drag placement when a chest is open.
pub fn preview_chest_drag_place(
    inventory: &Inventory,
    hotbar: &Hotbar,
    chest: &Chest,
    slots: &[SlotId],
    mode: DragPlace,
) -> Vec<(SlotId, ItemStack)> {
    let Some(carried) = inventory.carried else {
        return Vec::new();
    };
    let accepted = slots
        .iter()
        .copied()
        .filter(|slot| chest_slot_accepts_drag(inventory, hotbar, chest, *slot, carried))
        .fold(Vec::new(), |mut unique, slot| {
            if !unique.contains(&slot) {
                unique.push(slot);
            }
            unique
        });
    if accepted.len() < 2 || (carried.count() as usize) < accepted.len() {
        return Vec::new();
    }
    let share = match mode {
        DragPlace::Split => carried.count() / accepted.len() as u8,
        DragPlace::OneEach => 1,
    };
    if share == 0 {
        return Vec::new();
    }
    accepted
        .into_iter()
        .filter_map(|slot| {
            let current = read_chest_slot(inventory, hotbar, chest, slot)?;
            let target = (u16::from(share) + u16::from(current.map_or(0, ItemStack::count)))
                .min(u16::from(carried.definition().max_stack_size)) as u8;
            (target > current.map_or(0, ItemStack::count))
                .then(|| ItemStack::with_data(carried.item(), target, carried.data()).ok())
                .flatten()
                .map(|stack| (slot, stack))
        })
        .collect()
}

/// Apply a drag preview to chest/player storage and leave the remainder carried.
pub fn chest_drag_place(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    chest: &mut Chest,
    slots: &[SlotId],
    mode: DragPlace,
) -> bool {
    let preview = preview_chest_drag_place(inventory, hotbar, chest, slots, mode);
    let Some(carried) = inventory.carried else {
        return false;
    };
    if preview.is_empty() {
        return false;
    }
    let mut removed = 0u16;
    for (slot, stack) in preview {
        let already = read_chest_slot(inventory, hotbar, chest, slot)
            .flatten()
            .map_or(0, ItemStack::count);
        removed += u16::from(stack.count() - already);
        write_chest_slot(inventory, hotbar, chest, slot, Some(stack));
    }
    let left = u16::from(carried.count()).saturating_sub(removed);
    inventory.carried = (left > 0).then(|| {
        ItemStack::with_data(carried.item(), left as u8, carried.data())
            .expect("drag remainder stays within the stack limit")
    });
    true
}

fn accepts_stack(existing: Option<ItemStack>, carried: ItemStack) -> bool {
    match existing {
        None => true,
        Some(existing) => {
            existing.item() == carried.item()
                && existing.data() == carried.data()
                && existing.count() < existing.definition().max_stack_size
        }
    }
}

fn read_chest_slot(
    inventory: &Inventory,
    hotbar: &Hotbar,
    chest: &Chest,
    slot: SlotId,
) -> Option<Option<ItemStack>> {
    match slot {
        SlotId::Chest(index) => chest.slots.get(index).copied(),
        SlotId::Main(index) => inventory.main.get(index).copied(),
        SlotId::Hotbar(index) => hotbar.slots.get(index).copied(),
        _ => None,
    }
}

fn write_chest_slot(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    chest: &mut Chest,
    slot: SlotId,
    stack: Option<ItemStack>,
) {
    match slot {
        SlotId::Chest(index) => {
            if let Some(target) = chest.slots.get_mut(index) {
                *target = stack;
            }
        }
        SlotId::Main(index) => {
            if let Some(target) = inventory.main.get_mut(index) {
                *target = stack;
            }
        }
        SlotId::Hotbar(index) => {
            if let Some(target) = hotbar.slots.get_mut(index) {
                *target = stack;
            }
        }
        _ => {}
    }
}

fn place_in_chest(chest: &mut Chest, mut stack: ItemStack) -> Option<ItemStack> {
    if stack.definition().max_stack_size > 1 {
        for existing in chest.slots.iter_mut().flatten() {
            stack = existing.merge(stack)?;
        }
    }
    for slot in &mut chest.slots {
        if slot.is_none() {
            *slot = Some(stack);
            return None;
        }
    }
    Some(stack)
}

/// How a carried stack is painted across slots on mouse release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPlace {
    /// Left drag: each slot's added count is `floor(carried / slots)`.
    Split,
    /// Right drag: one item added to each slot.
    OneEach,
}

#[derive(Clone, Copy)]
enum Dest {
    Main(usize),
    Hotbar(usize),
}

enum OutputTarget {
    /// Beta result quick-move: hotbar from the right, then main from the end.
    ReversePlayer,
    HotbarSlot(usize),
}

const MAX_SHIFT_CRAFTS: usize = 64 * 9;

/// Shift-click `slot`, matching Beta `mergeItemStack` ranges.
///
/// Returns whether any item left the source slot.
pub fn shift_click_slot(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut workbench: Option<&mut CraftingGrid>,
    workbench_open: bool,
    slot: SlotId,
) -> bool {
    if matches!(slot, SlotId::CraftResult) {
        return shift_craft(inventory, hotbar, workbench, workbench_open);
    }
    let Some(stack) = take_slot(inventory, hotbar, workbench.as_deref_mut(), slot) else {
        return false;
    };
    let before = stack.count();
    let order = match slot {
        SlotId::Hotbar(_) => main_slots(),
        SlotId::Main(_) => hotbar_slots(),
        SlotId::Craft(_) | SlotId::Workbench(_) | SlotId::Chest(_) | SlotId::Armor(_) => {
            player_slots_forward()
        }
        SlotId::CraftResult => unreachable!("handled above"),
    };
    match place_stack(inventory, hotbar, stack, &order) {
        None => true,
        Some(rest) => {
            let moved = rest.count() != before;
            write_slot(inventory, hotbar, workbench, slot, Some(rest));
            moved
        }
    }
}

/// Pressing 1–9 over `slot` swaps it with that hotbar index.
///
/// A crafting result only moves into an empty hotbar slot, and does so once.
pub fn hotbar_key_swap(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    workbench: Option<&mut CraftingGrid>,
    workbench_open: bool,
    slot: SlotId,
    hotbar_index: usize,
) -> bool {
    if hotbar_index >= HOTBAR_SLOTS {
        return false;
    }
    if matches!(slot, SlotId::Hotbar(index) if index == hotbar_index) {
        return false;
    }
    if matches!(slot, SlotId::CraftResult) {
        if hotbar.slots[hotbar_index].is_some() {
            return false;
        }
        return craft_once(
            inventory,
            hotbar,
            workbench,
            workbench_open,
            OutputTarget::HotbarSlot(hotbar_index),
        );
    }
    let source = read_slot(inventory, hotbar, workbench.as_deref(), false, slot);
    let destination = hotbar.slots[hotbar_index];
    if source.is_none() && destination.is_none() {
        return false;
    }
    write_slot(inventory, hotbar, workbench, slot, destination);
    hotbar.slots[hotbar_index] = source;
    true
}

/// Place `slots` from the carried stack. Fewer than two accepting slots is a click,
/// not a drag, so this leaves the inventory unchanged and returns false.
pub fn drag_place(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut workbench: Option<&mut CraftingGrid>,
    slots: &[SlotId],
    mode: DragPlace,
) -> bool {
    let preview = preview_drag_place(inventory, hotbar, workbench.as_deref(), slots, mode);
    let Some(carried) = inventory.carried else {
        return false;
    };
    if preview.is_empty() {
        return false;
    }
    let mut removed = 0u16;
    for (slot, stack) in preview {
        let already = read_slot(inventory, hotbar, workbench.as_deref(), false, slot)
            .map_or(0, ItemStack::count);
        removed += u16::from(stack.count() - already);
        write_slot(
            inventory,
            hotbar,
            workbench.as_deref_mut(),
            slot,
            Some(stack),
        );
    }
    let left = u16::from(carried.count()).saturating_sub(removed);
    inventory.carried = (left > 0).then(|| {
        ItemStack::with_data(carried.item(), left as u8, carried.data())
            .expect("drag remainder stays within the stack limit")
    });
    true
}

/// Calculate the resulting stacks for a drag without changing inventory data.
/// Invalid slots and stacks that would not receive any items are omitted.
pub fn preview_drag_place(
    inventory: &Inventory,
    hotbar: &Hotbar,
    workbench: Option<&CraftingGrid>,
    slots: &[SlotId],
    mode: DragPlace,
) -> Vec<(SlotId, ItemStack)> {
    let Some(carried) = inventory.carried else {
        return Vec::new();
    };
    let mut accepted = Vec::new();
    for slot in slots {
        if accepted.contains(slot) {
            continue;
        }
        if slot_accepts_drag(inventory, hotbar, workbench, *slot, carried) {
            accepted.push(*slot);
        }
    }
    if accepted.len() < 2 || (carried.count() as usize) < accepted.len() {
        return Vec::new();
    }
    let share = match mode {
        DragPlace::Split => carried.count() / accepted.len() as u8,
        DragPlace::OneEach => 1,
    };
    if share == 0 {
        return Vec::new();
    }
    let max = carried.definition().max_stack_size;
    accepted
        .into_iter()
        .filter_map(|slot| {
            let already =
                read_slot(inventory, hotbar, workbench, false, slot).map_or(0, ItemStack::count);
            let target = (u16::from(share) + u16::from(already)).min(u16::from(max)) as u8;
            (target > already)
                .then(|| ItemStack::with_data(carried.item(), target, carried.data()).ok())
                .flatten()
                .map(|stack| (slot, stack))
        })
        .collect()
}

/// Empty slots accept the carried stack. A partial matching stack accepts more.
/// Crafting results and full or different stacks do not.
pub fn slot_accepts_drag(
    inventory: &Inventory,
    hotbar: &Hotbar,
    workbench: Option<&CraftingGrid>,
    slot: SlotId,
    carried: ItemStack,
) -> bool {
    if matches!(slot, SlotId::CraftResult) {
        return false;
    }
    match read_slot(inventory, hotbar, workbench, false, slot) {
        None => true,
        Some(existing) => {
            existing.item() == carried.item()
                && existing.data() == carried.data()
                && existing.count() < existing.definition().max_stack_size
        }
    }
}

fn shift_craft(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut workbench: Option<&mut CraftingGrid>,
    workbench_open: bool,
) -> bool {
    let mut crafted = false;
    for _ in 0..MAX_SHIFT_CRAFTS {
        if !craft_once(
            inventory,
            hotbar,
            workbench.as_deref_mut(),
            workbench_open,
            OutputTarget::ReversePlayer,
        ) {
            break;
        }
        crafted = true;
    }
    crafted
}

fn craft_once(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut workbench: Option<&mut CraftingGrid>,
    workbench_open: bool,
    target: OutputTarget,
) -> bool {
    if workbench_open {
        let Some(grid) = workbench.as_deref_mut() else {
            return false;
        };
        let mut slots: Vec<Option<ItemStack>> = grid.slots().collect();
        let crafted = simulate_craft(inventory, hotbar, &mut slots, 3, 3, target);
        if crafted {
            for (index, stack) in slots.into_iter().enumerate() {
                grid.set(index % 3, index / 3, stack);
            }
        }
        crafted
    } else {
        let mut slots = inventory.crafting;
        let crafted = simulate_craft(inventory, hotbar, &mut slots, 2, 2, target);
        inventory.crafting = slots;
        crafted
    }
}

/// Consume one craft on a copy. Commit only when the output and every container
/// remainder fit, so a failed craft leaves the grid untouched.
fn simulate_craft(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    slots: &mut [Option<ItemStack>],
    width: usize,
    height: usize,
    target: OutputTarget,
) -> bool {
    let mut simulated_inventory = inventory.clone();
    let mut simulated_hotbar = hotbar.clone();
    let mut grid = CraftingGrid::from_slots(width, height, slots);
    let Some(output) = beta_recipe_book().find(&grid) else {
        return false;
    };
    let Some(remainders) = beta_recipe_book().consume_one(&mut grid) else {
        return false;
    };
    let placed = match target {
        OutputTarget::ReversePlayer => place_stack(
            &mut simulated_inventory,
            &mut simulated_hotbar,
            output,
            &player_slots_reverse(),
        )
        .is_none(),
        OutputTarget::HotbarSlot(index) => {
            if simulated_hotbar.slots[index].is_some() {
                false
            } else {
                simulated_hotbar.slots[index] = Some(output);
                true
            }
        }
    };
    if !placed {
        return false;
    }
    for remainder in remainders {
        if simulated_inventory
            .insert(&mut simulated_hotbar, remainder)
            .is_some()
        {
            return false;
        }
    }
    let pop = hotbar.pop;
    *inventory = simulated_inventory;
    *hotbar = simulated_hotbar;
    hotbar.pop = pop;
    for (index, stack) in grid.slots().enumerate() {
        slots[index] = stack;
    }
    true
}

fn place_stack(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut stack: ItemStack,
    order: &[Dest],
) -> Option<ItemStack> {
    if stack.definition().max_stack_size > 1 {
        for dest in order {
            let Some(mut existing) = read_dest(inventory, hotbar, *dest) else {
                continue;
            };
            match existing.merge(stack) {
                Some(rest) => {
                    write_dest(inventory, hotbar, *dest, Some(existing));
                    stack = rest;
                }
                None => {
                    write_dest(inventory, hotbar, *dest, Some(existing));
                    return None;
                }
            }
        }
    }
    for dest in order {
        if read_dest(inventory, hotbar, *dest).is_none() {
            write_dest(inventory, hotbar, *dest, Some(stack));
            return None;
        }
    }
    Some(stack)
}

fn main_slots() -> Vec<Dest> {
    (0..MAIN_SLOTS).map(Dest::Main).collect()
}

fn hotbar_slots() -> Vec<Dest> {
    (0..HOTBAR_SLOTS).map(Dest::Hotbar).collect()
}

fn player_slots_forward() -> Vec<Dest> {
    main_slots().into_iter().chain(hotbar_slots()).collect()
}

fn player_slots_reverse() -> Vec<Dest> {
    (0..HOTBAR_SLOTS)
        .rev()
        .map(Dest::Hotbar)
        .chain((0..MAIN_SLOTS).rev().map(Dest::Main))
        .collect()
}

fn read_dest(inventory: &Inventory, hotbar: &Hotbar, dest: Dest) -> Option<ItemStack> {
    match dest {
        Dest::Main(index) => inventory.main[index],
        Dest::Hotbar(index) => hotbar.slots[index],
    }
}

fn write_dest(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    dest: Dest,
    stack: Option<ItemStack>,
) {
    match dest {
        Dest::Main(index) => inventory.main[index] = stack,
        Dest::Hotbar(index) => hotbar.slots[index] = stack,
    }
}

fn read_slot(
    inventory: &Inventory,
    hotbar: &Hotbar,
    workbench: Option<&CraftingGrid>,
    workbench_open: bool,
    slot: SlotId,
) -> Option<ItemStack> {
    match slot {
        SlotId::Hotbar(index) => hotbar.slots.get(index).copied().flatten(),
        SlotId::Main(index) => inventory.main.get(index).copied().flatten(),
        SlotId::Craft(index) => inventory.crafting.get(index).copied().flatten(),
        SlotId::Armor(index) => inventory.armor.get(index).copied().flatten(),
        SlotId::Workbench(index) if index < 9 => {
            workbench.and_then(|grid| grid.get(index % 3, index / 3))
        }
        SlotId::Workbench(_) => None,
        SlotId::Chest(_) => None,
        SlotId::CraftResult if workbench_open => {
            workbench.and_then(|grid| beta_recipe_book().find(grid))
        }
        SlotId::CraftResult => inventory.crafting_result(),
    }
}

fn write_slot(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    mut workbench: Option<&mut CraftingGrid>,
    slot: SlotId,
    stack: Option<ItemStack>,
) {
    match slot {
        SlotId::Hotbar(index) => {
            if let Some(entry) = hotbar.slots.get_mut(index) {
                *entry = stack;
            }
        }
        SlotId::Main(index) => {
            if let Some(entry) = inventory.main.get_mut(index) {
                *entry = stack;
            }
        }
        SlotId::Craft(index) => {
            if let Some(entry) = inventory.crafting.get_mut(index) {
                *entry = stack;
            }
        }
        SlotId::Armor(index) => {
            if let Some(entry) = inventory.armor.get_mut(index) {
                *entry = stack;
            }
        }
        SlotId::Workbench(index) if index < 9 => {
            if let Some(grid) = workbench.as_deref_mut() {
                grid.set(index % 3, index / 3, stack);
            }
        }
        SlotId::Workbench(_) | SlotId::Chest(_) | SlotId::CraftResult => {}
    }
}

fn take_slot(
    inventory: &mut Inventory,
    hotbar: &mut Hotbar,
    workbench: Option<&mut CraftingGrid>,
    slot: SlotId,
) -> Option<ItemStack> {
    let stack = read_slot(inventory, hotbar, workbench.as_deref(), false, slot)?;
    write_slot(inventory, hotbar, workbench, slot, None);
    Some(stack)
}

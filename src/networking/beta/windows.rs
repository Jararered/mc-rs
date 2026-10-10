//! `Container` and its subclasses: the windows a Beta client has open, the
//! clicks it makes in them, and keeping its copy of their slots right.
//!
//! The client plays every click out on its own copy first and tells the
//! server what it clicked on. [`Windows::click`] runs Beta's
//! `Container.slotClick` on the real slots; when that picked up what the
//! client said it would the click is accepted with a `Packet106Transaction`,
//! and otherwise it is refused and the whole window sent again.
//!
//! A window's slots are gathered from wherever they live (the player's
//! inventory, a chest's or furnace's block, a cart's cargo), clicked on as one
//! list in the order the client numbers them, and written back.

use bevy::prelude::*;

use super::codec::WireStack;
use super::codec::Writer;
use crate::crafting::CraftingGrid;
use crate::crafting::beta_recipe_book;
use crate::entity::minecart::Cargo;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::armor_slot_accepts;
use crate::item::ItemStack;
use crate::item::registry::ItemData;
use crate::player::actions::Window;
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;

type Slot = Option<ItemStack>;

/// What a window other than the player's own is a view of.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Container {
    /// `ContainerPlayer`, window 0, which is never opened or closed.
    Player,
    /// `ContainerWorkbench`. Its grid belongs to the window and is spilled
    /// when it closes.
    Workbench { grid: [Slot; 9], position: IVec3 },
    /// `ContainerChest` over a chest, a double chest, or (as
    /// `ContainerDispenser`) a dispenser.
    Chest { group: ChestGroup },
    /// `ContainerChest` over a chest cart.
    Cart { cart: Entity },
    /// `ContainerFurnace`.
    Furnace { position: IVec3 },
}

/// How a slot takes items (`Slot.isItemValid`, `getSlotStackLimit`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    Normal,
    /// `SlotCrafting`: taking from it uses the grid up.
    Result,
    /// `SlotFurnace`: the output, which nothing can be put in.
    Output,
    /// `SlotArmor`, numbered from the helmet.
    Armor(usize),
}

impl Container {
    /// The slots that are not the player's main inventory and hotbar.
    fn own_slots(&self) -> usize {
        match self {
            Self::Player => 9,
            Self::Workbench { .. } => 10,
            Self::Chest { group } => group.slot_count(),
            Self::Cart { .. } => 27,
            Self::Furnace { .. } => 3,
        }
    }

    /// The crafting grid's slots and width, when slot 0 is its result.
    fn grid(&self) -> Option<(std::ops::Range<usize>, usize)> {
        match self {
            Self::Player => Some((1..5, 2)),
            Self::Workbench { .. } => Some((1..10, 3)),
            _ => None,
        }
    }

    fn kind(&self, slot: usize) -> SlotKind {
        match self {
            Self::Player | Self::Workbench { .. } if slot == 0 => SlotKind::Result,
            Self::Player if (5..9).contains(&slot) => SlotKind::Armor(slot - 5),
            Self::Furnace { .. } if slot == 2 => SlotKind::Output,
            _ => SlotKind::Normal,
        }
    }

    /// `func_27086_a`'s choice of where a shift-click sends a slot: the range
    /// to merge into, and whether it is filled from the far end.
    fn shift_target(&self, slot: usize) -> (usize, usize, bool) {
        let own = self.own_slots();
        let end = own + 36;
        match self {
            Self::Chest { .. } | Self::Cart { .. } => {
                if slot < own {
                    (own, end, true)
                } else {
                    (0, own, false)
                }
            }
            // The result or the furnace's output goes anywhere in the
            // inventory, the main inventory and the hotbar swap, and
            // anything else goes to either.
            _ => {
                let special = match self {
                    Self::Furnace { .. } => 2,
                    _ => 0,
                };
                if slot == special {
                    (own, end, true)
                } else if (own..own + 27).contains(&slot) {
                    (own + 27, end, false)
                } else if (own + 27..end).contains(&slot) {
                    (own, own + 27, false)
                } else {
                    (own, end, false)
                }
            }
        }
    }

    /// `Packet100OpenWindow`'s type, title and slot count.
    fn opening(&self) -> (u8, &'static str, u8) {
        match self {
            Self::Player => (0, "", 0),
            Self::Workbench { .. } => (1, "Crafting", 9),
            Self::Chest { group } if group.dispenser => (3, "Trap", 9),
            Self::Chest { group } if group.is_double() => (0, "Large chest", 54),
            Self::Chest { .. } => (0, "Chest", 27),
            Self::Cart { .. } => (0, "Minecart", 27),
            Self::Furnace { .. } => (2, "Furnace", 3),
        }
    }

    /// The block a window is of, which its player has to stay near.
    fn anchor(&self) -> Option<IVec3> {
        match self {
            Self::Workbench { position, .. } | Self::Furnace { position } => Some(*position),
            Self::Chest { group } => Some(IVec3::from(group.first)),
            Self::Player | Self::Cart { .. } => None,
        }
    }
}

fn wire(stack: Slot) -> Option<WireStack> {
    stack.map(|stack| {
        (
            stack.item().as_u16() as i16,
            stack.count() as i8,
            stack.data() as i16,
        )
    })
}

/// `itemID` equal and, for an item with subtypes, the same one: what Beta
/// asks before stacking. Two tools of different wear count as the same.
fn alike(a: ItemStack, b: ItemStack) -> bool {
    a.item() == b.item()
        && (!matches!(a.definition().data, ItemData::Subtype(_) | ItemData::Map)
            || a.data() == b.data())
}

/// Every slot of `container` for `player`, in the client's numbering, or
/// `None` when what it is a view of has gone.
fn read(world: &World, player: Entity, container: &Container) -> Option<Vec<Slot>> {
    let inventory = world.get::<Inventory>(player)?;
    let hotbar = world.get::<Hotbar>(player)?;
    let chunks = world.resource::<WorldChunks>();
    let mut slots: Vec<Slot> = Vec::with_capacity(container.own_slots() + 36);
    match container {
        Container::Player => {
            slots.push(None);
            slots.extend(inventory.crafting);
            slots.extend(inventory.armor);
        }
        Container::Workbench { grid, .. } => {
            slots.push(None);
            slots.extend(grid);
        }
        Container::Chest { group } => {
            let (x, y, z) = group.first;
            if group.dispenser {
                slots.extend(chunks.dispenser_at(x, y, z)?.slots);
            } else {
                slots.extend(chunks.chest_at(x, y, z)?.slots);
                if let Some((x, y, z)) = group.second {
                    slots.extend(chunks.chest_at(x, y, z)?.slots);
                }
            }
        }
        Container::Cart { cart } => slots.extend(world.get::<Cargo>(*cart)?.0),
        Container::Furnace { position } => {
            slots.extend(
                chunks
                    .furnace_at(position.x, position.y, position.z)?
                    .slots
                    .clone(),
            );
        }
    }
    slots.extend(inventory.main);
    slots.extend(hotbar.slots);
    refresh_result(container, &mut slots);
    Some(slots)
}

/// `onCraftMatrixChanged`: the result slot shows what the grid makes.
fn refresh_result(container: &Container, slots: &mut [Slot]) {
    if let Some((grid, width)) = container.grid() {
        slots[0] = beta_recipe_book().find(&CraftingGrid::from_slots(width, width, &slots[grid]));
    }
}

/// Put `after` back where [`read`] got `before` from.
fn write(
    world: &mut World,
    player: Entity,
    container: &mut Container,
    before: &[Slot],
    after: &[Slot],
) {
    let own = container.own_slots();
    if before[..own] != after[..own] {
        let mut dirty = Vec::new();
        match container {
            Container::Player => {
                if let Some(mut inventory) = world.get_mut::<Inventory>(player) {
                    inventory.crafting.copy_from_slice(&after[1..5]);
                    inventory.armor.copy_from_slice(&after[5..9]);
                }
            }
            Container::Workbench { grid, .. } => grid.copy_from_slice(&after[1..10]),
            Container::Chest { group } => {
                let mut chunks = world.resource_mut::<WorldChunks>();
                let (x, y, z) = group.first;
                dirty.push((x, z));
                if group.dispenser {
                    if let Some(dispenser) = chunks.dispenser_at_mut(x, y, z) {
                        dispenser.slots.copy_from_slice(&after[..9]);
                    }
                } else {
                    if let Some(chest) = chunks.chest_at_mut(x, y, z) {
                        chest.slots.copy_from_slice(&after[..27]);
                    }
                    if let Some((x, y, z)) = group.second {
                        dirty.push((x, z));
                        if let Some(chest) = chunks.chest_at_mut(x, y, z) {
                            chest.slots.copy_from_slice(&after[27..54]);
                        }
                    }
                }
            }
            Container::Cart { cart } => {
                if let Some(mut cargo) = world.get_mut::<Cargo>(*cart) {
                    cargo.0.copy_from_slice(&after[..27]);
                }
            }
            Container::Furnace { position } => {
                dirty.push((position.x, position.z));
                if let Some(furnace) = world
                    .resource_mut::<WorldChunks>()
                    .furnace_at_mut(position.x, position.y, position.z)
                {
                    furnace.slots.copy_from_slice(&after[..3]);
                }
            }
        }
        if let Some(mut persistence) = world.get_resource_mut::<WorldPersistence>() {
            for (x, z) in dirty {
                persistence.mark_dirty(ChunkPosition::from_block(x, z));
            }
        }
    }
    if before[own..own + 27] != after[own..own + 27]
        && let Some(mut inventory) = world.get_mut::<Inventory>(player)
    {
        inventory.main.copy_from_slice(&after[own..own + 27]);
    }
    if before[own + 27..] != after[own + 27..]
        && let Some(mut hotbar) = world.get_mut::<Hotbar>(player)
    {
        hotbar.slots.copy_from_slice(&after[own + 27..]);
    }
}

/// A window's slots while a click plays out.
struct Clicking<'a> {
    container: &'a Container,
    slots: Vec<Slot>,
    /// `InventoryPlayer.itemStack`: the stack on the cursor.
    carried: Slot,
    /// Thrown out of the window.
    dropped: Vec<ItemStack>,
}

impl Clicking<'_> {
    fn limit(&self, slot: usize) -> u8 {
        match self.container.kind(slot) {
            SlotKind::Armor(_) => 1,
            _ => 64,
        }
    }

    fn accepts(&self, slot: usize, stack: ItemStack) -> bool {
        match self.container.kind(slot) {
            SlotKind::Normal => true,
            SlotKind::Result | SlotKind::Output => false,
            SlotKind::Armor(index) => armor_slot_accepts(index, stack),
        }
    }

    /// `Slot.decrStackSize`: up to `count` out of `slot`. A crafting result
    /// always comes out whole.
    fn take(&mut self, slot: usize, count: u8) -> Slot {
        let stack = self.slots[slot]?;
        if self.container.kind(slot) == SlotKind::Result || count >= stack.count() {
            self.slots[slot] = None;
            return Some(stack);
        }
        self.slots[slot] = stack.with_count(stack.count() - count).ok();
        stack.with_count(count).ok()
    }

    /// `Slot.onPickupFromSlot`: taking a crafting result uses up one of
    /// everything in the grid.
    fn picked_up(&mut self, slot: usize) {
        if self.container.kind(slot) != SlotKind::Result {
            return;
        }
        let Some((range, width)) = self.container.grid() else {
            return;
        };
        let mut grid = CraftingGrid::from_slots(width, width, &self.slots[range.clone()]);
        // The result slot was emptied by the pickup, so ask the grid itself.
        if let Some(remainders) = beta_recipe_book().consume_one(&mut grid) {
            for (slot, left) in self.slots[range].iter_mut().zip(grid.slots()) {
                *slot = left;
            }
            self.dropped.extend(remainders);
        }
        refresh_result(self.container, &mut self.slots);
    }

    /// `func_28126_a`: merge `stack` into the slots from `start` to `end`,
    /// topping up stacks of it and then taking the first empty slot. Returns
    /// what did not fit.
    fn merge(&mut self, stack: ItemStack, start: usize, end: usize, reverse: bool) -> Slot {
        let order = |index: usize| {
            if reverse {
                end - 1 - index
            } else {
                start + index
            }
        };
        let mut rest = Some(stack);
        if stack.definition().max_stack_size > 1 {
            for index in 0..end - start {
                let Some(moving) = rest else { break };
                if let Some(target) = self.slots[order(index)].as_mut() {
                    rest = target.merge(moving);
                }
            }
        }
        if let Some(moving) = rest {
            for index in 0..end - start {
                let slot = &mut self.slots[order(index)];
                if slot.is_none() {
                    *slot = Some(moving);
                    return None;
                }
            }
        }
        rest
    }

    /// `func_27086_a`, a shift-click: send `slot`'s stack to the other part
    /// of the window. Returns the stack as it was, or `None` when nothing
    /// came of it.
    fn transfer(&mut self, slot: usize) -> Slot {
        let stack = self.slots[slot]?;
        let (start, end, reverse) = self.container.shift_target(slot);
        let kind = self.container.kind(slot);
        if kind == SlotKind::Result {
            // A result that does not all fit stays uncrafted.
            let saved = self.slots.clone();
            if self.merge(stack, start, end, reverse).is_some() {
                self.slots = saved;
                return None;
            }
            self.slots[slot] = None;
            self.picked_up(slot);
            return Some(stack);
        }
        let rest = self.merge(stack, start, end, reverse);
        self.slots[slot] = rest;
        refresh_result(self.container, &mut self.slots);
        let chest = matches!(
            self.container,
            Container::Chest { .. } | Container::Cart { .. }
        );
        if !chest && rest.is_some_and(|rest| rest.count() == stack.count()) {
            return None;
        }
        Some(stack)
    }

    /// `Container.func_27085_a`, `slotClick`. Returns what the client is
    /// expected to have seen in the slot.
    fn click(&mut self, slot: i16, button: i8, shift: bool) -> Slot {
        if button != 0 && button != 1 {
            return None;
        }
        if slot == -999 {
            // Outside the window: the cursor's stack is thrown, or one of it.
            if let Some(carried) = self.carried {
                if button == 0 || carried.count() == 1 {
                    self.dropped.push(carried);
                    self.carried = None;
                } else {
                    self.dropped.extend(carried.with_count(1).ok());
                    self.carried = carried.with_count(carried.count() - 1).ok();
                }
            }
            return None;
        }
        let index = usize::try_from(slot)
            .ok()
            .filter(|index| *index < self.slots.len())?;
        if shift {
            let moved = self.transfer(index)?;
            // What stayed behind is tried once more.
            if self.slots[index].is_some_and(|left| left.count() < moved.count()) {
                self.click(slot, button, shift);
            }
            return Some(moved);
        }

        let before = self.slots[index];
        match (before, self.carried) {
            (None, None) => {}
            (None, Some(carried)) => {
                if self.accepts(index, carried) {
                    let count = if button == 0 { carried.count() } else { 1 };
                    let count = count.min(self.limit(index));
                    self.slots[index] = carried.with_count(count).ok();
                    self.carried = carried.with_count(carried.count() - count).ok();
                }
            }
            (Some(stack), None) => {
                let count = if button == 0 {
                    stack.count()
                } else {
                    stack.count().div_ceil(2)
                };
                self.carried = self.take(index, count);
                self.picked_up(index);
            }
            (Some(stack), Some(carried)) if self.accepts(index, carried) => {
                if alike(stack, carried) {
                    let room = stack.definition().max_stack_size.min(self.limit(index));
                    let count = if button == 0 { carried.count() } else { 1 };
                    let count = count.min(room.saturating_sub(stack.count()));
                    if count > 0 {
                        self.slots[index] = stack.with_count(stack.count() + count).ok();
                        self.carried = carried.with_count(carried.count() - count).ok();
                    }
                } else if carried.count() <= self.limit(index) {
                    self.slots[index] = Some(carried);
                    self.carried = Some(stack);
                }
            }
            // A result or an output: more of what the cursor holds is added
            // to it when all of it fits.
            (Some(stack), Some(carried)) => {
                let max = carried.definition().max_stack_size;
                if alike(stack, carried) && max > 1 && stack.count() + carried.count() <= max {
                    self.carried = carried.with_count(carried.count() + stack.count()).ok();
                    self.slots[index] = None;
                    self.picked_up(index);
                }
            }
        }
        refresh_result(self.container, &mut self.slots);
        before
    }
}

/// The windows one client has: its inventory, and at most one other.
#[derive(Default)]
pub(super) struct Windows {
    /// `currentWindowId`, 1 to 100.
    last_id: i8,
    open: Option<(i8, Container)>,
    /// The inventory window as the client has it, or empty to send it whole.
    inventory: Vec<Option<WireStack>>,
    /// The open window as the client has it.
    contents: Vec<Option<WireStack>>,
    cursor: Option<WireStack>,
    /// A furnace's cook time, burn time and fuel time, as sent.
    bars: [i16; 3],
    /// A click was refused, and the client has not said it saw that yet:
    /// until it does its clicks are ignored (`setCanCraft`).
    refused: Option<(i8, i16)>,
}

impl Windows {
    /// Send the client everything again: it has changed its own copy in a
    /// way the server did not follow.
    pub fn resend(&mut self) {
        self.inventory.clear();
        self.contents.clear();
    }

    /// The window `id` names, if it is the one the client has in front of
    /// it (`currentCraftingInventory`).
    fn container(&self, id: i8) -> Option<Container> {
        match &self.open {
            Some((open, container)) => (*open == id).then(|| container.clone()),
            None => (id == 0).then_some(Container::Player),
        }
    }

    fn store(&mut self, id: i8, container: Container) {
        if let Some((open, held)) = &mut self.open
            && *open == id
        {
            *held = container;
        }
    }

    /// The world opened a container for the player (`displayGUIChest` and
    /// its like). Returns what closing the last window threw out.
    pub fn open(
        &mut self,
        world: &mut World,
        player: Entity,
        window: Window,
        out: &mut Writer,
    ) -> Vec<ItemStack> {
        let dropped = self.close(world, player);
        let container = match window {
            Window::Workbench { position } => Container::Workbench {
                grid: [None; 9],
                position,
            },
            Window::Furnace { position } => Container::Furnace { position },
            Window::Chest { group, .. } => Container::Chest { group },
            Window::CartChest { cart, .. } => {
                // The checked-out copy the game's own screen edits is not
                // used here: the cart's cargo is edited in place.
                world.resource_mut::<WorldChunks>().open_cart = None;
                Container::Cart { cart }
            }
        };
        self.last_id = self.last_id % 100 + 1;
        let (kind, title, slots) = container.opening();
        out.open_window(self.last_id, kind, title, slots);
        self.open = Some((self.last_id, container));
        self.contents.clear();
        self.bars = [-1; 3];
        dropped
    }

    /// `closeCraftingGui`: the cursor's stack and anything left in a
    /// crafting grid are thrown out, and the inventory is all that is open.
    pub fn close(&mut self, world: &mut World, player: Entity) -> Vec<ItemStack> {
        let mut dropped = Vec::new();
        if let Some(mut inventory) = world.get_mut::<Inventory>(player) {
            dropped.extend(inventory.carried.take());
            dropped.extend(inventory.crafting.iter_mut().filter_map(Option::take));
        }
        if let Some((_, Container::Workbench { grid, .. })) = self.open.take() {
            dropped.extend(grid.into_iter().flatten());
        }
        self.contents.clear();
        self.refused = None;
        dropped
    }

    /// `Packet106Transaction` from the client: it has seen a refusal.
    pub fn acknowledge(&mut self, window: i8, action: i16) {
        if self.refused == Some((window, action)) {
            self.refused = None;
        }
    }

    /// `Packet102WindowClick`. Returns what the click threw out.
    #[allow(clippy::too_many_arguments)]
    pub fn click(
        &mut self,
        world: &mut World,
        player: Entity,
        window: i8,
        slot: i16,
        button: i8,
        action: i16,
        shift: bool,
        expected: Option<WireStack>,
        out: &mut Writer,
    ) -> Vec<ItemStack> {
        if self.refused.is_some() {
            return Vec::new();
        }
        let Some(mut container) = self.container(window) else {
            return Vec::new();
        };
        let Some(before) = read(world, player, &container) else {
            return Vec::new();
        };
        let carried = world
            .get::<Inventory>(player)
            .and_then(|inventory| inventory.carried);
        let mut clicking = Clicking {
            container: &container,
            slots: before.clone(),
            carried,
            dropped: Vec::new(),
        };
        let seen = clicking.click(slot, button, shift);
        let Clicking {
            slots,
            carried,
            dropped,
            ..
        } = clicking;
        write(world, player, &mut container, &before, &slots);
        if let Some(mut inventory) = world.get_mut::<Inventory>(player) {
            inventory.carried = carried;
        }
        self.store(window, container);

        if wire(seen) == expected {
            out.transaction(window, action, true);
            // The client has already done the same to its own copy
            // (`isChangingQuantityOnly`), so none of it is sent back.
            self.cursor = wire(carried);
            let sent: Vec<_> = slots.iter().map(|slot| wire(*slot)).collect();
            if window == 0 {
                self.inventory = sent;
            } else {
                // The player's own slots are in both windows.
                if self.inventory.len() == 45 {
                    let own = slots.len() - 36;
                    self.inventory[9..].copy_from_slice(&sent[own..]);
                }
                self.contents = sent;
            }
        } else {
            out.transaction(window, action, false);
            self.refused = Some((window, action));
            out.set_slot(-1, -1, wire(carried));
            self.cursor = wire(carried);
            if window == 0 {
                self.inventory.clear();
            } else {
                self.contents.clear();
            }
        }
        dropped
    }

    /// `updateCraftingMatrix`: tell the client which slots are no longer
    /// what it was last sent, and close a window whose block has gone or
    /// been walked away from. Returns what such a closing threw out.
    pub fn sync(
        &mut self,
        world: &mut World,
        player: Entity,
        eyes: Vec3,
        out: &mut Writer,
    ) -> Vec<ItemStack> {
        let mut dropped = Vec::new();
        if let Some((id, container)) = self.open.clone() {
            let near = container.anchor().is_none_or(|block| {
                (block.as_vec3() + Vec3::splat(0.5)).distance_squared(eyes) <= 64.0
            });
            if let Some(slots) = read(world, player, &container).filter(|_| near) {
                let now: Vec<_> = slots.iter().map(|slot| wire(*slot)).collect();
                if self.contents.len() == now.len() {
                    for (index, slot) in now.iter().enumerate() {
                        if *slot != self.contents[index] {
                            out.set_slot(id, index as i16, *slot);
                        }
                    }
                } else {
                    out.window_items(id, &now);
                }
                // The inventory window shares the player's slots, so it does
                // not need telling about them twice.
                if self.inventory.len() == 45 {
                    let own = now.len() - 36;
                    self.inventory[9..].copy_from_slice(&now[own..]);
                }
                self.contents = now;
                if let Container::Furnace { position } = container
                    && let Some(furnace) = world
                        .resource::<WorldChunks>()
                        .furnace_at(position.x, position.y, position.z)
                {
                    let bars = [furnace.cook_ticks, furnace.burn_ticks, furnace.fuel_ticks]
                        .map(|ticks| ticks.min(i16::MAX as u16) as i16);
                    for (bar, value) in bars.into_iter().enumerate() {
                        if value != self.bars[bar] {
                            out.progress_bar(id, bar as i16, value);
                        }
                    }
                    self.bars = bars;
                }
            } else {
                out.close_window(id);
                dropped = self.close(world, player);
            }
        }

        if let Some(slots) = read(world, player, &Container::Player) {
            let now: Vec<_> = slots.iter().map(|slot| wire(*slot)).collect();
            if self.inventory.len() == now.len() {
                for (index, slot) in now.iter().enumerate() {
                    if *slot != self.inventory[index] {
                        out.set_slot(0, index as i16, *slot);
                    }
                }
            } else {
                out.window_items(0, &now);
            }
            self.inventory = now;
        }
        let cursor = wire(
            world
                .get::<Inventory>(player)
                .and_then(|inventory| inventory.carried),
        );
        if cursor != self.cursor {
            out.set_slot(-1, -1, cursor);
            self.cursor = cursor;
        }
        dropped
    }
}

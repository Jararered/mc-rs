//! Active inventory/container sessions, shared by interactions and the GUI.

use bevy::prelude::*;

use crate::crafting::CraftingGrid;
use crate::entity::drops::items::spawn_thrown_item;
use crate::item::ItemStack;
use crate::random::ItemRng;
use crate::world::chunk::ChestGroup;

use super::Hotbar;
use super::Inventory;

/// Active container and interaction state; widgets are owned by the GUI.
#[derive(Resource, Default)]
pub struct InventorySession {
    pub open: bool,
    pub workbench: bool,
    pub furnace: bool,
    pub furnace_position: Option<(i32, i32, i32)>,
    pub chest: bool,
    pub chest_position: Option<(i32, i32, i32)>,
    pub chest_group: Option<ChestGroup>,
}

/// Reusable workbench inputs and the position of the currently open table.
#[derive(Resource)]
pub struct ActiveWorkbench {
    pub grid: CraftingGrid,
    pub position: Option<(i32, i32, i32)>,
}

impl Default for ActiveWorkbench {
    fn default() -> Self {
        Self {
            grid: CraftingGrid::workbench(),
            position: None,
        }
    }
}

/// Return all session inputs and carried items, dropping inventory overflow.
pub fn close_crafting_session(
    commands: &mut Commands,
    player: &Transform,
    rng: &mut ItemRng,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    workbench: &mut ActiveWorkbench,
) {
    let mut stacks: Vec<ItemStack> = inventory
        .crafting
        .iter_mut()
        .filter_map(Option::take)
        .collect();
    stacks.extend(workbench.grid.drain());
    for stack in stacks {
        return_or_drop(commands, rng, player, hotbar, inventory, stack);
    }
    if let Some(stack) = inventory.carried.take() {
        return_or_drop(commands, rng, player, hotbar, inventory, stack);
    }
    // A closed interface must never leave a reusable session holding inputs;
    // the next workbench always starts with a fresh 3×3 grid.
    workbench.grid = CraftingGrid::workbench();
    workbench.position = None;
}

fn return_or_drop(
    commands: &mut Commands,
    rng: &mut ItemRng,
    player: &Transform,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    stack: ItemStack,
) {
    if let Some(remainder) = inventory.insert(hotbar, stack) {
        spawn_thrown_item(commands, rng, player, *player.forward(), remainder);
    }
}

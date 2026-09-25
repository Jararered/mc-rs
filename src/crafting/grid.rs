//! Crafting grid state and workbench sessions.

use crate::item::ItemStack;

pub const MAX_GRID_SLOTS: usize = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftingGrid {
    width: usize,
    height: usize,
    pub(super) slots: [Option<ItemStack>; MAX_GRID_SLOTS],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbenchSession {
    pub grid: CraftingGrid,
    pub position: (i32, i32, i32),
}

impl WorkbenchSession {
    pub fn new(position: (i32, i32, i32)) -> Self {
        Self {
            grid: CraftingGrid::workbench(),
            position,
        }
    }
    pub fn within_reach(self, player: (f32, f32, f32)) -> bool {
        let dx = player.0 - (self.position.0 as f32 + 0.5);
        let dy = player.1 - (self.position.1 as f32 + 0.5);
        let dz = player.2 - (self.position.2 as f32 + 0.5);
        dx * dx + dy * dy + dz * dz <= 64.0
    }
}

impl CraftingGrid {
    pub fn new(width: usize, height: usize) -> Self {
        assert!((1..=3).contains(&width) && (1..=3).contains(&height));
        Self {
            width,
            height,
            slots: [None; MAX_GRID_SLOTS],
        }
    }
    pub fn player() -> Self {
        Self::new(2, 2)
    }
    pub fn workbench() -> Self {
        Self::new(3, 3)
    }
    pub fn from_slots(width: usize, height: usize, slots: &[Option<ItemStack>]) -> Self {
        let mut grid = Self::new(width, height);
        for (target, source) in grid.slots[..width * height].iter_mut().zip(slots) {
            *target = *source;
        }
        grid
    }
    pub const fn width(&self) -> usize {
        self.width
    }
    pub const fn height(&self) -> usize {
        self.height
    }
    pub fn get(&self, x: usize, y: usize) -> Option<ItemStack> {
        (x < self.width && y < self.height)
            .then(|| self.slots[y * self.width + x])
            .flatten()
    }
    pub fn set(&mut self, x: usize, y: usize, stack: Option<ItemStack>) {
        assert!(x < self.width && y < self.height);
        self.slots[y * self.width + x] = stack;
    }
    pub fn slots(&self) -> impl Iterator<Item = Option<ItemStack>> + '_ {
        self.slots[..self.width * self.height].iter().copied()
    }
    pub fn slots_mut(&mut self) -> &mut [Option<ItemStack>] {
        &mut self.slots[..self.width * self.height]
    }
    pub fn occupied(&self) -> usize {
        self.slots().filter(Option::is_some).count()
    }

    /// Remove every input stack from the grid for container-close handling.
    pub fn drain(&mut self) -> Vec<ItemStack> {
        let mut drained = Vec::new();
        for slot in &mut self.slots[..self.width * self.height] {
            if let Some(stack) = slot.take() {
                drained.push(stack);
            }
        }
        drained
    }
}

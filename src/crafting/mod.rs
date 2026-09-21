//! Beta 1.7.3 crafting rules.
//!
//! This module deliberately has no Bevy dependencies.  A recipe consumes and
//! produces validated `ItemStack` values, while inventory containers decide
//! how those values are presented and transferred to a player.

use std::sync::OnceLock;

use crate::item::ItemId;
use crate::item::ItemStack;
use crate::world::block::registry::BetaBlockId;

pub const MAX_GRID_SLOTS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngredientData {
    Any,
    Exact(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ingredient {
    pub item: ItemId,
    pub data: IngredientData,
}

impl Ingredient {
    pub const fn any(item: ItemId) -> Self {
        Self {
            item,
            data: IngredientData::Any,
        }
    }
    pub const fn exact(item: ItemId, data: u16) -> Self {
        Self {
            item,
            data: IngredientData::Exact(data),
        }
    }
    pub fn matches(self, stack: ItemStack) -> bool {
        self.item == stack.item() && matches!(self.data, IngredientData::Any)
            || self.item == stack.item()
                && matches!(self.data, IngredientData::Exact(data) if data == stack.data())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CraftingGrid {
    width: usize,
    height: usize,
    slots: [Option<ItemStack>; MAX_GRID_SLOTS],
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recipe {
    Shaped {
        width: usize,
        height: usize,
        ingredients: Vec<Option<Ingredient>>,
        output: ItemStack,
    },
    Shapeless {
        ingredients: Vec<Ingredient>,
        output: ItemStack,
    },
}

impl Recipe {
    pub fn output(&self) -> ItemStack {
        match self {
            Self::Shaped { output, .. } | Self::Shapeless { output, .. } => *output,
        }
    }
    pub fn matches(&self, grid: &CraftingGrid) -> bool {
        match self {
            Self::Shaped {
                width,
                height,
                ingredients,
                ..
            } => {
                if *width > grid.width() || *height > grid.height() {
                    return false;
                }
                for y in 0..=grid.height() - height {
                    for x in 0..=grid.width() - width {
                        if self.matches_shaped_at(grid, *width, *height, ingredients, x, y, false)
                            || self.matches_shaped_at(
                                grid,
                                *width,
                                *height,
                                ingredients,
                                x,
                                y,
                                true,
                            )
                        {
                            return true;
                        }
                    }
                }
                false
            }
            Self::Shapeless { ingredients, .. } => {
                if ingredients.len() != grid.occupied() {
                    return false;
                }
                let mut used = vec![false; ingredients.len()];
                for stack in grid.slots().flatten() {
                    let Some(index) = ingredients
                        .iter()
                        .enumerate()
                        .position(|(i, ingredient)| !used[i] && ingredient.matches(stack))
                    else {
                        return false;
                    };
                    used[index] = true;
                }
                true
            }
        }
    }
    fn matches_shaped_at(
        &self,
        grid: &CraftingGrid,
        width: usize,
        height: usize,
        ingredients: &[Option<Ingredient>],
        x: usize,
        y: usize,
        mirrored: bool,
    ) -> bool {
        for gy in 0..grid.height() {
            for gx in 0..grid.width() {
                let rx = gx as isize - x as isize;
                let ry = gy as isize - y as isize;
                let expected =
                    if rx >= 0 && ry >= 0 && (rx as usize) < width && (ry as usize) < height {
                        let ix = if mirrored {
                            width - rx as usize - 1
                        } else {
                            rx as usize
                        };
                        ingredients[ry as usize * width + ix]
                    } else {
                        None
                    };
                if expected.is_some() != grid.get(gx, gy).is_some() {
                    return false;
                }
                if let (Some(ingredient), Some(stack)) = (expected, grid.get(gx, gy)) {
                    if !ingredient.matches(stack) {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Debug, Default)]
pub struct RecipeBook {
    recipes: Vec<Recipe>,
}

impl RecipeBook {
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }
    pub fn find(&self, grid: &CraftingGrid) -> Option<ItemStack> {
        self.recipes
            .iter()
            .find(|recipe| recipe.matches(grid))
            .map(Recipe::output)
    }

    /// Consume one unit from each occupied slot after a successful output
    /// pickup. The returned stacks are the Beta container-item remainders.
    pub fn consume_one(&self, grid: &mut CraftingGrid) -> Option<Vec<ItemStack>> {
        self.find(grid)?;
        let mut remainders = Vec::new();
        for slot in &mut grid.slots[..grid.width * grid.height] {
            let Some(stack) = *slot else { continue };
            if let Some(item) = stack.container_item() {
                remainders.push(ItemStack::new(item, 1).ok()?);
            }
            *slot = stack.with_count(stack.count() - 1).ok();
        }
        Some(remainders)
    }
}

fn b(id: BetaBlockId) -> ItemId {
    ItemId::from_block(id)
}
fn i(id: ItemId) -> Ingredient {
    Ingredient::any(id)
}
fn d(id: ItemId, data: u16) -> Ingredient {
    Ingredient::exact(id, data)
}
fn out(id: ItemId, count: u8, data: u16) -> ItemStack {
    ItemStack::with_data(id, count, data).expect("registered Beta recipe output")
}

impl RecipeBook {
    fn add_shaped(&mut self, rows: &[&str], symbols: &[(char, Ingredient)], output: ItemStack) {
        let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        let mut ingredients = Vec::with_capacity(width * rows.len());
        for row in rows {
            for ch in row.chars() {
                ingredients.push(
                    symbols
                        .iter()
                        .find(|(key, _)| *key == ch)
                        .map(|(_, value)| *value),
                );
            }
            for _ in row.len()..width {
                ingredients.push(None);
            }
        }
        self.recipes.push(Recipe::Shaped {
            width,
            height: rows.len(),
            ingredients,
            output,
        });
    }
    fn add_shapeless(&mut self, ingredients: &[Ingredient], output: ItemStack) {
        self.recipes.push(Recipe::Shapeless {
            ingredients: ingredients.to_vec(),
            output,
        });
    }
}

/// The complete Beta 1.7.3 recipe registry, built once and then shared.
pub fn beta_recipe_book() -> &'static RecipeBook {
    static BOOK: OnceLock<RecipeBook> = OnceLock::new();
    BOOK.get_or_init(|| {
        use BetaBlockId as B;
        use ItemId as I;
        let mut r = RecipeBook::default();
        let materials = [
            (
                b(B::WOODEN_PLANKS),
                I::WOODEN_PICKAXE,
                I::WOODEN_SHOVEL,
                I::WOODEN_AXE,
                I::WOODEN_HOE,
            ),
            (
                b(B::COBBLESTONE),
                I::STONE_PICKAXE,
                I::STONE_SHOVEL,
                I::STONE_AXE,
                I::STONE_HOE,
            ),
            (
                I::IRON_INGOT,
                I::IRON_PICKAXE,
                I::IRON_SHOVEL,
                I::IRON_AXE,
                I::IRON_HOE,
            ),
            (
                I::DIAMOND,
                I::DIAMOND_PICKAXE,
                I::DIAMOND_SHOVEL,
                I::DIAMOND_AXE,
                I::DIAMOND_HOE,
            ),
            (
                I::GOLD_INGOT,
                I::GOLD_PICKAXE,
                I::GOLD_SHOVEL,
                I::GOLD_AXE,
                I::GOLD_HOE,
            ),
        ];
        for (material, pick, shovel, axe, hoe) in materials {
            r.add_shaped(
                &["XXX", " # ", " # "],
                &[('X', i(material)), ('#', i(I::STICK))],
                out(pick, 1, 0),
            );
            r.add_shaped(
                &["X", "#", "#"],
                &[('X', i(material)), ('#', i(I::STICK))],
                out(shovel, 1, 0),
            );
            r.add_shaped(
                &["XX", "X#", " #"],
                &[('X', i(material)), ('#', i(I::STICK))],
                out(axe, 1, 0),
            );
            r.add_shaped(
                &["XX", " #", " #"],
                &[('X', i(material)), ('#', i(I::STICK))],
                out(hoe, 1, 0),
            );
        }
        for (material, sword) in [
            (b(B::WOODEN_PLANKS), I::WOODEN_SWORD),
            (b(B::COBBLESTONE), I::STONE_SWORD),
            (I::IRON_INGOT, I::IRON_SWORD),
            (I::DIAMOND, I::DIAMOND_SWORD),
            (I::GOLD_INGOT, I::GOLD_SWORD),
        ] {
            r.add_shaped(
                &["X", "X", "#"],
                &[('X', i(material)), ('#', i(I::STICK))],
                out(sword, 1, 0),
            );
        }
        r.add_shaped(
            &["##", "##"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(b(B::CRAFTING_TABLE), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(b(B::CHEST), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::COBBLESTONE)))],
            out(b(B::FURNACE), 1, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::SAND)))],
            out(b(B::SANDSTONE), 1, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(I::STICK))],
            out(b(B::FENCE), 2, 0),
        );
        r.add_shaped(
            &["###", "XXX", "###"],
            &[('#', i(b(B::WOODEN_PLANKS))), ('X', i(I::BOOK))],
            out(b(B::BOOKSHELF), 1, 0),
        );
        r.add_shaped(
            &["#", "#"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(I::STICK, 4, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::COAL)), ('#', i(I::STICK))],
            out(b(B::TORCH), 4, 0),
        );
        r.add_shaped(
            &["# #", " # "],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(I::BOWL, 4, 0),
        );
        r.add_shaped(
            &["# #", "###"],
            &[('#', i(I::IRON_INGOT))],
            out(I::MINECART, 1, 0),
        );
        r.add_shaped(
            &["# #", " # "],
            &[('#', i(I::IRON_INGOT))],
            out(I::BUCKET, 1, 0),
        );
        r.add_shaped(&["###"], &[('#', i(I::WHEAT))], out(I::BREAD, 1, 0));
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::STICK)), ('X', i(b(B::WOOL)))],
            out(I::PAINTING, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::GOLD_BLOCK))), ('X', i(I::APPLE))],
            out(I::GOLDEN_APPLE, 1, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::STICK)), ('#', i(b(B::COBBLESTONE)))],
            out(b(B::LEVER), 1, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::REDSTONE)), ('#', i(I::STICK))],
            out(b(B::REDSTONE_TORCH), 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "#R#"],
            &[
                ('#', i(b(B::COBBLESTONE))),
                ('X', i(I::BOW)),
                ('R', i(I::REDSTONE)),
            ],
            out(b(B::DISPENSER), 1, 0),
        );
        r.add_shaped(
            &["TTT", "#X#", "#R#"],
            &[
                ('T', i(b(B::WOODEN_PLANKS))),
                ('#', i(b(B::COBBLESTONE))),
                ('X', i(I::IRON_INGOT)),
                ('R', i(I::REDSTONE)),
            ],
            out(b(B::PISTON), 1, 0),
        );
        r.add_shaped(
            &["###", "XXX"],
            &[('#', i(b(B::WOOL))), ('X', i(b(B::WOODEN_PLANKS)))],
            out(I::BED, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::WOODEN_PLANKS))), ('X', i(I::DIAMOND))],
            out(b(B::JUKEBOX), 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::WOODEN_PLANKS))), ('X', i(I::REDSTONE))],
            out(b(B::NOTE_BLOCK), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::SNOWBALL))],
            out(b(B::SNOW), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::CLAY_BALL))],
            out(b(B::CLAY), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::BRICK))],
            out(b(B::BRICKS), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::GLOWSTONE_DUST))],
            out(b(B::GLOWSTONE), 1, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "X X"],
            &[('X', i(I::IRON_INGOT)), ('#', i(I::STICK))],
            out(b(B::RAIL), 16, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "XRX"],
            &[
                ('X', i(I::GOLD_INGOT)),
                ('#', i(I::STICK)),
                ('R', i(I::REDSTONE)),
            ],
            out(b(B::POWERED_RAIL), 6, 0),
        );
        r.add_shaped(
            &["# #", "###"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(I::BOAT, 1, 0),
        );
        r.add_shaped(
            &[" #X", "# X", " #X"],
            &[('#', i(I::STICK)), ('X', i(I::STRING))],
            out(I::BOW, 1, 0),
        );
        r.add_shaped(
            &["X", "#", "Y"],
            &[('X', i(I::FLINT)), ('#', i(I::STICK)), ('Y', i(I::FEATHER))],
            out(I::ARROW, 4, 0),
        );
        r.add_shaped(
            &["  #", " #X", "# X"],
            &[('#', i(I::STICK)), ('X', i(I::STRING))],
            out(I::FISHING_ROD, 1, 0),
        );
        r.add_shaped(
            &[" #", "# "],
            &[('#', i(I::IRON_INGOT))],
            out(I::SHEARS, 1, 0),
        );
        r.add_shaped(
            &["# #", "###", "# #"],
            &[('#', i(I::STICK))],
            out(b(B::LADDER), 2, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::COBBLESTONE)))],
            out(b(B::STONE_SLAB), 3, 3),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::STONE)))],
            out(b(B::STONE_SLAB), 3, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::SANDSTONE)))],
            out(b(B::STONE_SLAB), 3, 1),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(b(B::STONE_SLAB), 3, 2),
        );
        r.add_shaped(
            &["#  ", "## ", "###"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(b(B::WOODEN_STAIRS), 4, 0),
        );
        r.add_shaped(
            &["#  ", "## ", "###"],
            &[('#', i(b(B::COBBLESTONE)))],
            out(b(B::COBBLESTONE_STAIRS), 4, 0),
        );
        r.add_shaped(
            &["##", "##", "##"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(I::WOODEN_DOOR, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::GUNPOWDER)), ('X', i(b(B::SAND)))],
            out(b(B::TNT), 1, 0),
        );
        r.add_shaped(
            &[" # ", "#X#", " # "],
            &[('#', i(I::GOLD_INGOT)), ('X', i(I::REDSTONE))],
            out(I::CLOCK, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::PAPER)), ('X', i(I::COMPASS))],
            out(I::MAP, 1, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "XRX"],
            &[
                ('X', i(I::IRON_INGOT)),
                ('#', i(b(B::STONE_PRESSURE_PLATE))),
                ('R', i(I::REDSTONE)),
            ],
            out(b(B::DETECTOR_RAIL), 6, 0),
        );
        r.add_shaped(
            &["#X#", "III"],
            &[
                ('#', i(b(B::REDSTONE_TORCH))),
                ('X', i(I::REDSTONE)),
                ('I', i(b(B::STONE))),
            ],
            out(I::REPEATER, 1, 0),
        );
        r.add_shaped(
            &["#", "#"],
            &[('#', i(b(B::STONE)))],
            out(b(B::STONE_BUTTON), 1, 0),
        );
        for (material, helmet, chest, legs, boots) in [
            (
                i(I::LEATHER),
                I::LEATHER_HELMET,
                I::LEATHER_CHESTPLATE,
                I::LEATHER_LEGGINGS,
                I::LEATHER_BOOTS,
            ),
            (
                i(b(B::FIRE)),
                I::CHAINMAIL_HELMET,
                I::CHAINMAIL_CHESTPLATE,
                I::CHAINMAIL_LEGGINGS,
                I::CHAINMAIL_BOOTS,
            ),
            (
                i(I::IRON_INGOT),
                I::IRON_HELMET,
                I::IRON_CHESTPLATE,
                I::IRON_LEGGINGS,
                I::IRON_BOOTS,
            ),
            (
                i(I::DIAMOND),
                I::DIAMOND_HELMET,
                I::DIAMOND_CHESTPLATE,
                I::DIAMOND_LEGGINGS,
                I::DIAMOND_BOOTS,
            ),
            (
                i(I::GOLD_INGOT),
                I::GOLD_HELMET,
                I::GOLD_CHESTPLATE,
                I::GOLD_LEGGINGS,
                I::GOLD_BOOTS,
            ),
        ] {
            r.add_shaped(&["XXX", "X X"], &[('X', material)], out(helmet, 1, 0));
            r.add_shaped(&["X X", "XXX", "XXX"], &[('X', material)], out(chest, 1, 0));
            r.add_shaped(&["XXX", "X X", "X X"], &[('X', material)], out(legs, 1, 0));
            r.add_shaped(&["X X", "X X"], &[('X', material)], out(boots, 1, 0));
        }
        r.add_shaped(
            &["A", "B"],
            &[('A', i(b(B::PUMPKIN))), ('B', i(b(B::TORCH)))],
            out(b(B::JACK_OLANTERN), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::STONE)))],
            out(b(B::STONE_PRESSURE_PLATE), 1, 0),
        );
        r.add_shaped(
            &["##"],
            &[('#', i(b(B::STONE)))],
            out(b(B::STONE_BUTTON), 1, 0),
        );
        r.add_shaped(
            &["###", "###"],
            &[('#', i(b(B::WOODEN_PLANKS)))],
            out(b(B::TRAPDOOR), 2, 0),
        );
        r.add_shaped(
            &["##", "##", "##"],
            &[('#', i(I::IRON_INGOT))],
            out(I::IRON_DOOR, 1, 0),
        );
        r.add_shaped(
            &["###", "###", " X "],
            &[('#', i(b(B::WOODEN_PLANKS))), ('X', i(I::STICK))],
            out(I::SIGN, 1, 0),
        );
        r.add_shaped(
            &["AAA", "BEB", "CCC"],
            &[
                ('A', i(I::MILK_BUCKET)),
                ('B', i(I::SUGAR)),
                ('C', i(I::WHEAT)),
                ('E', i(I::EGG)),
            ],
            out(I::CAKE, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::PAPER)), ('X', i(I::COMPASS))],
            out(I::MAP, 1, 0),
        );
        r.add_shaped(
            &[" # ", "#X#", " # "],
            &[('#', i(I::IRON_INGOT)), ('X', i(I::REDSTONE))],
            out(I::COMPASS, 1, 0),
        );
        r.add_shapeless(&[i(b(B::WOOD))], out(b(B::WOODEN_PLANKS), 4, 0));
        r.add_shapeless(&[i(I::SUGAR_CANE)], out(I::PAPER, 3, 0));
        r.add_shapeless(&[i(I::SUGAR_CANE)], out(I::SUGAR, 1, 0));
        r.add_shapeless(&[i(I::BONE)], out(I::DYE, 3, 15));
        r.add_shapeless(&[d(I::DYE, 1), d(I::DYE, 15)], out(I::DYE, 2, 9));
        for color in 0..16u16 {
            r.add_shapeless(
                &[d(I::DYE, color), d(b(B::WOOL), 0)],
                out(b(B::WOOL), 1, color),
            );
        }
        r.add_shapeless(&[i(b(B::DANDELION))], out(I::DYE, 2, 11));
        r.add_shapeless(&[i(b(B::ROSE))], out(I::DYE, 2, 1));
        r.add_shapeless(&[i(I::BONE)], out(I::DYE, 3, 15));
        r.add_shaped(
            &["Y", "X", "#"],
            &[
                ('Y', i(b(B::RED_MUSHROOM))),
                ('X', i(b(B::BROWN_MUSHROOM))),
                ('#', i(I::BOWL)),
            ],
            out(I::MUSHROOM_STEW, 1, 0),
        );
        r.add_shaped(
            &["Y", "X", "#"],
            &[
                ('Y', i(b(B::BROWN_MUSHROOM))),
                ('X', i(b(B::RED_MUSHROOM))),
                ('#', i(I::BOWL)),
            ],
            out(I::MUSHROOM_STEW, 1, 0),
        );
        r.add_shaped(
            &["#X#"],
            &[('#', i(I::WHEAT)), ('X', d(I::DYE, 3))],
            out(I::COOKIE, 8, 0),
        );
        for (block, item) in [
            (b(B::GOLD_BLOCK), I::GOLD_INGOT),
            (b(B::IRON_BLOCK), I::IRON_INGOT),
            (b(B::DIAMOND_BLOCK), I::DIAMOND),
        ] {
            r.add_shaped(&["###", "###", "###"], &[('#', i(item))], out(block, 1, 0));
            r.add_shapeless(&[i(block)], out(item, 9, 0));
        }
        r
    })
}

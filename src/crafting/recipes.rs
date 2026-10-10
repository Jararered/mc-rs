//! Recipe matching and the Beta 1.7.3 recipe registry.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::item::Item;
use crate::item::ItemStack;

use super::grid::CraftingGrid;
use super::ingredient::Ingredient;

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
    /// pickup. As in `SlotCrafting.onPickupFromSlot`, a container item such as
    /// milk's empty bucket takes the place of the input it came from. The
    /// returned stacks are container items whose slot still held inputs.
    pub fn consume_one(&self, grid: &mut CraftingGrid) -> Option<Vec<ItemStack>> {
        self.find(grid)?;
        let mut remainders = Vec::new();
        let slot_count = grid.width() * grid.height();
        for slot in &mut grid.slots[..slot_count] {
            let Some(stack) = *slot else { continue };
            *slot = stack.with_count(stack.count() - 1).ok();
            if let Some(item) = stack.container_item() {
                let container = ItemStack::new(item, 1).ok()?;
                if slot.is_none() {
                    *slot = Some(container);
                } else {
                    remainders.push(container);
                }
            }
        }
        Some(remainders)
    }
}

/// One entry of `data/recipes.ron`.
#[derive(Deserialize)]
enum RecipeData {
    Shaped {
        rows: Vec<String>,
        key: BTreeMap<char, String>,
        out: (String, u8),
    },
    Shapeless {
        of: Vec<String>,
        out: (String, u8),
    },
}

/// `"Name"` or `"Name:data"`.
fn named(text: &str) -> (Item, Option<u16>) {
    let (name, data) = match text.split_once(':') {
        Some((name, data)) => (name, data.parse().ok()),
        None => (text, None),
    };
    let item = Item::named(name).unwrap_or_else(|| panic!("recipes.ron names no item {name}"));
    (item, data)
}

fn ingredient(text: &str) -> Ingredient {
    match named(text) {
        (item, Some(data)) => Ingredient::exact(item, data),
        (item, None) => Ingredient::any(item),
    }
}

fn output((text, count): &(String, u8)) -> ItemStack {
    let (item, data) = named(text);
    ItemStack::with_data(item, *count, data.unwrap_or(0)).expect("registered Beta recipe output")
}

impl From<RecipeData> for Recipe {
    fn from(data: RecipeData) -> Self {
        match data {
            RecipeData::Shaped { rows, key, out } => {
                let width = rows.iter().map(String::len).max().unwrap_or(0);
                let mut ingredients = Vec::with_capacity(width * rows.len());
                for row in &rows {
                    ingredients.extend(
                        row.chars()
                            .map(|ch| key.get(&ch).map(|name| ingredient(name))),
                    );
                    ingredients.resize(ingredients.len() + width - row.len(), None);
                }
                Self::Shaped {
                    width,
                    height: rows.len(),
                    ingredients,
                    output: output(&out),
                }
            }
            RecipeData::Shapeless { of, out } => Self::Shapeless {
                ingredients: of.iter().map(|name| ingredient(name)).collect(),
                output: output(&out),
            },
        }
    }
}

/// The complete Beta 1.7.3 recipe registry, read once from
/// `data/recipes.ron` and then shared.
pub fn beta_recipe_book() -> &'static RecipeBook {
    static BOOK: OnceLock<RecipeBook> = OnceLock::new();
    BOOK.get_or_init(|| {
        let recipes: Vec<RecipeData> = ron::from_str(include_str!("../../data/recipes.ron"))
            .unwrap_or_else(|error| panic!("data/recipes.ron: {error}"));
        RecipeBook {
            recipes: recipes.into_iter().map(Recipe::from).collect(),
        }
    })
}

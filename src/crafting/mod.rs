//! Beta 1.7.3 crafting rules.
//!
//! This module deliberately has no Bevy dependencies. A recipe consumes and
//! produces validated `ItemStack` values, while inventory containers decide
//! how those values are presented and transferred to a player.

mod grid;
mod ingredient;
mod recipes;

pub use grid::CraftingGrid;
pub use grid::MAX_GRID_SLOTS;
pub use grid::WorkbenchSession;
pub use ingredient::Ingredient;
pub use ingredient::IngredientData;
pub use recipes::Recipe;
pub use recipes::RecipeBook;
pub use recipes::beta_recipe_book;

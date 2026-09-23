//! Beta 1.7.3 crafting rules.
//!
//! This module deliberately has no Bevy dependencies. A recipe consumes and
//! produces validated `ItemStack` values, while inventory containers decide
//! how those values are presented and transferred to a player.

mod crafting_grid;
mod ingredient;
mod recipe_book;

pub use crafting_grid::CraftingGrid;
pub use crafting_grid::MAX_GRID_SLOTS;
pub use crafting_grid::WorkbenchSession;
pub use ingredient::Ingredient;
pub use ingredient::IngredientData;
pub use recipe_book::Recipe;
pub use recipe_book::RecipeBook;
pub use recipe_book::beta_recipe_book;

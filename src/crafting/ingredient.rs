//! Ingredient matching rules for crafting recipes.

use crate::item::ItemId;
use crate::item::ItemStack;

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

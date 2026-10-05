//! Recipe matching and the Beta 1.7.3 recipe registry.

use std::sync::OnceLock;

use crate::block::blocks::Block;
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

fn b(block: Block) -> Item {
    Item::from_block(block).expect("air is not an item")
}
fn i(item: Item) -> Ingredient {
    Ingredient::any(item)
}
fn d(item: Item, data: u16) -> Ingredient {
    Ingredient::exact(item, data)
}
fn out(item: Item, count: u8, data: u16) -> ItemStack {
    ItemStack::with_data(item, count, data).expect("registered Beta recipe output")
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
        use Block as B;
        use Item as I;
        let mut r = RecipeBook::default();
        let materials = [
            (
                b(B::WoodenPlanks),
                I::WoodenPickaxe,
                I::WoodenShovel,
                I::WoodenAxe,
                I::WoodenHoe,
            ),
            (
                b(B::Cobblestone),
                I::StonePickaxe,
                I::StoneShovel,
                I::StoneAxe,
                I::StoneHoe,
            ),
            (
                I::IronIngot,
                I::IronPickaxe,
                I::IronShovel,
                I::IronAxe,
                I::IronHoe,
            ),
            (
                I::Diamond,
                I::DiamondPickaxe,
                I::DiamondShovel,
                I::DiamondAxe,
                I::DiamondHoe,
            ),
            (
                I::GoldIngot,
                I::GoldPickaxe,
                I::GoldShovel,
                I::GoldAxe,
                I::GoldHoe,
            ),
        ];
        for (material, pick, shovel, axe, hoe) in materials {
            r.add_shaped(
                &["XXX", " # ", " # "],
                &[('X', i(material)), ('#', i(I::Stick))],
                out(pick, 1, 0),
            );
            r.add_shaped(
                &["X", "#", "#"],
                &[('X', i(material)), ('#', i(I::Stick))],
                out(shovel, 1, 0),
            );
            r.add_shaped(
                &["XX", "X#", " #"],
                &[('X', i(material)), ('#', i(I::Stick))],
                out(axe, 1, 0),
            );
            r.add_shaped(
                &["XX", " #", " #"],
                &[('X', i(material)), ('#', i(I::Stick))],
                out(hoe, 1, 0),
            );
        }
        for (material, sword) in [
            (b(B::WoodenPlanks), I::WoodenSword),
            (b(B::Cobblestone), I::StoneSword),
            (I::IronIngot, I::IronSword),
            (I::Diamond, I::DiamondSword),
            (I::GoldIngot, I::GoldSword),
        ] {
            r.add_shaped(
                &["X", "X", "#"],
                &[('X', i(material)), ('#', i(I::Stick))],
                out(sword, 1, 0),
            );
        }
        r.add_shaped(
            &["##", "##"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(b(B::CraftingTable), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(b(B::Chest), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::Cobblestone)))],
            out(b(B::Furnace), 1, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::Sand)))],
            out(b(B::Sandstone), 1, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(I::Stick))],
            out(b(B::Fence), 2, 0),
        );
        r.add_shaped(
            &["###", "XXX", "###"],
            &[('#', i(b(B::WoodenPlanks))), ('X', i(I::Book))],
            out(b(B::Bookshelf), 1, 0),
        );
        r.add_shaped(
            &["#", "#"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(I::Stick, 4, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::Coal)), ('#', i(I::Stick))],
            out(b(B::Torch), 4, 0),
        );
        r.add_shaped(
            &["# #", " # "],
            &[('#', i(b(B::WoodenPlanks)))],
            out(I::Bowl, 4, 0),
        );
        r.add_shaped(
            &["# #", "###"],
            &[('#', i(I::IronIngot))],
            out(I::Minecart, 1, 0),
        );
        r.add_shaped(
            &["# #", " # "],
            &[('#', i(I::IronIngot))],
            out(I::Bucket, 1, 0),
        );
        r.add_shaped(&["###"], &[('#', i(I::Wheat))], out(I::Bread, 1, 0));
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::Stick)), ('X', i(b(B::Wool)))],
            out(I::Painting, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::GoldBlock))), ('X', i(I::Apple))],
            out(I::GoldenApple, 1, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::Stick)), ('#', i(b(B::Cobblestone)))],
            out(b(B::Lever), 1, 0),
        );
        r.add_shaped(
            &["X", "#"],
            &[('X', i(I::Redstone)), ('#', i(I::Stick))],
            out(b(B::RedstoneTorch), 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "#R#"],
            &[
                ('#', i(b(B::Cobblestone))),
                ('X', i(I::Bow)),
                ('R', i(I::Redstone)),
            ],
            out(b(B::Dispenser), 1, 0),
        );
        r.add_shaped(
            &["TTT", "#X#", "#R#"],
            &[
                ('T', i(b(B::WoodenPlanks))),
                ('#', i(b(B::Cobblestone))),
                ('X', i(I::IronIngot)),
                ('R', i(I::Redstone)),
            ],
            out(b(B::Piston), 1, 0),
        );
        r.add_shaped(
            &["###", "XXX"],
            &[('#', i(b(B::Wool))), ('X', i(b(B::WoodenPlanks)))],
            out(I::Bed, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::WoodenPlanks))), ('X', i(I::Diamond))],
            out(b(B::Jukebox), 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(b(B::WoodenPlanks))), ('X', i(I::Redstone))],
            out(b(B::NoteBlock), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::Snowball))],
            out(b(B::Snow), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::ClayBall))],
            out(b(B::Clay), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::Brick))],
            out(b(B::Bricks), 1, 0),
        );
        r.add_shaped(
            &["##", "##"],
            &[('#', i(I::GlowstoneDust))],
            out(b(B::Glowstone), 1, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "X X"],
            &[('X', i(I::IronIngot)), ('#', i(I::Stick))],
            out(b(B::Rail), 16, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "XRX"],
            &[
                ('X', i(I::GoldIngot)),
                ('#', i(I::Stick)),
                ('R', i(I::Redstone)),
            ],
            out(b(B::PoweredRail), 6, 0),
        );
        r.add_shaped(
            &["# #", "###"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(I::Boat, 1, 0),
        );
        r.add_shaped(
            &[" #X", "# X", " #X"],
            &[('#', i(I::Stick)), ('X', i(I::String))],
            out(I::Bow, 1, 0),
        );
        r.add_shaped(
            &["X", "#", "Y"],
            &[('X', i(I::Flint)), ('#', i(I::Stick)), ('Y', i(I::Feather))],
            out(I::Arrow, 4, 0),
        );
        r.add_shaped(
            &["  #", " #X", "# X"],
            &[('#', i(I::Stick)), ('X', i(I::String))],
            out(I::FishingRod, 1, 0),
        );
        r.add_shaped(
            &[" #", "# "],
            &[('#', i(I::IronIngot))],
            out(I::Shears, 1, 0),
        );
        r.add_shaped(
            &["# #", "###", "# #"],
            &[('#', i(I::Stick))],
            out(b(B::Ladder), 2, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::Cobblestone)))],
            out(b(B::StoneSlab), 3, 3),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::Stone)))],
            out(b(B::StoneSlab), 3, 0),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::Sandstone)))],
            out(b(B::StoneSlab), 3, 1),
        );
        r.add_shaped(
            &["###", "###", "###"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(b(B::StoneSlab), 3, 2),
        );
        r.add_shaped(
            &["#  ", "## ", "###"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(b(B::WoodenStairs), 4, 0),
        );
        r.add_shaped(
            &["#  ", "## ", "###"],
            &[('#', i(b(B::Cobblestone)))],
            out(b(B::CobblestoneStairs), 4, 0),
        );
        r.add_shaped(
            &["##", "##", "##"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(I::WoodenDoor, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::Gunpowder)), ('X', i(b(B::Sand)))],
            out(b(B::Tnt), 1, 0),
        );
        r.add_shaped(
            &[" # ", "#X#", " # "],
            &[('#', i(I::GoldIngot)), ('X', i(I::Redstone))],
            out(I::Clock, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::Paper)), ('X', i(I::Compass))],
            out(I::Map, 1, 0),
        );
        r.add_shaped(
            &["X X", "X#X", "XRX"],
            &[
                ('X', i(I::IronIngot)),
                ('#', i(b(B::StonePressurePlate))),
                ('R', i(I::Redstone)),
            ],
            out(b(B::DetectorRail), 6, 0),
        );
        r.add_shaped(
            &["#X#", "III"],
            &[
                ('#', i(b(B::RedstoneTorch))),
                ('X', i(I::Redstone)),
                ('I', i(b(B::Stone))),
            ],
            out(I::Repeater, 1, 0),
        );
        r.add_shaped(
            &["#", "#"],
            &[('#', i(b(B::Stone)))],
            out(b(B::StoneButton), 1, 0),
        );
        for (material, helmet, chest, legs, boots) in [
            (
                i(I::Leather),
                I::LeatherHelmet,
                I::LeatherChestplate,
                I::LeatherLeggings,
                I::LeatherBoots,
            ),
            (
                i(b(B::Fire)),
                I::ChainmailHelmet,
                I::ChainmailChestplate,
                I::ChainmailLeggings,
                I::ChainmailBoots,
            ),
            (
                i(I::IronIngot),
                I::IronHelmet,
                I::IronChestplate,
                I::IronLeggings,
                I::IronBoots,
            ),
            (
                i(I::Diamond),
                I::DiamondHelmet,
                I::DiamondChestplate,
                I::DiamondLeggings,
                I::DiamondBoots,
            ),
            (
                i(I::GoldIngot),
                I::GoldHelmet,
                I::GoldChestplate,
                I::GoldLeggings,
                I::GoldBoots,
            ),
        ] {
            r.add_shaped(&["XXX", "X X"], &[('X', material)], out(helmet, 1, 0));
            r.add_shaped(&["X X", "XXX", "XXX"], &[('X', material)], out(chest, 1, 0));
            r.add_shaped(&["XXX", "X X", "X X"], &[('X', material)], out(legs, 1, 0));
            r.add_shaped(&["X X", "X X"], &[('X', material)], out(boots, 1, 0));
        }
        r.add_shaped(
            &["A", "B"],
            &[('A', i(b(B::Pumpkin))), ('B', i(b(B::Torch)))],
            out(b(B::JackOLantern), 1, 0),
        );
        r.add_shaped(
            &["###", "# #", "###"],
            &[('#', i(b(B::Stone)))],
            out(b(B::StonePressurePlate), 1, 0),
        );
        r.add_shaped(
            &["##"],
            &[('#', i(b(B::Stone)))],
            out(b(B::StoneButton), 1, 0),
        );
        r.add_shaped(
            &["###", "###"],
            &[('#', i(b(B::WoodenPlanks)))],
            out(b(B::Trapdoor), 2, 0),
        );
        r.add_shaped(
            &["##", "##", "##"],
            &[('#', i(I::IronIngot))],
            out(I::IronDoor, 1, 0),
        );
        r.add_shaped(
            &["###", "###", " X "],
            &[('#', i(b(B::WoodenPlanks))), ('X', i(I::Stick))],
            out(I::Sign, 1, 0),
        );
        r.add_shaped(
            &["AAA", "BEB", "CCC"],
            &[
                ('A', i(I::MilkBucket)),
                ('B', i(I::Sugar)),
                ('C', i(I::Wheat)),
                ('E', i(I::Egg)),
            ],
            out(I::Cake, 1, 0),
        );
        r.add_shaped(
            &["###", "#X#", "###"],
            &[('#', i(I::Paper)), ('X', i(I::Compass))],
            out(I::Map, 1, 0),
        );
        r.add_shaped(
            &[" # ", "#X#", " # "],
            &[('#', i(I::IronIngot)), ('X', i(I::Redstone))],
            out(I::Compass, 1, 0),
        );
        for species in 0..=2 {
            r.add_shapeless(
                &[d(b(B::Wood), species)],
                out(b(B::WoodenPlanks), 4, species),
            );
        }
        r.add_shapeless(
            &[i(I::SugarCane), i(I::SugarCane), i(I::SugarCane)],
            out(I::Paper, 3, 0),
        );
        r.add_shapeless(&[i(I::SugarCane)], out(I::Sugar, 1, 0));
        r.add_shapeless(&[i(I::Bone)], out(I::Dye, 3, 15));
        r.add_shapeless(&[d(I::Dye, 1), d(I::Dye, 15)], out(I::Dye, 2, 9));
        for color in 0..16u16 {
            r.add_shapeless(
                &[d(I::Dye, color), d(b(B::Wool), 0)],
                out(b(B::Wool), 1, color),
            );
        }
        r.add_shapeless(&[i(b(B::Dandelion))], out(I::Dye, 2, 11));
        r.add_shapeless(&[i(b(B::Rose))], out(I::Dye, 2, 1));
        r.add_shapeless(&[i(I::Bone)], out(I::Dye, 3, 15));
        r.add_shaped(
            &["Y", "X", "#"],
            &[
                ('Y', i(b(B::RedMushroom))),
                ('X', i(b(B::BrownMushroom))),
                ('#', i(I::Bowl)),
            ],
            out(I::MushroomStew, 1, 0),
        );
        r.add_shaped(
            &["Y", "X", "#"],
            &[
                ('Y', i(b(B::BrownMushroom))),
                ('X', i(b(B::RedMushroom))),
                ('#', i(I::Bowl)),
            ],
            out(I::MushroomStew, 1, 0),
        );
        r.add_shaped(
            &["#X#"],
            &[('#', i(I::Wheat)), ('X', d(I::Dye, 3))],
            out(I::Cookie, 8, 0),
        );
        for (block, item) in [
            (b(B::GoldBlock), I::GoldIngot),
            (b(B::IronBlock), I::IronIngot),
            (b(B::DiamondBlock), I::Diamond),
        ] {
            r.add_shaped(&["###", "###", "###"], &[('#', i(item))], out(block, 1, 0));
            r.add_shapeless(&[i(block)], out(item, 9, 0));
        }
        r
    })
}

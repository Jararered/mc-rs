//! Original dungeon chest loot from Beta 1.7.3 `WorldGenDungeons`.

use crate::item::Item;
use crate::item::ItemStack;
use crate::random::JavaRandom;
use crate::world::chest::CHEST_SLOTS;
use crate::world::chest::Chest;

const DUNGEON_LOOT_ROLLS: usize = 8;

/// Generate the inventory placed in a newly created Beta dungeon chest.
///
/// The selector, conditional child draws, eight attempts, and direct slot
/// overwrites intentionally mirror `WorldGenDungeons.pickCheckLootItem` and its
/// chest-placement loop so the surrounding Java RNG sequence remains unchanged.
///
/// # Panics
///
/// Panics if the chest slot count cannot be represented by Java's bounded
/// integer API. The current 27-slot chest cannot trigger this.
pub fn generate_dungeon_chest(random: &mut JavaRandom) -> Chest {
    let mut chest = Chest::default();
    let slot_count = u32::try_from(CHEST_SLOTS).expect("a chest has at most u32::MAX slots");
    for _ in 0..DUNGEON_LOOT_ROLLS {
        let Some(stack) = pick_loot_item(random) else {
            continue;
        };
        let slot = random.next_int(slot_count) as usize;
        chest.slots[slot] = Some(stack);
    }
    chest
}

fn pick_loot_item(random: &mut JavaRandom) -> Option<ItemStack> {
    match random.next_int(11) {
        0 => Some(single(Item::Saddle)),
        1 => Some(
            ItemStack::new(Item::IronIngot, random_count(random))
                .expect("Beta dungeon loot quantities are valid iron-ingot stacks"),
        ),
        2 => Some(single(Item::Bread)),
        3 => Some(
            ItemStack::new(Item::Wheat, random_count(random))
                .expect("Beta dungeon loot quantities are valid wheat stacks"),
        ),
        4 => Some(
            ItemStack::new(Item::Gunpowder, random_count(random))
                .expect("Beta dungeon loot quantities are valid gunpowder stacks"),
        ),
        5 => Some(
            ItemStack::new(Item::String, random_count(random))
                .expect("Beta dungeon loot quantities are valid string stacks"),
        ),
        6 => Some(single(Item::Bucket)),
        7 if random.next_int(100) == 0 => Some(single(Item::GoldenApple)),
        8 if random.next_int(2) == 0 => Some(
            ItemStack::new(Item::Redstone, random_count(random))
                .expect("Beta dungeon loot quantities are valid redstone stacks"),
        ),
        9 if random.next_int(10) == 0 => Some(single(if random.next_int(2) == 0 {
            Item::Record13
        } else {
            Item::RecordCat
        })),
        10 => Some(
            ItemStack::with_data(Item::Dye, 1, 3)
                .expect("brown dye is a valid Beta dungeon loot stack"),
        ),
        _ => None,
    }
}

fn random_count(random: &mut JavaRandom) -> u8 {
    u8::try_from(random.next_int(4)).expect("a four-sided roll fits in u8") + 1
}

fn single(item: Item) -> ItemStack {
    ItemStack::new(item, 1).expect("Beta dungeon loot item must be registered")
}

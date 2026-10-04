use std::collections::HashSet;

use game::item::Item;
use game::item::ItemStack;
use game::random::JavaRandom;
use game::world::generation::overworld::generate_dungeon_chest;

fn one(item: Item) -> ItemStack {
    ItemStack::new(item, 1).expect("test dungeon loot item should be registered")
}

#[test]
fn dungeon_loot_matches_the_beta_java_random_sequence() {
    let mut random = JavaRandom::new(1);
    let chest = generate_dungeon_chest(&mut random);
    let expected = std::array::from_fn(|slot| match slot {
        5 => Some(one(Item::Redstone)),
        10 => Some(ItemStack::new(Item::Wheat, 4).unwrap()),
        12 => Some(one(Item::Saddle)),
        16 => Some(one(Item::Bread)),
        19 => Some(one(Item::Gunpowder)),
        _ => None,
    });

    // Slot 16 originally receives a bucket and is then overwritten by bread.
    assert_eq!(chest.slots, expected);
    assert_eq!(random.state(), 137_102_909_728_704);

    for (seed, expected_state, rare_item) in [
        (79, 124_265_925_996_201, Item::RecordCat),
        (66, 239_990_216_468_994, Item::Record13),
        (393, 261_165_909_313_586, Item::GoldenApple),
    ] {
        let mut random = JavaRandom::new(seed);
        let chest = generate_dungeon_chest(&mut random);
        assert!(chest.slots.contains(&Some(one(rare_item))));
        assert_eq!(random.state(), expected_state);
    }

    let mut random = JavaRandom::new(89_497);
    let chest = generate_dungeon_chest(&mut random);
    assert!(chest.slots.iter().all(Option::is_none));
    assert_eq!(random.state(), 166_419_017_100_932);
}

#[test]
fn dungeon_loot_contains_every_beta_entry_and_only_valid_counts() {
    let mut seen = HashSet::new();
    for seed in 0..20_000 {
        let mut random = JavaRandom::new(seed);
        for stack in generate_dungeon_chest(&mut random)
            .slots
            .into_iter()
            .flatten()
        {
            match stack.item() {
                Item::IronIngot
                | Item::Wheat
                | Item::Gunpowder
                | Item::String
                | Item::Redstone => {
                    assert!((1..=4).contains(&stack.count()));
                    assert_eq!(stack.data(), 0);
                }
                Item::Saddle
                | Item::Bread
                | Item::Bucket
                | Item::GoldenApple
                | Item::Record13
                | Item::RecordCat => {
                    assert_eq!(stack.count(), 1);
                    assert_eq!(stack.data(), 0);
                }
                Item::Dye => {
                    assert_eq!(stack.count(), 1);
                    assert_eq!(stack.data(), 3);
                }
                item => panic!("non-Beta dungeon loot item: {item:?}"),
            }
            seen.insert((stack.item(), stack.data()));
        }
    }

    let expected = HashSet::from([
        (Item::Saddle, 0),
        (Item::IronIngot, 0),
        (Item::Bread, 0),
        (Item::Wheat, 0),
        (Item::Gunpowder, 0),
        (Item::String, 0),
        (Item::Bucket, 0),
        (Item::GoldenApple, 0),
        (Item::Redstone, 0),
        (Item::Record13, 0),
        (Item::RecordCat, 0),
        (Item::Dye, 3),
    ]);
    assert_eq!(seen, expected);
}

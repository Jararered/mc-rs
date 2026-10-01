use std::collections::HashSet;

use game::item::ItemId;
use game::item::ItemStack;
use game::random::JavaRandom;
use game::world::generation::overworld::generate_dungeon_chest;

fn one(item: ItemId) -> ItemStack {
    ItemStack::new(item, 1).expect("test dungeon loot item should be registered")
}

#[test]
fn dungeon_loot_matches_the_beta_java_random_sequence() {
    let mut random = JavaRandom::new(1);
    let chest = generate_dungeon_chest(&mut random);
    let expected = std::array::from_fn(|slot| match slot {
        5 => Some(one(ItemId::Redstone)),
        10 => Some(ItemStack::new(ItemId::Wheat, 4).unwrap()),
        12 => Some(one(ItemId::Saddle)),
        16 => Some(one(ItemId::Bread)),
        19 => Some(one(ItemId::Gunpowder)),
        _ => None,
    });

    // Slot 16 originally receives a bucket and is then overwritten by bread.
    assert_eq!(chest.slots, expected);
    assert_eq!(random.state(), 137_102_909_728_704);

    for (seed, expected_state, rare_item) in [
        (79, 124_265_925_996_201, ItemId::RecordCat),
        (66, 239_990_216_468_994, ItemId::Record13),
        (393, 261_165_909_313_586, ItemId::GoldenApple),
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
                ItemId::IronIngot
                | ItemId::Wheat
                | ItemId::Gunpowder
                | ItemId::String
                | ItemId::Redstone => {
                    assert!((1..=4).contains(&stack.count()));
                    assert_eq!(stack.data(), 0);
                }
                ItemId::Saddle
                | ItemId::Bread
                | ItemId::Bucket
                | ItemId::GoldenApple
                | ItemId::Record13
                | ItemId::RecordCat => {
                    assert_eq!(stack.count(), 1);
                    assert_eq!(stack.data(), 0);
                }
                ItemId::Dye => {
                    assert_eq!(stack.count(), 1);
                    assert_eq!(stack.data(), 3);
                }
                item => panic!("non-Beta dungeon loot item: {item:?}"),
            }
            seen.insert((stack.item(), stack.data()));
        }
    }

    let expected = HashSet::from([
        (ItemId::Saddle, 0),
        (ItemId::IronIngot, 0),
        (ItemId::Bread, 0),
        (ItemId::Wheat, 0),
        (ItemId::Gunpowder, 0),
        (ItemId::String, 0),
        (ItemId::Bucket, 0),
        (ItemId::GoldenApple, 0),
        (ItemId::Redstone, 0),
        (ItemId::Record13, 0),
        (ItemId::RecordCat, 0),
        (ItemId::Dye, 3),
    ]);
    assert_eq!(seen, expected);
}

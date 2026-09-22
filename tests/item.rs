use game::inventory::Hotbar;
use game::item::ItemData;
use game::item::ItemId;
use game::item::ItemRegistry;
use game::item::ItemStack;
use game::item::StackError;
use game::world::block::block::BlockId;
use game::world::block::registry::BLOCK_DEFINITIONS;
use game::world::block::registry::BetaBlockId;
use game::world::block::registry::BetaBlockState;
use game::world::block::registry::block_definition;

#[test]
fn registries_cover_beta_ranges_without_registering_holes_or_air_items() {
    assert_eq!(BLOCK_DEFINITIONS.len(), 97);
    assert_eq!(ItemRegistry::iter().count(), 202);
    for raw in 0..=u16::MAX {
        let item = ItemRegistry::get(ItemId(raw));
        assert_eq!(
            item.is_some(),
            matches!(raw, 1..=96 | 256..=359 | 2256..=2257)
        );
        if let Some(item) = item {
            assert_eq!(item.id, ItemId(raw));
            assert!(!item.name.is_empty());
            assert!((1..=64).contains(&item.max_stack_size));
        }
    }
    for raw in 0..=u8::MAX {
        assert_eq!(block_definition(raw).is_some(), raw <= 96);
    }
}

#[test]
fn known_beta_identities_do_not_use_the_native_variant_ids() {
    assert_eq!(BetaBlockId::CAKE.as_u8(), 92);
    assert_eq!(BetaBlockId::REPEATER.as_u8(), 93);
    assert_eq!(BetaBlockId::POWERED_REPEATER.as_u8(), 94);
    assert_eq!(BetaBlockId::LOCKED_CHEST.as_u8(), 95);
    assert_eq!(BetaBlockId::TRAPDOOR.as_u8(), 96);
    assert_eq!(ItemId::IRON_SHOVEL.0, 256);
    assert_eq!(ItemId::DIAMOND.0, 264);
    assert_eq!(ItemId::WOODEN_DOOR.0, 324);
    assert_eq!(ItemId::CAKE.0, 354);
    assert_eq!(ItemId::SHEARS.0, 359);
    assert_eq!(ItemId::RECORD13.0, 2256);
    assert_eq!(ItemId::RECORD_CAT.0, 2257);
    assert_eq!(ItemId::from_block(BetaBlockId::STONE).0, 1);
    assert_eq!(ItemRegistry::get(ItemId(257)).unwrap().name, "iron_pickaxe");
}

#[test]
fn native_save_values_and_supported_states_round_trip() {
    assert_eq!(BlockId::from_u8(200), Some(BlockId::SpruceLeaves));
    assert_eq!(BlockId::from_u8(204), Some(BlockId::TorchWest));
    for raw in 0..=u8::MAX {
        if let Some(block) = BlockId::from_u8(raw) {
            assert_eq!(block.as_u8(), raw);
            assert_eq!(block.beta_state().runtime_block(), Some(block));
        }
    }
    for id in [BetaBlockId::CAKE, BetaBlockId::TRAPDOOR, BetaBlockId::GLASS] {
        assert_eq!(BetaBlockState { id, metadata: 0 }.runtime_block(), None);
        assert_eq!(
            ItemStack::new(ItemId::from_block(id), 1)
                .unwrap()
                .runtime_block(),
            None
        );
    }
    assert_eq!(
        BetaBlockState {
            id: BetaBlockId::WOOD,
            metadata: 15
        }
        .runtime_block(),
        None
    );
}

#[test]
fn stacks_reject_unknown_ids_zero_overflow_and_invalid_data() {
    assert_eq!(
        ItemStack::new(ItemId(0), 1),
        Err(StackError::UnknownItem(ItemId(0)))
    );
    assert!(ItemStack::new(ItemId::COAL, 0).is_err());
    assert!(ItemStack::new(ItemId::COAL, 65).is_err());
    assert!(ItemStack::new(ItemId::IRON_PICKAXE, 2).is_err());
    assert!(ItemStack::with_data(ItemId::COAL, 1, 2).is_err());
    assert!(ItemStack::with_data(ItemId::DIAMOND, 1, 1).is_err());
    assert!(ItemStack::with_data(ItemId::IRON_PICKAXE, 1, 250).is_ok());
    assert!(ItemStack::with_data(ItemId::IRON_PICKAXE, 1, 251).is_err());
    assert!(ItemStack::with_data(ItemId::MAP, 1, 1234).is_ok());
}

#[test]
fn beta_stack_and_durability_rules_include_pre_release_differences() {
    for (id, limit) in [
        (ItemId::APPLE, 1),
        (ItemId::COOKIE, 8),
        (ItemId::EGG, 16),
        (ItemId::SNOWBALL, 16),
        (ItemId::BUCKET, 1),
        (ItemId::SIGN, 1),
        (ItemId::ARROW, 64),
        (ItemId::BOW, 1),
        (ItemId::RECORD13, 1),
    ] {
        assert_eq!(id.definition().unwrap().max_stack_size, limit);
    }
    for (id, damage) in [
        (ItemId::WOODEN_PICKAXE, 59),
        (ItemId::STONE_PICKAXE, 131),
        (ItemId::IRON_PICKAXE, 250),
        (ItemId::DIAMOND_PICKAXE, 1561),
        (ItemId::GOLD_PICKAXE, 32),
        (ItemId::SHEARS, 238),
        (ItemId::LEATHER_HELMET, 33),
        (ItemId::DIAMOND_CHESTPLATE, 384),
    ] {
        assert_eq!(id.definition().unwrap().data, ItemData::Durability(damage));
    }
    // Beta bows have no durability.
    assert_eq!(ItemId::BOW.definition().unwrap().data, ItemData::None);
}

#[test]
fn species_survive_stacks_but_torch_attachments_do_not() {
    let spruce = ItemStack::from_block(BlockId::SpruceWood, 4).unwrap();
    assert_eq!(spruce.item(), ItemId(17));
    assert_eq!(spruce.data(), 1);
    assert_eq!(spruce.runtime_block(), Some(BlockId::SpruceWood));
    let birch = ItemStack::from_block(BlockId::BirchLeaves, 4).unwrap();
    assert_eq!(birch.item(), ItemId(18));
    assert_eq!(birch.data(), 2);
    assert_eq!(birch.runtime_block(), Some(BlockId::BirchLeaves));
    let torch = ItemStack::from_block(BlockId::TorchEast, 4).unwrap();
    assert_eq!(torch.item(), ItemId(50));
    assert_eq!(torch.data(), 0);
    assert_eq!(torch.runtime_block(), Some(BlockId::Torch));
    assert!(ItemStack::from_block(BlockId::Air, 1).is_err());
}

#[test]
fn insertion_merges_before_empty_slots_and_preserves_subtypes() {
    let mut hotbar = Hotbar::default();
    hotbar.slots[4] = Some(ItemStack::new(ItemId::COAL, 60).unwrap());
    assert_eq!(
        hotbar.insert(ItemStack::new(ItemId::COAL, 10).unwrap()),
        None
    );
    assert_eq!(hotbar.slots[4].unwrap().count(), 64);
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
    let charcoal = ItemStack::with_data(ItemId::COAL, 10, 1).unwrap();
    assert_eq!(hotbar.insert(charcoal), None);
    assert_eq!(hotbar.slots[1], Some(charcoal));
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
}

#[test]
fn full_hotbar_returns_exact_remainder_and_does_not_stack_tools() {
    let coal = ItemStack::new(ItemId::COAL, 64).unwrap();
    let mut hotbar = Hotbar {
        slots: [Some(coal); 9],
        selected: 0,
        pop: [0; 9],
    };
    hotbar.slots[8] = Some(ItemStack::new(ItemId::COAL, 63).unwrap());
    let remainder = hotbar
        .insert(ItemStack::new(ItemId::COAL, 10).unwrap())
        .unwrap();
    assert_eq!(remainder.count(), 9);
    assert_eq!(hotbar.slots[8], Some(coal));
    let tool = ItemStack::new(ItemId::IRON_PICKAXE, 1).unwrap();
    hotbar.slots = [Some(tool); 9];
    assert_eq!(hotbar.insert(tool), Some(tool));
}

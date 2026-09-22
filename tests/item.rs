use game::inventory::Hotbar;
use game::item::ItemData;
use game::item::ItemId;
use game::item::ItemRegistry;
use game::item::ItemStack;
use game::item::StackError;
use game::item::tools::break_durability;
use game::item::tools::can_harvest;
use game::item::tools::ticks_to_break;
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

fn held(id: ItemId) -> Option<ItemStack> {
    Some(ItemStack::new(id, 1).unwrap())
}

fn break_ticks(
    tool: Option<ItemId>,
    block: BlockId,
    on_ground: bool,
    in_water: bool,
) -> Option<u32> {
    ticks_to_break(
        block,
        tool.map(|id| ItemStack::new(id, 1).unwrap()),
        on_ground,
        in_water,
    )
}

#[test]
fn tool_break_times_match_beta_173() {
    let t = |tool, block| break_ticks(tool, block, true, false);
    assert_eq!(t(None, BlockId::Dirt), Some(15));
    assert_eq!(t(None, BlockId::Stone), Some(150));
    assert_eq!(t(None, BlockId::Obsidian), Some(1000));
    assert_eq!(t(None, BlockId::Leaves), Some(6));

    assert_eq!(t(Some(ItemId::WOODEN_PICKAXE), BlockId::Stone), Some(23));
    assert_eq!(t(Some(ItemId::STONE_PICKAXE), BlockId::Stone), Some(12));
    assert_eq!(t(Some(ItemId::IRON_PICKAXE), BlockId::Stone), Some(8));
    assert_eq!(t(Some(ItemId::DIAMOND_PICKAXE), BlockId::Stone), Some(6));
    assert_eq!(t(Some(ItemId::GOLD_PICKAXE), BlockId::Stone), Some(4));

    // Obsidian is harvestable by diamond but is not in the speed list.
    assert_eq!(
        t(Some(ItemId::DIAMOND_PICKAXE), BlockId::Obsidian),
        Some(300)
    );
    assert_eq!(t(Some(ItemId::IRON_PICKAXE), BlockId::GoldOre), Some(15));
    assert_eq!(t(Some(ItemId::WOODEN_PICKAXE), BlockId::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::GOLD_PICKAXE), BlockId::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::WOODEN_PICKAXE), BlockId::CoalOre), Some(45));
    assert_eq!(t(Some(ItemId::DIAMOND_PICKAXE), BlockId::CoalOre), Some(12));
    assert_eq!(
        t(Some(ItemId::IRON_PICKAXE), BlockId::RedstoneOre),
        Some(90)
    );
    assert_eq!(
        t(Some(ItemId::IRON_PICKAXE), BlockId::LitRedstoneOre),
        Some(90)
    );
    assert_eq!(t(Some(ItemId::WOODEN_PICKAXE), BlockId::Furnace), Some(105));
    assert_eq!(t(Some(ItemId::GOLD_PICKAXE), BlockId::Dispenser), Some(105));
    assert_eq!(
        t(Some(ItemId::STONE_PICKAXE), BlockId::LitFurnace),
        Some(105)
    );
    assert_eq!(t(Some(ItemId::IRON_PICKAXE), BlockId::Glowstone), Some(9));

    assert_eq!(t(Some(ItemId::DIAMOND_SHOVEL), BlockId::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::GOLD_SHOVEL), BlockId::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::WOODEN_SHOVEL), BlockId::Grass), Some(9));
    assert_eq!(t(Some(ItemId::WOODEN_SHOVEL), BlockId::Snow), Some(3));
    assert_eq!(t(Some(ItemId::STONE_SHOVEL), BlockId::Snow), Some(2));
    assert_eq!(t(Some(ItemId::IRON_SHOVEL), BlockId::Snow), Some(1));
    assert_eq!(t(Some(ItemId::DIAMOND_SHOVEL), BlockId::Snow), Some(1));
    assert_eq!(t(Some(ItemId::GOLD_SHOVEL), BlockId::Snow), Some(1));

    assert_eq!(t(Some(ItemId::SHEARS), BlockId::Leaves), Some(1));
    assert_eq!(t(Some(ItemId::SHEARS), BlockId::SpruceLeaves), Some(1));
    assert_eq!(t(Some(ItemId::SHEARS), BlockId::BirchLeaves), Some(1));
    assert_eq!(t(Some(ItemId::SHEARS), BlockId::Wool), Some(5));

    assert_eq!(t(Some(ItemId::IRON_SWORD), BlockId::Dirt), Some(10));
    assert_eq!(t(Some(ItemId::IRON_SWORD), BlockId::Stone), Some(150));
    assert_eq!(t(Some(ItemId::WOODEN_AXE), BlockId::Wood), Some(30));
    assert_eq!(t(Some(ItemId::DIAMOND_AXE), BlockId::Wood), Some(8));
    assert_eq!(t(Some(ItemId::DIAMOND_AXE), BlockId::SpruceWood), Some(8));
    assert_eq!(t(Some(ItemId::WOODEN_AXE), BlockId::BirchWood), Some(30));
    assert_eq!(t(Some(ItemId::WOODEN_AXE), BlockId::Bookshelf), Some(23));
    assert_eq!(
        t(Some(ItemId::DIAMOND_AXE), BlockId::CraftingTable),
        Some(75)
    );
    assert_eq!(t(None, BlockId::CraftingTable), Some(75));
    assert_eq!(
        t(Some(ItemId::IRON_SWORD), BlockId::CraftingTable),
        Some(50)
    );
    assert_eq!(t(Some(ItemId::DIAMOND_AXE), BlockId::NoteBlock), Some(24));
    assert_eq!(t(None, BlockId::NoteBlock), Some(24));
    assert_eq!(t(Some(ItemId::DIAMOND_AXE), BlockId::Jukebox), Some(60));
    assert_eq!(t(None, BlockId::Jukebox), Some(60));
    assert_eq!(t(Some(ItemId::DIAMOND_AXE), BlockId::Pumpkin), Some(30));
    assert_eq!(t(None, BlockId::Pumpkin), Some(30));

    assert_eq!(
        break_ticks(Some(ItemId::DIAMOND_PICKAXE), BlockId::Stone, false, false),
        Some(29)
    );
    assert_eq!(
        break_ticks(Some(ItemId::DIAMOND_PICKAXE), BlockId::Stone, false, true),
        Some(141)
    );
    assert_eq!(break_ticks(None, BlockId::Stone, false, true), Some(150));
    assert_eq!(
        break_ticks(Some(ItemId::WOODEN_PICKAXE), BlockId::Obsidian, false, true),
        Some(1000)
    );
    assert_eq!(
        break_ticks(Some(ItemId::WOODEN_PICKAXE), BlockId::Stone, false, false),
        Some(113)
    );

    let worn = ItemStack::with_data(ItemId::DIAMOND_PICKAXE, 1, 1000).unwrap();
    assert_eq!(
        ticks_to_break(BlockId::Stone, Some(worn), true, false),
        t(Some(ItemId::DIAMOND_PICKAXE), BlockId::Stone)
    );
}

#[test]
fn pickaxe_harvest_levels_match_beta() {
    let wood = held(ItemId::WOODEN_PICKAXE);
    let gold = held(ItemId::GOLD_PICKAXE);
    let stone = held(ItemId::STONE_PICKAXE);
    let iron = held(ItemId::IRON_PICKAXE);
    let diamond = held(ItemId::DIAMOND_PICKAXE);

    for pick in [wood, gold, stone, iron, diamond] {
        assert!(can_harvest(pick, BlockId::Stone));
        assert!(can_harvest(pick, BlockId::CoalOre));
        assert!(can_harvest(pick, BlockId::Cobblestone));
        assert!(can_harvest(pick, BlockId::Netherrack));
        assert!(can_harvest(pick, BlockId::Glowstone));
        assert!(can_harvest(pick, BlockId::Furnace));
        assert!(can_harvest(pick, BlockId::Dispenser));
        assert!(can_harvest(pick, BlockId::Bricks));
    }
    for pick in [wood, gold] {
        assert!(!can_harvest(pick, BlockId::IronOre));
        assert!(!can_harvest(pick, BlockId::IronBlock));
        assert!(!can_harvest(pick, BlockId::LapisOre));
        assert!(!can_harvest(pick, BlockId::LapisBlock));
        assert!(!can_harvest(pick, BlockId::GoldOre));
        assert!(!can_harvest(pick, BlockId::GoldBlock));
        assert!(!can_harvest(pick, BlockId::DiamondOre));
        assert!(!can_harvest(pick, BlockId::DiamondBlock));
        assert!(!can_harvest(pick, BlockId::RedstoneOre));
        assert!(!can_harvest(pick, BlockId::LitRedstoneOre));
        assert!(!can_harvest(pick, BlockId::Obsidian));
    }
    assert!(can_harvest(stone, BlockId::IronOre));
    assert!(can_harvest(stone, BlockId::IronBlock));
    assert!(can_harvest(stone, BlockId::LapisOre));
    assert!(can_harvest(stone, BlockId::LapisBlock));
    assert!(!can_harvest(stone, BlockId::GoldOre));
    assert!(!can_harvest(stone, BlockId::GoldBlock));
    assert!(!can_harvest(stone, BlockId::DiamondOre));
    assert!(!can_harvest(stone, BlockId::DiamondBlock));
    assert!(!can_harvest(stone, BlockId::RedstoneOre));
    assert!(!can_harvest(stone, BlockId::Obsidian));

    assert!(can_harvest(iron, BlockId::GoldOre));
    assert!(can_harvest(iron, BlockId::GoldBlock));
    assert!(can_harvest(iron, BlockId::DiamondOre));
    assert!(can_harvest(iron, BlockId::DiamondBlock));
    assert!(can_harvest(iron, BlockId::RedstoneOre));
    assert!(can_harvest(iron, BlockId::LitRedstoneOre));
    assert!(!can_harvest(iron, BlockId::Obsidian));
    assert!(can_harvest(diamond, BlockId::Obsidian));

    assert!(!can_harvest(None, BlockId::Stone));
    assert!(can_harvest(None, BlockId::Dirt));
    assert!(can_harvest(held(ItemId::WOODEN_SHOVEL), BlockId::Snow));
    assert!(!can_harvest(wood, BlockId::Snow));
    assert!(!can_harvest(None, BlockId::Snow));
    assert!(!can_harvest(held(ItemId::DIAMOND_SWORD), BlockId::Stone));
    assert!(can_harvest(None, BlockId::Leaves));
    assert!(can_harvest(held(ItemId::SHEARS), BlockId::Leaves));
}

#[test]
fn block_breaks_spend_beta_durability() {
    let pick = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).unwrap();
    assert_eq!(break_durability(pick, BlockId::Stone), 1);
    assert_eq!(break_durability(pick, BlockId::Dirt), 1);
    assert_eq!(pick.apply_damage(1).unwrap().data(), 1);

    let sword = ItemStack::new(ItemId::WOODEN_SWORD, 1).unwrap();
    assert_eq!(break_durability(sword, BlockId::Dirt), 2);
    assert_eq!(break_durability(sword, BlockId::Stone), 2);

    let shears = ItemStack::new(ItemId::SHEARS, 1).unwrap();
    assert_eq!(break_durability(shears, BlockId::Leaves), 1);
    assert_eq!(break_durability(shears, BlockId::BirchLeaves), 1);
    assert_eq!(break_durability(shears, BlockId::Wool), 0);
    assert_eq!(break_durability(shears, BlockId::Stone), 0);

    let hoe = ItemStack::new(ItemId::WOODEN_HOE, 1).unwrap();
    assert_eq!(break_durability(hoe, BlockId::Dirt), 0);
    let dirt = ItemStack::from_block(BlockId::Dirt, 1).unwrap();
    assert_eq!(break_durability(dirt, BlockId::Dirt), 0);
    assert_eq!(dirt.apply_damage(1), Some(dirt));

    let mut pick = ItemStack::new(ItemId::WOODEN_PICKAXE, 1).unwrap();
    for use_index in 1..=59 {
        pick = pick.apply_damage(1).expect("wooden pick survives 59 uses");
        assert_eq!(pick.data(), use_index);
    }
    assert!(pick.apply_damage(1).is_none());

    let sword = ItemStack::with_data(ItemId::WOODEN_SWORD, 1, 57).unwrap();
    assert_eq!(sword.apply_damage(2).unwrap().data(), 59);
    let sword = ItemStack::with_data(ItemId::WOODEN_SWORD, 1, 58).unwrap();
    assert!(sword.apply_damage(2).is_none());

    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(ItemStack::with_data(ItemId::WOODEN_PICKAXE, 1, 58).unwrap());
    hotbar.damage_selected(1);
    assert_eq!(hotbar.selected_stack().unwrap().data(), 59);
    hotbar.damage_selected(1);
    assert!(hotbar.selected_stack().is_none());
}

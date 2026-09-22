use game::inventory::Hotbar;
use game::item::ItemData;
use game::item::ItemId;
use game::item::ItemRegistry;
use game::item::ItemStack;
use game::item::StackError;
use game::item::tools::break_durability;
use game::item::tools::can_harvest;
use game::item::tools::str_vs_block;
use game::item::tools::ticks_to_break;
use game::world::block::block::BlockId;

#[test]
fn registries_cover_beta_ranges_without_registering_holes_or_air_items() {
    assert_eq!(
        (0..=u8::MAX)
            .filter(|raw| BlockId::from_u8(*raw).is_some())
            .count(),
        114
    );
    assert_eq!(ItemRegistry::iter().count(), 202);
    for raw in 0..=u16::MAX {
        let item = ItemRegistry::get(raw);
        assert_eq!(
            item.is_some(),
            matches!(raw, 1..=96 | 256..=359 | 2256..=2257)
        );
        if let Some(item) = item {
            assert_eq!(item.id.as_u16(), raw);
            assert!(!item.name.is_empty());
            assert!((1..=64).contains(&item.max_stack_size));
        }
    }
    for raw in 0..=u8::MAX {
        match BlockId::from_u8(raw) {
            Some(block) => {
                assert_eq!(block.as_u8(), raw);
                assert!(!block.name().is_empty());
            }
            None => assert!(raw > 96 && !(200..=208).contains(&raw) && !(209..=216).contains(&raw)),
        }
    }
}

#[test]
fn known_beta_identities_do_not_use_the_native_variant_ids() {
    assert_eq!(BlockId::Cake.as_u8(), 92);
    assert_eq!(BlockId::Repeater.as_u8(), 93);
    assert_eq!(BlockId::PoweredRepeater.as_u8(), 94);
    assert_eq!(BlockId::LockedChest.as_u8(), 95);
    assert_eq!(BlockId::Trapdoor.as_u8(), 96);
    assert_eq!(BlockId::SpruceLeaves.as_u8(), 200);
    assert_eq!(ItemId::IronShovel.as_u16(), 256);
    assert_eq!(ItemId::Diamond.as_u16(), 264);
    assert_eq!(ItemId::WoodenDoor.as_u16(), 324);
    assert_eq!(ItemId::Cake.as_u16(), 354);
    assert_eq!(ItemId::Shears.as_u16(), 359);
    assert_eq!(ItemId::Record13.as_u16(), 2256);
    assert_eq!(ItemId::RecordCat.as_u16(), 2257);
    assert_eq!(ItemId::from_block(BlockId::Stone).unwrap().as_u16(), 1);
    assert!(ItemId::from_block(BlockId::SpruceWood).is_none());
    assert_eq!(ItemRegistry::get(257).unwrap().name, "iron_pickaxe");
}

#[test]
fn native_save_values_and_supported_states_round_trip() {
    assert_eq!(BlockId::from_u8(200), Some(BlockId::SpruceLeaves));
    assert_eq!(BlockId::from_u8(204), Some(BlockId::TorchWest));
    for raw in 0..=u8::MAX {
        if let Some(block) = BlockId::from_u8(raw) {
            assert_eq!(block.as_u8(), raw);
            let (item_block, metadata) = block.item_form();
            let placed = item_block.placed(metadata);
            if matches!(
                block,
                BlockId::TorchWest | BlockId::TorchEast | BlockId::TorchNorth | BlockId::TorchSouth
            ) {
                assert_eq!(placed, Some(BlockId::Torch));
            } else if block.is_furnace() {
                assert_eq!(placed, Some(BlockId::Furnace));
            } else if block.in_world() {
                assert_eq!(placed, Some(block));
            }
        }
    }
    for id in [BlockId::Cake, BlockId::Trapdoor, BlockId::Glass] {
        assert_eq!(id.placed(0), None);
        assert!(!id.in_world());
        assert_eq!(
            ItemStack::new(ItemId::from_block(id).unwrap(), 1)
                .unwrap()
                .runtime_block(),
            None
        );
    }
    assert_eq!(BlockId::Wood.placed(15), None);
}

#[test]
fn stacks_reject_unknown_ids_zero_overflow_and_invalid_data() {
    assert!(ItemId::from_u16(0).is_none());
    assert_eq!(
        ItemStack::new(ItemId::Block(BlockId::Air), 1),
        Err(StackError::UnknownItem(ItemId::Block(BlockId::Air)))
    );
    assert!(ItemStack::new(ItemId::Coal, 0).is_err());
    assert!(ItemStack::new(ItemId::Coal, 65).is_err());
    assert!(ItemStack::new(ItemId::IronPickaxe, 2).is_err());
    assert!(ItemStack::with_data(ItemId::Coal, 1, 2).is_err());
    assert!(ItemStack::with_data(ItemId::Diamond, 1, 1).is_err());
    assert!(ItemStack::with_data(ItemId::IronPickaxe, 1, 250).is_ok());
    assert!(ItemStack::with_data(ItemId::IronPickaxe, 1, 251).is_err());
    assert!(ItemStack::with_data(ItemId::Map, 1, 1234).is_ok());
}

#[test]
fn beta_stack_and_durability_rules_include_pre_release_differences() {
    for (id, limit) in [
        (ItemId::Apple, 1),
        (ItemId::Cookie, 8),
        (ItemId::Egg, 16),
        (ItemId::Snowball, 16),
        (ItemId::Bucket, 1),
        (ItemId::Sign, 1),
        (ItemId::Arrow, 64),
        (ItemId::Bow, 1),
        (ItemId::Record13, 1),
    ] {
        assert_eq!(id.definition().unwrap().max_stack_size, limit);
    }
    for (id, damage) in [
        (ItemId::WoodenPickaxe, 59),
        (ItemId::StonePickaxe, 131),
        (ItemId::IronPickaxe, 250),
        (ItemId::DiamondPickaxe, 1561),
        (ItemId::GoldPickaxe, 32),
        (ItemId::Shears, 238),
        (ItemId::LeatherHelmet, 33),
        (ItemId::DiamondChestplate, 384),
    ] {
        assert_eq!(id.definition().unwrap().data, ItemData::Durability(damage));
    }
    // Beta bows have no durability.
    assert_eq!(ItemId::Bow.definition().unwrap().data, ItemData::None);
}

#[test]
fn species_survive_stacks_but_torch_attachments_do_not() {
    let spruce = ItemStack::from_block(BlockId::SpruceWood, 4).unwrap();
    assert_eq!(spruce.item(), ItemId::Block(BlockId::Wood));
    assert_eq!(spruce.data(), 1);
    assert_eq!(spruce.runtime_block(), Some(BlockId::SpruceWood));
    let birch = ItemStack::from_block(BlockId::BirchLeaves, 4).unwrap();
    assert_eq!(birch.item(), ItemId::Block(BlockId::Leaves));
    assert_eq!(birch.data(), 2);
    assert_eq!(birch.runtime_block(), Some(BlockId::BirchLeaves));
    let torch = ItemStack::from_block(BlockId::TorchEast, 4).unwrap();
    assert_eq!(torch.item(), ItemId::Block(BlockId::Torch));
    assert_eq!(torch.data(), 0);
    assert_eq!(torch.runtime_block(), Some(BlockId::Torch));
    assert!(ItemStack::from_block(BlockId::Air, 1).is_err());
}

#[test]
fn insertion_merges_before_empty_slots_and_preserves_subtypes() {
    let mut hotbar = Hotbar::default();
    hotbar.slots[4] = Some(ItemStack::new(ItemId::Coal, 60).unwrap());
    assert_eq!(
        hotbar.insert(ItemStack::new(ItemId::Coal, 10).unwrap()),
        None
    );
    assert_eq!(hotbar.slots[4].unwrap().count(), 64);
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
    let charcoal = ItemStack::with_data(ItemId::Coal, 10, 1).unwrap();
    assert_eq!(hotbar.insert(charcoal), None);
    assert_eq!(hotbar.slots[1], Some(charcoal));
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
}

#[test]
fn full_hotbar_returns_exact_remainder_and_does_not_stack_tools() {
    let coal = ItemStack::new(ItemId::Coal, 64).unwrap();
    let mut hotbar = Hotbar {
        slots: [Some(coal); 9],
        selected: 0,
        pop: [0; 9],
    };
    hotbar.slots[8] = Some(ItemStack::new(ItemId::Coal, 63).unwrap());
    let remainder = hotbar
        .insert(ItemStack::new(ItemId::Coal, 10).unwrap())
        .unwrap();
    assert_eq!(remainder.count(), 9);
    assert_eq!(hotbar.slots[8], Some(coal));
    let tool = ItemStack::new(ItemId::IronPickaxe, 1).unwrap();
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

    assert_eq!(t(Some(ItemId::WoodenPickaxe), BlockId::Stone), Some(23));
    assert_eq!(t(Some(ItemId::StonePickaxe), BlockId::Stone), Some(12));
    assert_eq!(t(Some(ItemId::IronPickaxe), BlockId::Stone), Some(8));
    assert_eq!(t(Some(ItemId::DiamondPickaxe), BlockId::Stone), Some(6));
    assert_eq!(t(Some(ItemId::GoldPickaxe), BlockId::Stone), Some(4));

    // Obsidian is harvestable by diamond but is not in the speed list.
    assert_eq!(
        t(Some(ItemId::DiamondPickaxe), BlockId::Obsidian),
        Some(300)
    );
    assert_eq!(t(Some(ItemId::IronPickaxe), BlockId::GoldOre), Some(15));
    assert_eq!(t(Some(ItemId::WoodenPickaxe), BlockId::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::GoldPickaxe), BlockId::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::WoodenPickaxe), BlockId::CoalOre), Some(45));
    assert_eq!(t(Some(ItemId::DiamondPickaxe), BlockId::CoalOre), Some(12));
    assert_eq!(t(Some(ItemId::IronPickaxe), BlockId::RedstoneOre), Some(90));
    assert_eq!(
        t(Some(ItemId::IronPickaxe), BlockId::LitRedstoneOre),
        Some(90)
    );
    assert_eq!(t(Some(ItemId::WoodenPickaxe), BlockId::Furnace), Some(105));
    assert_eq!(t(Some(ItemId::GoldPickaxe), BlockId::Dispenser), Some(105));
    assert_eq!(
        t(Some(ItemId::StonePickaxe), BlockId::LitFurnace),
        Some(105)
    );
    assert_eq!(t(Some(ItemId::IronPickaxe), BlockId::Glowstone), Some(9));

    assert_eq!(t(Some(ItemId::DiamondShovel), BlockId::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::GoldShovel), BlockId::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::WoodenShovel), BlockId::Grass), Some(9));
    assert_eq!(t(Some(ItemId::WoodenShovel), BlockId::Snow), Some(3));
    assert_eq!(t(Some(ItemId::StoneShovel), BlockId::Snow), Some(2));
    assert_eq!(t(Some(ItemId::IronShovel), BlockId::Snow), Some(1));
    assert_eq!(t(Some(ItemId::DiamondShovel), BlockId::Snow), Some(1));
    assert_eq!(t(Some(ItemId::GoldShovel), BlockId::Snow), Some(1));

    assert_eq!(t(Some(ItemId::Shears), BlockId::Leaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), BlockId::SpruceLeaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), BlockId::BirchLeaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), BlockId::Wool), Some(5));
    assert_eq!(str_vs_block(held(ItemId::Shears), BlockId::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(ItemId::IronSword), BlockId::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(ItemId::IronSword), BlockId::Dirt), 1.5);

    assert_eq!(t(Some(ItemId::IronSword), BlockId::Dirt), Some(10));
    assert_eq!(t(Some(ItemId::IronSword), BlockId::Stone), Some(150));
    assert_eq!(t(Some(ItemId::WoodenAxe), BlockId::Wood), Some(30));
    assert_eq!(t(Some(ItemId::DiamondAxe), BlockId::Wood), Some(8));
    assert_eq!(t(Some(ItemId::DiamondAxe), BlockId::SpruceWood), Some(8));
    assert_eq!(t(Some(ItemId::WoodenAxe), BlockId::BirchWood), Some(30));
    assert_eq!(t(Some(ItemId::WoodenAxe), BlockId::Bookshelf), Some(23));
    assert_eq!(
        t(Some(ItemId::DiamondAxe), BlockId::CraftingTable),
        Some(75)
    );
    assert_eq!(t(None, BlockId::CraftingTable), Some(75));
    assert_eq!(t(Some(ItemId::IronSword), BlockId::CraftingTable), Some(50));
    assert_eq!(t(Some(ItemId::DiamondAxe), BlockId::NoteBlock), Some(24));
    assert_eq!(t(None, BlockId::NoteBlock), Some(24));
    assert_eq!(t(Some(ItemId::DiamondAxe), BlockId::Jukebox), Some(60));
    assert_eq!(t(None, BlockId::Jukebox), Some(60));
    assert_eq!(t(Some(ItemId::DiamondAxe), BlockId::Pumpkin), Some(30));
    assert_eq!(t(None, BlockId::Pumpkin), Some(30));

    assert_eq!(
        break_ticks(Some(ItemId::DiamondPickaxe), BlockId::Stone, false, false),
        Some(29)
    );
    assert_eq!(
        break_ticks(Some(ItemId::DiamondPickaxe), BlockId::Stone, false, true),
        Some(141)
    );
    assert_eq!(break_ticks(None, BlockId::Stone, false, true), Some(150));
    assert_eq!(
        break_ticks(Some(ItemId::WoodenPickaxe), BlockId::Obsidian, false, true),
        Some(1000)
    );
    assert_eq!(
        break_ticks(Some(ItemId::WoodenPickaxe), BlockId::Stone, false, false),
        Some(113)
    );

    let worn = ItemStack::with_data(ItemId::DiamondPickaxe, 1, 1000).unwrap();
    assert_eq!(
        ticks_to_break(BlockId::Stone, Some(worn), true, false),
        t(Some(ItemId::DiamondPickaxe), BlockId::Stone)
    );
}

#[test]
fn pickaxe_harvest_levels_match_beta() {
    let wood = held(ItemId::WoodenPickaxe);
    let gold = held(ItemId::GoldPickaxe);
    let stone = held(ItemId::StonePickaxe);
    let iron = held(ItemId::IronPickaxe);
    let diamond = held(ItemId::DiamondPickaxe);

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
    assert!(can_harvest(held(ItemId::WoodenShovel), BlockId::Snow));
    assert!(!can_harvest(wood, BlockId::Snow));
    assert!(!can_harvest(None, BlockId::Snow));
    assert!(!can_harvest(held(ItemId::DiamondSword), BlockId::Stone));
    assert!(can_harvest(None, BlockId::Leaves));
    assert!(can_harvest(held(ItemId::Shears), BlockId::Leaves));

    assert!(!can_harvest(None, BlockId::Cobweb));
    assert!(can_harvest(held(ItemId::WoodenSword), BlockId::Cobweb));
    assert!(can_harvest(held(ItemId::Shears), BlockId::Cobweb));
    assert!(!can_harvest(wood, BlockId::Cobweb));
    assert!(!can_harvest(None, BlockId::SnowLayer));
    assert!(can_harvest(held(ItemId::WoodenShovel), BlockId::SnowLayer));
    assert!(!can_harvest(None, BlockId::IronDoor));
    assert!(can_harvest(wood, BlockId::IronDoor));
    assert!(can_harvest(wood, BlockId::StoneSlab));
    assert!(can_harvest(wood, BlockId::CobblestoneStairs));
    assert!(can_harvest(wood, BlockId::StonePressurePlate));
    assert!(!can_harvest(None, BlockId::StoneSlab));
}

#[test]
fn block_breaks_spend_beta_durability() {
    let pick = ItemStack::new(ItemId::WoodenPickaxe, 1).unwrap();
    assert_eq!(break_durability(pick, BlockId::Stone), 1);
    assert_eq!(break_durability(pick, BlockId::Dirt), 1);
    assert_eq!(pick.apply_damage(1).unwrap().data(), 1);

    let sword = ItemStack::new(ItemId::WoodenSword, 1).unwrap();
    assert_eq!(break_durability(sword, BlockId::Dirt), 2);
    assert_eq!(break_durability(sword, BlockId::Stone), 2);

    let shears = ItemStack::new(ItemId::Shears, 1).unwrap();
    assert_eq!(break_durability(shears, BlockId::Leaves), 1);
    assert_eq!(break_durability(shears, BlockId::BirchLeaves), 1);
    assert_eq!(break_durability(shears, BlockId::Wool), 0);
    assert_eq!(break_durability(shears, BlockId::Cobweb), 1);
    assert_eq!(break_durability(shears, BlockId::Stone), 0);

    let hoe = ItemStack::new(ItemId::WoodenHoe, 1).unwrap();
    assert_eq!(break_durability(hoe, BlockId::Dirt), 0);
    let dirt = ItemStack::from_block(BlockId::Dirt, 1).unwrap();
    assert_eq!(break_durability(dirt, BlockId::Dirt), 0);
    assert_eq!(dirt.apply_damage(1), Some(dirt));

    let mut pick = ItemStack::new(ItemId::WoodenPickaxe, 1).unwrap();
    for use_index in 1..=59 {
        pick = pick.apply_damage(1).expect("wooden pick survives 59 uses");
        assert_eq!(pick.data(), use_index);
    }
    assert!(pick.apply_damage(1).is_none());

    let sword = ItemStack::with_data(ItemId::WoodenSword, 1, 57).unwrap();
    assert_eq!(sword.apply_damage(2).unwrap().data(), 59);
    let sword = ItemStack::with_data(ItemId::WoodenSword, 1, 58).unwrap();
    assert!(sword.apply_damage(2).is_none());

    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(ItemStack::with_data(ItemId::WoodenPickaxe, 1, 58).unwrap());
    hotbar.damage_selected(1);
    assert_eq!(hotbar.selected_stack().unwrap().data(), 59);
    hotbar.damage_selected(1);
    assert!(hotbar.selected_stack().is_none());
}

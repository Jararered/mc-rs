use game::block::id::Id;
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
use game::world::biome::Biome;

#[test]
fn numeric_enum_conversions_preserve_valid_and_unknown_values() {
    for raw in 0..=u8::MAX {
        let block = Id::from(raw);
        let round_trip: u8 = block.into();
        assert_eq!(round_trip, raw);
        assert_eq!(Id::from_u8(raw).is_some(), !matches!(block, Id::Unknown(_)));

        let biome = Biome::from(raw);
        let round_trip: u8 = biome.into();
        assert_eq!(round_trip, raw);
        assert_eq!(
            Biome::from_u8(raw).is_some(),
            !matches!(biome, Biome::Unknown(_))
        );
    }

    for raw in 0..=u16::MAX {
        let item = ItemId::from(raw);
        let round_trip: u16 = item.into();
        assert_eq!(round_trip, raw);
        assert_eq!(
            ItemId::from_u16(raw).is_some(),
            matches!(raw, 1..=96 | 256..=359 | 2256..=2257)
        );
        if let ItemId::BlockOrUnknown(value) = item {
            assert_eq!(value, raw);
            assert_eq!(item.block().is_some(), (1..=96).contains(&raw));
        }
    }
}

#[test]
fn registries_cover_beta_ranges_without_registering_holes_or_air_items() {
    assert_eq!(
        (0..=u8::MAX)
            .filter(|raw| Id::from_u8(*raw).is_some())
            .count(),
        128
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
            assert!((1..=64).contains(&item.max_stack_size));
        }
    }
    for raw in 0..=u8::MAX {
        match Id::from_u8(raw) {
            Some(block) => {
                assert_eq!(block.as_u8(), raw);
            }
            None => assert!(raw > 96 && !(200..=208).contains(&raw) && !(209..=218).contains(&raw)),
        }
    }
}

#[test]
fn known_beta_identities_do_not_use_the_native_variant_ids() {
    assert_eq!(Id::Cake.as_u8(), 92);
    assert_eq!(Id::Repeater.as_u8(), 93);
    assert_eq!(Id::PoweredRepeater.as_u8(), 94);
    assert_eq!(Id::LockedChest.as_u8(), 95);
    assert_eq!(Id::Trapdoor.as_u8(), 96);
    assert_eq!(Id::SpruceLeaves.as_u8(), 200);
    assert_eq!(ItemId::IronShovel.as_u16(), 256);
    assert_eq!(ItemId::Diamond.as_u16(), 264);
    assert_eq!(ItemId::WoodenDoor.as_u16(), 324);
    assert_eq!(ItemId::Cake.as_u16(), 354);
    assert_eq!(ItemId::Shears.as_u16(), 359);
    assert_eq!(ItemId::Record13.as_u16(), 2256);
    assert_eq!(ItemId::RecordCat.as_u16(), 2257);
    assert_eq!(ItemId::from_block(Id::Stone).unwrap().as_u16(), 1);
    assert!(ItemId::from_block(Id::SpruceWood).is_none());
    assert_eq!(
        ItemRegistry::get(257).unwrap().id.to_string(),
        "IronPickaxe"
    );
}

#[test]
fn item_ids_format_as_their_rust_variants() {
    assert_eq!(ItemId::from_block(Id::Stone).unwrap().to_string(), "Stone");
    assert_eq!(ItemId::IronPickaxe.to_string(), "IronPickaxe");
    assert_eq!(
        ItemId::BlockOrUnknown(999).to_string(),
        "BlockOrUnknown(999)"
    );
}

#[test]
fn native_save_values_and_supported_states_round_trip() {
    assert_eq!(Id::from_u8(200), Some(Id::SpruceLeaves));
    assert_eq!(Id::from_u8(204), Some(Id::TorchWest));
    for raw in 0..=u8::MAX {
        if let Some(block) = Id::from_u8(raw) {
            assert_eq!(block.as_u8(), raw);
            let (item_block, metadata) = block.item_form();
            let placed = item_block.placed(metadata);
            if matches!(
                block,
                Id::TorchWest | Id::TorchEast | Id::TorchNorth | Id::TorchSouth
            ) {
                assert_eq!(placed, Some(Id::Torch));
            } else if matches!(
                block,
                Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest
            ) {
                assert_eq!(placed, Some(Id::Pumpkin));
            } else if block.is_furnace() {
                assert_eq!(placed, Some(Id::Furnace));
            } else if block.is_chest() {
                assert_eq!(placed, Some(Id::Chest));
            } else if block.is_ladder() && block != Id::Ladder {
                assert_eq!(placed, Some(Id::Ladder));
            } else if block.in_world() {
                assert_eq!(placed, Some(block));
            }
        }
    }
    for id in [Id::Cake, Id::Trapdoor, Id::Glass] {
        assert_eq!(id.placed(0), None);
        assert!(!id.in_world());
        assert_eq!(
            ItemStack::new(ItemId::from_block(id).unwrap(), 1)
                .unwrap()
                .runtime_block(),
            None
        );
    }
    assert_eq!(Id::Wood.placed(15), None);
}

#[test]
fn stacks_reject_unknown_ids_zero_overflow_and_invalid_data() {
    assert!(ItemId::from_u16(0).is_none());
    assert_eq!(
        ItemStack::new(ItemId::BlockOrUnknown(0), 1),
        Err(StackError::UnknownItem(ItemId::BlockOrUnknown(0)))
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
        assert_eq!(id.properties().unwrap().max_stack_size, limit);
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
        assert_eq!(id.properties().unwrap().data, ItemData::Durability(damage));
    }
    // Beta bows have no durability.
    assert_eq!(ItemId::Bow.properties().unwrap().data, ItemData::None);
}

#[test]
fn species_survive_stacks_but_torch_attachments_do_not() {
    let spruce = ItemStack::from_block(Id::SpruceWood, 4).unwrap();
    assert_eq!(spruce.item(), ItemId::from_block(Id::Wood).unwrap());
    assert_eq!(spruce.data(), 1);
    assert_eq!(spruce.runtime_block(), Some(Id::SpruceWood));
    let birch = ItemStack::from_block(Id::BirchLeaves, 4).unwrap();
    assert_eq!(birch.item(), ItemId::from_block(Id::Leaves).unwrap());
    assert_eq!(birch.data(), 2);
    assert_eq!(birch.runtime_block(), Some(Id::BirchLeaves));
    let spruce_planks = ItemStack::from_block(Id::SprucePlanks, 4).unwrap();
    assert_eq!(
        spruce_planks.item(),
        ItemId::from_block(Id::WoodenPlanks).unwrap()
    );
    assert_eq!(spruce_planks.data(), 1);
    assert_eq!(spruce_planks.runtime_block(), Some(Id::SprucePlanks));
    let birch_planks = ItemStack::from_block(Id::BirchPlanks, 4).unwrap();
    assert_eq!(
        birch_planks.item(),
        ItemId::from_block(Id::WoodenPlanks).unwrap()
    );
    assert_eq!(birch_planks.data(), 2);
    assert_eq!(birch_planks.runtime_block(), Some(Id::BirchPlanks));
    let torch = ItemStack::from_block(Id::TorchEast, 4).unwrap();
    assert_eq!(torch.item(), ItemId::from_block(Id::Torch).unwrap());
    assert_eq!(torch.data(), 0);
    assert_eq!(torch.runtime_block(), Some(Id::Torch));
    assert!(ItemStack::from_block(Id::Air, 1).is_err());
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

fn break_ticks(tool: Option<ItemId>, block: Id, on_ground: bool, in_water: bool) -> Option<u32> {
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
    assert_eq!(t(None, Id::Dirt), Some(15));
    assert_eq!(t(None, Id::Stone), Some(150));
    assert_eq!(t(None, Id::Obsidian), Some(1000));
    assert_eq!(t(None, Id::Leaves), Some(6));

    assert_eq!(t(Some(ItemId::WoodenPickaxe), Id::Stone), Some(23));
    assert_eq!(t(Some(ItemId::StonePickaxe), Id::Stone), Some(12));
    assert_eq!(t(Some(ItemId::IronPickaxe), Id::Stone), Some(8));
    assert_eq!(t(Some(ItemId::DiamondPickaxe), Id::Stone), Some(6));
    assert_eq!(t(Some(ItemId::GoldPickaxe), Id::Stone), Some(4));

    // Obsidian is harvestable by diamond but is not in the speed list.
    assert_eq!(t(Some(ItemId::DiamondPickaxe), Id::Obsidian), Some(300));
    assert_eq!(t(Some(ItemId::IronPickaxe), Id::GoldOre), Some(15));
    assert_eq!(t(Some(ItemId::WoodenPickaxe), Id::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::GoldPickaxe), Id::GoldOre), Some(300));
    assert_eq!(t(Some(ItemId::WoodenPickaxe), Id::CoalOre), Some(45));
    assert_eq!(t(Some(ItemId::DiamondPickaxe), Id::CoalOre), Some(12));
    assert_eq!(t(Some(ItemId::IronPickaxe), Id::RedstoneOre), Some(90));
    assert_eq!(t(Some(ItemId::IronPickaxe), Id::LitRedstoneOre), Some(90));
    assert_eq!(t(Some(ItemId::WoodenPickaxe), Id::Furnace), Some(105));
    assert_eq!(t(Some(ItemId::GoldPickaxe), Id::Dispenser), Some(105));
    assert_eq!(t(Some(ItemId::StonePickaxe), Id::LitFurnace), Some(105));
    assert_eq!(t(Some(ItemId::IronPickaxe), Id::Glowstone), Some(9));

    assert_eq!(t(Some(ItemId::DiamondShovel), Id::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::GoldShovel), Id::Dirt), Some(2));
    assert_eq!(t(Some(ItemId::WoodenShovel), Id::Grass), Some(9));
    assert_eq!(t(Some(ItemId::WoodenShovel), Id::Snow), Some(3));
    assert_eq!(t(Some(ItemId::StoneShovel), Id::Snow), Some(2));
    assert_eq!(t(Some(ItemId::IronShovel), Id::Snow), Some(1));
    assert_eq!(t(Some(ItemId::DiamondShovel), Id::Snow), Some(1));
    assert_eq!(t(Some(ItemId::GoldShovel), Id::Snow), Some(1));

    assert_eq!(t(Some(ItemId::Shears), Id::Leaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), Id::SpruceLeaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), Id::BirchLeaves), Some(1));
    assert_eq!(t(Some(ItemId::Shears), Id::Wool), Some(5));
    assert_eq!(str_vs_block(held(ItemId::Shears), Id::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(ItemId::IronSword), Id::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(ItemId::IronSword), Id::Dirt), 1.5);

    assert_eq!(t(Some(ItemId::IronSword), Id::Dirt), Some(10));
    assert_eq!(t(Some(ItemId::IronSword), Id::Stone), Some(150));
    assert_eq!(t(Some(ItemId::WoodenAxe), Id::Wood), Some(30));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::Wood), Some(8));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::SpruceWood), Some(8));
    assert_eq!(t(Some(ItemId::WoodenAxe), Id::BirchWood), Some(30));
    assert_eq!(t(Some(ItemId::WoodenAxe), Id::Bookshelf), Some(23));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::CraftingTable), Some(75));
    assert_eq!(t(None, Id::CraftingTable), Some(75));
    assert_eq!(t(Some(ItemId::IronSword), Id::CraftingTable), Some(50));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::NoteBlock), Some(24));
    assert_eq!(t(None, Id::NoteBlock), Some(24));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::Jukebox), Some(60));
    assert_eq!(t(None, Id::Jukebox), Some(60));
    assert_eq!(t(Some(ItemId::DiamondAxe), Id::Pumpkin), Some(30));
    assert_eq!(t(None, Id::Pumpkin), Some(30));

    assert_eq!(
        break_ticks(Some(ItemId::DiamondPickaxe), Id::Stone, false, false),
        Some(29)
    );
    assert_eq!(
        break_ticks(Some(ItemId::DiamondPickaxe), Id::Stone, false, true),
        Some(141)
    );
    assert_eq!(break_ticks(None, Id::Stone, false, true), Some(150));
    assert_eq!(
        break_ticks(Some(ItemId::WoodenPickaxe), Id::Obsidian, false, true),
        Some(1000)
    );
    assert_eq!(
        break_ticks(Some(ItemId::WoodenPickaxe), Id::Stone, false, false),
        Some(113)
    );

    let worn = ItemStack::with_data(ItemId::DiamondPickaxe, 1, 1000).unwrap();
    assert_eq!(
        ticks_to_break(Id::Stone, Some(worn), true, false),
        t(Some(ItemId::DiamondPickaxe), Id::Stone)
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
        assert!(can_harvest(pick, Id::Stone));
        assert!(can_harvest(pick, Id::CoalOre));
        assert!(can_harvest(pick, Id::Cobblestone));
        assert!(can_harvest(pick, Id::Netherrack));
        assert!(can_harvest(pick, Id::Glowstone));
        assert!(can_harvest(pick, Id::Furnace));
        assert!(can_harvest(pick, Id::Dispenser));
        assert!(can_harvest(pick, Id::Bricks));
    }
    for pick in [wood, gold] {
        assert!(!can_harvest(pick, Id::IronOre));
        assert!(!can_harvest(pick, Id::IronBlock));
        assert!(!can_harvest(pick, Id::LapisOre));
        assert!(!can_harvest(pick, Id::LapisBlock));
        assert!(!can_harvest(pick, Id::GoldOre));
        assert!(!can_harvest(pick, Id::GoldBlock));
        assert!(!can_harvest(pick, Id::DiamondOre));
        assert!(!can_harvest(pick, Id::DiamondBlock));
        assert!(!can_harvest(pick, Id::RedstoneOre));
        assert!(!can_harvest(pick, Id::LitRedstoneOre));
        assert!(!can_harvest(pick, Id::Obsidian));
    }
    assert!(can_harvest(stone, Id::IronOre));
    assert!(can_harvest(stone, Id::IronBlock));
    assert!(can_harvest(stone, Id::LapisOre));
    assert!(can_harvest(stone, Id::LapisBlock));
    assert!(!can_harvest(stone, Id::GoldOre));
    assert!(!can_harvest(stone, Id::GoldBlock));
    assert!(!can_harvest(stone, Id::DiamondOre));
    assert!(!can_harvest(stone, Id::DiamondBlock));
    assert!(!can_harvest(stone, Id::RedstoneOre));
    assert!(!can_harvest(stone, Id::Obsidian));

    assert!(can_harvest(iron, Id::GoldOre));
    assert!(can_harvest(iron, Id::GoldBlock));
    assert!(can_harvest(iron, Id::DiamondOre));
    assert!(can_harvest(iron, Id::DiamondBlock));
    assert!(can_harvest(iron, Id::RedstoneOre));
    assert!(can_harvest(iron, Id::LitRedstoneOre));
    assert!(!can_harvest(iron, Id::Obsidian));
    assert!(can_harvest(diamond, Id::Obsidian));

    assert!(!can_harvest(None, Id::Stone));
    assert!(can_harvest(None, Id::Dirt));
    assert!(can_harvest(held(ItemId::WoodenShovel), Id::Snow));
    assert!(!can_harvest(wood, Id::Snow));
    assert!(!can_harvest(None, Id::Snow));
    assert!(!can_harvest(held(ItemId::DiamondSword), Id::Stone));
    assert!(can_harvest(None, Id::Leaves));
    assert!(can_harvest(held(ItemId::Shears), Id::Leaves));

    assert!(!can_harvest(None, Id::Cobweb));
    assert!(can_harvest(held(ItemId::WoodenSword), Id::Cobweb));
    assert!(can_harvest(held(ItemId::Shears), Id::Cobweb));
    assert!(!can_harvest(wood, Id::Cobweb));
    assert!(!can_harvest(None, Id::SnowLayer));
    assert!(can_harvest(held(ItemId::WoodenShovel), Id::SnowLayer));
    assert!(!can_harvest(None, Id::IronDoor));
    assert!(can_harvest(wood, Id::IronDoor));
    assert!(can_harvest(wood, Id::StoneSlab));
    assert!(can_harvest(wood, Id::CobblestoneStairs));
    assert!(can_harvest(wood, Id::StonePressurePlate));
    assert!(!can_harvest(None, Id::StoneSlab));
}

#[test]
fn block_breaks_spend_beta_durability() {
    let pick = ItemStack::new(ItemId::WoodenPickaxe, 1).unwrap();
    assert_eq!(break_durability(pick, Id::Stone), 1);
    assert_eq!(break_durability(pick, Id::Dirt), 1);
    assert_eq!(pick.apply_damage(1).unwrap().data(), 1);

    let sword = ItemStack::new(ItemId::WoodenSword, 1).unwrap();
    assert_eq!(break_durability(sword, Id::Dirt), 2);
    assert_eq!(break_durability(sword, Id::Stone), 2);

    let shears = ItemStack::new(ItemId::Shears, 1).unwrap();
    assert_eq!(break_durability(shears, Id::Leaves), 1);
    assert_eq!(break_durability(shears, Id::BirchLeaves), 1);
    assert_eq!(break_durability(shears, Id::Wool), 0);
    assert_eq!(break_durability(shears, Id::Cobweb), 1);
    assert_eq!(break_durability(shears, Id::Stone), 0);

    let hoe = ItemStack::new(ItemId::WoodenHoe, 1).unwrap();
    assert_eq!(break_durability(hoe, Id::Dirt), 0);
    let dirt = ItemStack::from_block(Id::Dirt, 1).unwrap();
    assert_eq!(break_durability(dirt, Id::Dirt), 0);
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

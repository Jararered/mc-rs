use game::block::blocks::Block;
use game::block::direction::Direction;
use game::inventory::Hotbar;
use game::item::Item;
use game::item::ItemData;
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
        let block = Block::from(raw);
        let round_trip: u8 = block.into();
        assert_eq!(round_trip, raw);
        assert_eq!(
            Block::from_u8(raw).is_some(),
            !matches!(block, Block::Unknown(_))
        );

        let biome = Biome::from(raw);
        let round_trip: u8 = biome.into();
        assert_eq!(round_trip, raw);
        assert_eq!(
            Biome::from_u8(raw).is_some(),
            !matches!(biome, Biome::Unknown(_))
        );
    }

    for raw in 0..=u16::MAX {
        let item = Item::from(raw);
        let round_trip: u16 = item.into();
        assert_eq!(round_trip, raw);
        assert_eq!(
            Item::from_u16(raw).is_some(),
            matches!(raw, 1..=96 | 256..=359 | 2256..=2257)
        );
        if let Item::BlockOrUnknown(value) = item {
            assert_eq!(value, raw);
            assert_eq!(item.block().is_some(), (1..=96).contains(&raw));
        }
    }
}

#[test]
fn registries_cover_beta_ranges_without_registering_holes_or_air_items() {
    assert_eq!(
        (0..=u8::MAX)
            .filter(|raw| Block::from_u8(*raw).is_some())
            .count(),
        97
    );
    assert_eq!(ItemRegistry::iter().count(), 202);
    for raw in 0..=u16::MAX {
        let item = ItemRegistry::get(raw);
        assert_eq!(
            item.is_some(),
            matches!(raw, 1..=96 | 256..=359 | 2256..=2257)
        );
        if let Some(item) = item {
            assert_eq!(item.item.as_u16(), raw);
            assert!((1..=64).contains(&item.max_stack_size));
        }
    }
    for raw in 0..=u8::MAX {
        match Block::from_u8(raw) {
            Some(block) => {
                assert_eq!(block.as_u8(), raw);
            }
            None => assert!(raw > 96),
        }
    }
}

#[test]
fn known_beta_identities_are_the_only_block_ids() {
    assert_eq!(Block::Cake.as_u8(), 92);
    assert_eq!(Block::Repeater.as_u8(), 93);
    assert_eq!(Block::PoweredRepeater.as_u8(), 94);
    assert_eq!(Block::LockedChest.as_u8(), 95);
    assert_eq!(Block::Trapdoor.as_u8(), 96);
    assert_eq!(Item::IronShovel.as_u16(), 256);
    assert_eq!(Item::Diamond.as_u16(), 264);
    assert_eq!(Item::WoodenDoor.as_u16(), 324);
    assert_eq!(Item::Cake.as_u16(), 354);
    assert_eq!(Item::Shears.as_u16(), 359);
    assert_eq!(Item::Record13.as_u16(), 2256);
    assert_eq!(Item::RecordCat.as_u16(), 2257);
    assert_eq!(Item::from_block(Block::Stone).unwrap().as_u16(), 1);
    assert_eq!(
        ItemRegistry::get(257).unwrap().item.to_string(),
        "IronPickaxe"
    );
}

#[test]
fn item_ids_format_as_their_rust_variants() {
    assert_eq!(Item::from_block(Block::Stone).unwrap().to_string(), "Stone");
    assert_eq!(Item::IronPickaxe.to_string(), "IronPickaxe");
    assert_eq!(Item::BlockOrUnknown(999).to_string(), "BlockOrUnknown(999)");
}

#[test]
fn native_save_values_and_supported_states_round_trip() {
    for raw in 0..=u8::MAX {
        if let Some(block) = Block::from_u8(raw) {
            assert_eq!(block.as_u8(), raw);
            let (item_block, data) = block.item_form(0);
            let placed = item_block.placed(data);
            if block == Block::LitFurnace {
                assert_eq!(placed, Some((Block::Furnace, 0)));
            } else if placed.is_some() {
                assert_eq!(placed, Some((block, 0)));
            }
        }
    }
    for id in [Block::Cake, Block::Trapdoor, Block::Glass] {
        assert_eq!(
            ItemStack::new(Item::from_block(id).unwrap(), 1)
                .unwrap()
                .runtime_block(),
            Some((id, 0))
        );
    }
    assert_eq!(Block::Wood.placed(15), None);
}

#[test]
fn stacks_reject_unknown_ids_zero_overflow_and_invalid_data() {
    assert!(Item::from_u16(0).is_none());
    assert_eq!(
        ItemStack::new(Item::BlockOrUnknown(0), 1),
        Err(StackError::UnknownItem(Item::BlockOrUnknown(0)))
    );
    assert!(ItemStack::new(Item::Coal, 0).is_err());
    assert!(ItemStack::new(Item::Coal, 65).is_err());
    assert!(ItemStack::new(Item::IronPickaxe, 2).is_err());
    assert!(ItemStack::with_data(Item::Coal, 1, 2).is_err());
    assert!(ItemStack::with_data(Item::Diamond, 1, 1).is_err());
    assert!(ItemStack::with_data(Item::IronPickaxe, 1, 250).is_ok());
    assert!(ItemStack::with_data(Item::IronPickaxe, 1, 251).is_err());
    assert!(ItemStack::with_data(Item::Map, 1, 1234).is_ok());
}

#[test]
fn beta_stack_and_durability_rules_include_pre_release_differences() {
    for (id, limit) in [
        (Item::Apple, 1),
        (Item::Cookie, 8),
        (Item::Egg, 16),
        (Item::Snowball, 16),
        (Item::Bucket, 1),
        (Item::Sign, 1),
        (Item::Arrow, 64),
        (Item::Bow, 1),
        (Item::Record13, 1),
    ] {
        assert_eq!(id.properties().unwrap().max_stack_size, limit);
    }
    for (id, damage) in [
        (Item::WoodenPickaxe, 59),
        (Item::StonePickaxe, 131),
        (Item::IronPickaxe, 250),
        (Item::DiamondPickaxe, 1561),
        (Item::GoldPickaxe, 32),
        (Item::Shears, 238),
        (Item::LeatherHelmet, 33),
        (Item::DiamondChestplate, 384),
    ] {
        assert_eq!(id.properties().unwrap().data, ItemData::Durability(damage));
    }
    // Beta bows have no durability.
    assert_eq!(Item::Bow.properties().unwrap().data, ItemData::None);
}

#[test]
fn species_survive_stacks_but_orientation_does_not() {
    let wood = Item::from_block(Block::Wood).unwrap();
    let leaves = Item::from_block(Block::Leaves).unwrap();
    let planks = Item::from_block(Block::WoodenPlanks).unwrap();
    for (block, item, species) in [
        (Block::Wood, wood, 1),
        (Block::Leaves, leaves, 2),
        (Block::WoodenPlanks, planks, 1),
        (Block::WoodenPlanks, planks, 2),
    ] {
        let stack = ItemStack::from_block_state(block, species, 4).unwrap();
        assert_eq!(stack.item(), item);
        assert_eq!(stack.data(), u16::from(species));
        assert_eq!(stack.runtime_block(), Some((block, species)));
    }
    // A leaf's decay flag is not part of the stack.
    let flagged = ItemStack::from_block_state(Block::Leaves, 1 | 8, 4).unwrap();
    assert_eq!(flagged.data(), 1);
    let east = Block::Torch.facing_metadata(Direction::East);
    let torch = ItemStack::from_block_state(Block::Torch, east, 4).unwrap();
    assert_eq!(torch.item(), Item::from_block(Block::Torch).unwrap());
    assert_eq!(torch.data(), 0);
    assert_eq!(torch.runtime_block(), Some((Block::Torch, 0)));
    assert!(ItemStack::from_block(Block::Air, 1).is_err());
}

#[test]
fn insertion_merges_before_empty_slots_and_preserves_subtypes() {
    let mut hotbar = Hotbar::default();
    hotbar.slots[4] = Some(ItemStack::new(Item::Coal, 60).unwrap());
    assert_eq!(hotbar.insert(ItemStack::new(Item::Coal, 10).unwrap()), None);
    assert_eq!(hotbar.slots[4].unwrap().count(), 64);
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
    let charcoal = ItemStack::with_data(Item::Coal, 10, 1).unwrap();
    assert_eq!(hotbar.insert(charcoal), None);
    assert_eq!(hotbar.slots[1], Some(charcoal));
    assert_eq!(hotbar.slots[0].unwrap().count(), 6);
}

#[test]
fn full_hotbar_returns_exact_remainder_and_does_not_stack_tools() {
    let coal = ItemStack::new(Item::Coal, 64).unwrap();
    let mut hotbar = Hotbar {
        slots: [Some(coal); 9],
        selected: 0,
        pop: [0; 9],
    };
    hotbar.slots[8] = Some(ItemStack::new(Item::Coal, 63).unwrap());
    let remainder = hotbar
        .insert(ItemStack::new(Item::Coal, 10).unwrap())
        .unwrap();
    assert_eq!(remainder.count(), 9);
    assert_eq!(hotbar.slots[8], Some(coal));
    let tool = ItemStack::new(Item::IronPickaxe, 1).unwrap();
    hotbar.slots = [Some(tool); 9];
    assert_eq!(hotbar.insert(tool), Some(tool));
}

fn held(item: Item) -> Option<ItemStack> {
    Some(ItemStack::new(item, 1).unwrap())
}

fn break_ticks(tool: Option<Item>, block: Block, on_ground: bool, in_water: bool) -> Option<u32> {
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
    assert_eq!(t(None, Block::Dirt), Some(15));
    assert_eq!(t(None, Block::Stone), Some(150));
    assert_eq!(t(None, Block::Obsidian), Some(1000));
    assert_eq!(t(None, Block::Leaves), Some(6));

    assert_eq!(t(Some(Item::WoodenPickaxe), Block::Stone), Some(23));
    assert_eq!(t(Some(Item::StonePickaxe), Block::Stone), Some(12));
    assert_eq!(t(Some(Item::IronPickaxe), Block::Stone), Some(8));
    assert_eq!(t(Some(Item::DiamondPickaxe), Block::Stone), Some(6));
    assert_eq!(t(Some(Item::GoldPickaxe), Block::Stone), Some(4));

    // Obsidian is harvestable by diamond but is not in the speed list.
    assert_eq!(t(Some(Item::DiamondPickaxe), Block::Obsidian), Some(300));
    assert_eq!(t(Some(Item::IronPickaxe), Block::GoldOre), Some(15));
    assert_eq!(t(Some(Item::WoodenPickaxe), Block::GoldOre), Some(300));
    assert_eq!(t(Some(Item::GoldPickaxe), Block::GoldOre), Some(300));
    assert_eq!(t(Some(Item::WoodenPickaxe), Block::CoalOre), Some(45));
    assert_eq!(t(Some(Item::DiamondPickaxe), Block::CoalOre), Some(12));
    assert_eq!(t(Some(Item::IronPickaxe), Block::RedstoneOre), Some(90));
    assert_eq!(t(Some(Item::IronPickaxe), Block::LitRedstoneOre), Some(90));
    assert_eq!(t(Some(Item::WoodenPickaxe), Block::Furnace), Some(105));
    assert_eq!(t(Some(Item::GoldPickaxe), Block::Dispenser), Some(105));
    assert_eq!(t(Some(Item::StonePickaxe), Block::LitFurnace), Some(105));
    assert_eq!(t(Some(Item::IronPickaxe), Block::Glowstone), Some(9));

    assert_eq!(t(Some(Item::DiamondShovel), Block::Dirt), Some(2));
    assert_eq!(t(Some(Item::GoldShovel), Block::Dirt), Some(2));
    assert_eq!(t(Some(Item::WoodenShovel), Block::Grass), Some(9));
    assert_eq!(t(Some(Item::WoodenShovel), Block::Snow), Some(3));
    assert_eq!(t(Some(Item::StoneShovel), Block::Snow), Some(2));
    assert_eq!(t(Some(Item::IronShovel), Block::Snow), Some(1));
    assert_eq!(t(Some(Item::DiamondShovel), Block::Snow), Some(1));
    assert_eq!(t(Some(Item::GoldShovel), Block::Snow), Some(1));

    assert_eq!(t(Some(Item::Shears), Block::Leaves), Some(1));
    assert_eq!(t(Some(Item::Shears), Block::Wool), Some(5));
    assert_eq!(str_vs_block(held(Item::Shears), Block::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(Item::IronSword), Block::Cobweb), 15.0);
    assert_eq!(str_vs_block(held(Item::IronSword), Block::Dirt), 1.5);

    assert_eq!(t(Some(Item::IronSword), Block::Dirt), Some(10));
    assert_eq!(t(Some(Item::IronSword), Block::Stone), Some(150));
    assert_eq!(t(Some(Item::WoodenAxe), Block::Wood), Some(30));
    assert_eq!(t(Some(Item::DiamondAxe), Block::Wood), Some(8));
    assert_eq!(t(Some(Item::WoodenAxe), Block::Bookshelf), Some(23));
    assert_eq!(t(Some(Item::DiamondAxe), Block::CraftingTable), Some(75));
    assert_eq!(t(None, Block::CraftingTable), Some(75));
    assert_eq!(t(Some(Item::IronSword), Block::CraftingTable), Some(50));
    assert_eq!(t(Some(Item::DiamondAxe), Block::NoteBlock), Some(24));
    assert_eq!(t(None, Block::NoteBlock), Some(24));
    assert_eq!(t(Some(Item::DiamondAxe), Block::Jukebox), Some(60));
    assert_eq!(t(None, Block::Jukebox), Some(60));
    assert_eq!(t(Some(Item::DiamondAxe), Block::Pumpkin), Some(30));
    assert_eq!(t(None, Block::Pumpkin), Some(30));

    assert_eq!(
        break_ticks(Some(Item::DiamondPickaxe), Block::Stone, false, false),
        Some(29)
    );
    assert_eq!(
        break_ticks(Some(Item::DiamondPickaxe), Block::Stone, false, true),
        Some(141)
    );
    assert_eq!(break_ticks(None, Block::Stone, false, true), Some(150));
    assert_eq!(
        break_ticks(Some(Item::WoodenPickaxe), Block::Obsidian, false, true),
        Some(1000)
    );
    assert_eq!(
        break_ticks(Some(Item::WoodenPickaxe), Block::Stone, false, false),
        Some(113)
    );

    let worn = ItemStack::with_data(Item::DiamondPickaxe, 1, 1000).unwrap();
    assert_eq!(
        ticks_to_break(Block::Stone, Some(worn), true, false),
        t(Some(Item::DiamondPickaxe), Block::Stone)
    );
}

#[test]
fn pickaxe_harvest_levels_match_beta() {
    let wood = held(Item::WoodenPickaxe);
    let gold = held(Item::GoldPickaxe);
    let stone = held(Item::StonePickaxe);
    let iron = held(Item::IronPickaxe);
    let diamond = held(Item::DiamondPickaxe);

    for pick in [wood, gold, stone, iron, diamond] {
        assert!(can_harvest(pick, Block::Stone));
        assert!(can_harvest(pick, Block::CoalOre));
        assert!(can_harvest(pick, Block::Cobblestone));
        assert!(can_harvest(pick, Block::Netherrack));
        assert!(can_harvest(pick, Block::Glowstone));
        assert!(can_harvest(pick, Block::Furnace));
        assert!(can_harvest(pick, Block::Dispenser));
        assert!(can_harvest(pick, Block::Bricks));
    }
    for pick in [wood, gold] {
        assert!(!can_harvest(pick, Block::IronOre));
        assert!(!can_harvest(pick, Block::IronBlock));
        assert!(!can_harvest(pick, Block::LapisOre));
        assert!(!can_harvest(pick, Block::LapisBlock));
        assert!(!can_harvest(pick, Block::GoldOre));
        assert!(!can_harvest(pick, Block::GoldBlock));
        assert!(!can_harvest(pick, Block::DiamondOre));
        assert!(!can_harvest(pick, Block::DiamondBlock));
        assert!(!can_harvest(pick, Block::RedstoneOre));
        assert!(!can_harvest(pick, Block::LitRedstoneOre));
        assert!(!can_harvest(pick, Block::Obsidian));
    }
    assert!(can_harvest(stone, Block::IronOre));
    assert!(can_harvest(stone, Block::IronBlock));
    assert!(can_harvest(stone, Block::LapisOre));
    assert!(can_harvest(stone, Block::LapisBlock));
    assert!(!can_harvest(stone, Block::GoldOre));
    assert!(!can_harvest(stone, Block::GoldBlock));
    assert!(!can_harvest(stone, Block::DiamondOre));
    assert!(!can_harvest(stone, Block::DiamondBlock));
    assert!(!can_harvest(stone, Block::RedstoneOre));
    assert!(!can_harvest(stone, Block::Obsidian));

    assert!(can_harvest(iron, Block::GoldOre));
    assert!(can_harvest(iron, Block::GoldBlock));
    assert!(can_harvest(iron, Block::DiamondOre));
    assert!(can_harvest(iron, Block::DiamondBlock));
    assert!(can_harvest(iron, Block::RedstoneOre));
    assert!(can_harvest(iron, Block::LitRedstoneOre));
    assert!(!can_harvest(iron, Block::Obsidian));
    assert!(can_harvest(diamond, Block::Obsidian));

    assert!(!can_harvest(None, Block::Stone));
    assert!(can_harvest(None, Block::Dirt));
    assert!(can_harvest(held(Item::WoodenShovel), Block::Snow));
    assert!(!can_harvest(wood, Block::Snow));
    assert!(!can_harvest(None, Block::Snow));
    assert!(!can_harvest(held(Item::DiamondSword), Block::Stone));
    assert!(can_harvest(None, Block::Leaves));
    assert!(can_harvest(held(Item::Shears), Block::Leaves));

    assert!(!can_harvest(None, Block::Cobweb));
    assert!(can_harvest(held(Item::WoodenSword), Block::Cobweb));
    assert!(can_harvest(held(Item::Shears), Block::Cobweb));
    assert!(!can_harvest(wood, Block::Cobweb));
    assert!(!can_harvest(None, Block::SnowLayer));
    assert!(can_harvest(held(Item::WoodenShovel), Block::SnowLayer));
    assert!(!can_harvest(None, Block::IronDoor));
    assert!(can_harvest(wood, Block::IronDoor));
    assert!(can_harvest(wood, Block::StoneSlab));
    assert!(can_harvest(wood, Block::CobblestoneStairs));
    assert!(can_harvest(wood, Block::StonePressurePlate));
    assert!(!can_harvest(None, Block::StoneSlab));
}

#[test]
fn block_breaks_spend_beta_durability() {
    let pick = ItemStack::new(Item::WoodenPickaxe, 1).unwrap();
    assert_eq!(break_durability(pick, Block::Stone), 1);
    assert_eq!(break_durability(pick, Block::Dirt), 1);
    assert_eq!(pick.apply_damage(1).unwrap().data(), 1);

    let sword = ItemStack::new(Item::WoodenSword, 1).unwrap();
    assert_eq!(break_durability(sword, Block::Dirt), 2);
    assert_eq!(break_durability(sword, Block::Stone), 2);

    let shears = ItemStack::new(Item::Shears, 1).unwrap();
    assert_eq!(break_durability(shears, Block::Leaves), 1);
    assert_eq!(break_durability(shears, Block::Wool), 0);
    assert_eq!(break_durability(shears, Block::Cobweb), 1);
    assert_eq!(break_durability(shears, Block::Stone), 0);

    let hoe = ItemStack::new(Item::WoodenHoe, 1).unwrap();
    assert_eq!(break_durability(hoe, Block::Dirt), 0);
    let dirt = ItemStack::from_block(Block::Dirt, 1).unwrap();
    assert_eq!(break_durability(dirt, Block::Dirt), 0);
    assert_eq!(dirt.apply_damage(1), Some(dirt));

    let mut pick = ItemStack::new(Item::WoodenPickaxe, 1).unwrap();
    for use_index in 1..=59 {
        pick = pick.apply_damage(1).expect("wooden pick survives 59 uses");
        assert_eq!(pick.data(), use_index);
    }
    assert!(pick.apply_damage(1).is_none());

    let sword = ItemStack::with_data(Item::WoodenSword, 1, 57).unwrap();
    assert_eq!(sword.apply_damage(2).unwrap().data(), 59);
    let sword = ItemStack::with_data(Item::WoodenSword, 1, 58).unwrap();
    assert!(sword.apply_damage(2).is_none());

    let mut hotbar = Hotbar::default();
    hotbar.slots[0] = Some(ItemStack::with_data(Item::WoodenPickaxe, 1, 58).unwrap());
    hotbar.damage_selected(1);
    assert_eq!(hotbar.selected_stack().unwrap().data(), 59);
    hotbar.damage_selected(1);
    assert!(hotbar.selected_stack().is_none());
}

#[test]
fn picks_dig_single_slabs_and_shovels_dig_snow_layers_at_tool_speed() {
    assert_eq!(
        str_vs_block(held(Item::WoodenPickaxe), Block::StoneSlab),
        2.0
    );
    assert_eq!(str_vs_block(held(Item::IronShovel), Block::SnowLayer), 6.0);
    assert_eq!(str_vs_block(held(Item::IronShovel), Block::Snow), 6.0);
}

#[test]
fn armor_pieces_name_the_slot_they_are_worn_in() {
    assert_eq!(Item::IronHelmet.armor_slot(), Some(0));
    assert_eq!(Item::GoldChestplate.armor_slot(), Some(1));
    assert_eq!(Item::LeatherLeggings.armor_slot(), Some(2));
    assert_eq!(Item::DiamondBoots.armor_slot(), Some(3));
    assert_eq!(
        Item::from_block(Block::Pumpkin).unwrap().armor_slot(),
        Some(0)
    );
    assert_eq!(Item::from_block(Block::Dirt).unwrap().armor_slot(), None);
    assert_eq!(Item::Stick.armor_slot(), None);
}

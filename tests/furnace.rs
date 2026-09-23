use bevy::prelude::*;
use std::fs;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use game::item::ItemId;
use game::item::ItemStack;
use game::world::block::block::BlockId;
use game::world::chunk::ChunkPos;
use game::world::chunk::WorldChunks;
use game::world::furnace::Furnace;
use game::world::furnace::SMELT_TICKS;
use game::world::furnace::fuel_ticks;
use game::world::furnace::smelting_result;
use game::world::furnace::tick_furnaces;
use game::world::generation::WorldGenerator;
use game::world::persistence::WorldStorage;
use game::world::persistence::chunk_file_name;
use game::world::persistence::region_dir_name;
use game::world::persistence::region_of;
use game::world::tick::WorldTick;

fn stack(item: ItemId) -> ItemStack {
    ItemStack::new(item, 1).unwrap()
}

fn block(block: BlockId) -> ItemStack {
    ItemStack::from_block(block, 1).unwrap()
}

fn temp_saves() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("game-furnace-{unique}"));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn beta_smelting_recipes_cover_the_reference_list() {
    let recipes = [
        (block(BlockId::IronOre), stack(ItemId::IronIngot)),
        (block(BlockId::GoldOre), stack(ItemId::GoldIngot)),
        (block(BlockId::DiamondOre), stack(ItemId::Diamond)),
        (block(BlockId::Sand), block(BlockId::Glass)),
        (stack(ItemId::RawPorkchop), stack(ItemId::CookedPorkchop)),
        (stack(ItemId::RawFish), stack(ItemId::CookedFish)),
        (block(BlockId::Cobblestone), block(BlockId::Stone)),
        (stack(ItemId::ClayBall), stack(ItemId::Brick)),
        (
            block(BlockId::Cactus),
            ItemStack::with_data(ItemId::Dye, 1, 2).unwrap(),
        ),
        (
            ItemStack::with_data(ItemId::BlockWood, 1, 2).unwrap(),
            ItemStack::with_data(ItemId::Coal, 1, 1).unwrap(),
        ),
    ];

    for (input, expected) in recipes {
        assert_eq!(smelting_result(input), Some(expected));
    }
    assert_eq!(smelting_result(block(BlockId::CoalOre)), None);
}

#[test]
fn beta_fuel_durations_cover_coal_charcoal_and_wood_materials() {
    assert_eq!(fuel_ticks(block(BlockId::Wood)), Some(300));
    assert_eq!(fuel_ticks(block(BlockId::WoodenPlanks)), Some(300));
    assert_eq!(fuel_ticks(block(BlockId::Fence)), Some(300));
    assert_eq!(fuel_ticks(block(BlockId::Sapling)), Some(100));
    assert_eq!(fuel_ticks(stack(ItemId::Stick)), Some(100));
    assert_eq!(fuel_ticks(stack(ItemId::Coal)), Some(1_600));
    assert_eq!(
        fuel_ticks(ItemStack::with_data(ItemId::Coal, 1, 1).unwrap()),
        Some(1_600)
    );
    assert_eq!(fuel_ticks(stack(ItemId::LavaBucket)), Some(20_000));
    assert_eq!(fuel_ticks(block(BlockId::Leaves)), None);
}

#[test]
fn furnace_cooks_one_item_in_two_hundred_world_ticks() {
    let mut furnace = Furnace::default();
    furnace.slots[0] = Some(block(BlockId::IronOre));
    furnace.slots[1] = Some(stack(ItemId::Coal));

    for tick in 0..SMELT_TICKS {
        let lit_changed = furnace.tick();
        assert_eq!(lit_changed, tick == 0);
    }

    assert_eq!(furnace.slots[0], None);
    assert_eq!(furnace.slots[1], None);
    assert_eq!(furnace.slots[2], Some(stack(ItemId::IronIngot)));
    assert_eq!(furnace.burn_ticks, 1_401);
}

#[test]
fn smelted_items_merge_into_output_until_the_stack_limit() {
    let mut furnace = Furnace::default();
    furnace.slots[0] = Some(block(BlockId::IronOre));
    furnace.slots[1] = Some(stack(ItemId::Coal));
    furnace.slots[2] = Some(ItemStack::new(ItemId::IronIngot, 63).unwrap());
    for _ in 0..SMELT_TICKS {
        furnace.tick();
    }

    assert_eq!(
        furnace.slots[2],
        Some(ItemStack::new(ItemId::IronIngot, 64).unwrap())
    );
    assert!(furnace.slots[0].is_none());
}

#[test]
fn furnace_world_system_uses_world_ticks_and_switches_the_block_light_state() {
    let mut app = App::new();
    app.init_resource::<WorldChunks>()
        .init_resource::<WorldTick>()
        .add_systems(Update, tick_furnaces);
    let position = ChunkPos::ZERO;
    let mut generated = WorldGenerator::new(1).generate(position);
    generated.chunk.set(2, 40, 3, BlockId::Furnace);
    app.world_mut()
        .resource_mut::<WorldChunks>()
        .insert(position, generated);
    {
        let mut chunks = app.world_mut().resource_mut::<WorldChunks>();
        let furnace = chunks.furnace_at_mut(2, 40, 3).unwrap();
        furnace.slots[0] = Some(block(BlockId::IronOre));
        furnace.slots[1] = Some(stack(ItemId::Stick));
    }

    app.world_mut().resource_mut::<WorldTick>().advance(0.05);
    app.update();
    assert_eq!(
        app.world().resource::<WorldChunks>().block_at(2, 40, 3),
        Some(BlockId::LitFurnace)
    );

    for _ in 1..101 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        app.update();
    }
    assert_eq!(
        app.world().resource::<WorldChunks>().block_at(2, 40, 3),
        Some(BlockId::Furnace)
    );
}

#[test]
fn removing_a_furnace_block_removes_its_block_local_inventory() {
    let mut generated = WorldGenerator::new(2).generate(ChunkPos::ZERO);
    generated.chunk.set(2, 40, 3, BlockId::Furnace);
    let mut chunks = WorldChunks::default();
    chunks.insert(ChunkPos::ZERO, generated);
    chunks.furnace_at_mut(2, 40, 3).unwrap().slots[0] = Some(block(BlockId::IronOre));

    chunks.set_block(2, 40, 3, BlockId::Air);

    assert!(chunks.furnace_at(2, 40, 3).is_none());
}

#[test]
fn furnace_waits_when_output_is_full_and_keeps_progress_without_fuel() {
    let mut furnace = Furnace::default();
    furnace.slots[0] = Some(block(BlockId::IronOre));
    furnace.slots[1] = Some(stack(ItemId::Stick));
    furnace.slots[2] = Some(ItemStack::new(ItemId::IronIngot, 64).unwrap());
    for _ in 0..10 {
        furnace.tick();
    }
    assert_eq!(furnace.cook_ticks, 0);
    assert_eq!(furnace.slots[0], Some(block(BlockId::IronOre)));
    assert_eq!(furnace.slots[1], Some(stack(ItemId::Stick)));

    furnace.slots[2] = None;
    furnace.slots[1] = Some(stack(ItemId::Stick));
    for _ in 0..50 {
        furnace.tick();
    }
    assert_eq!(furnace.cook_ticks, 50);
    furnace.slots[1] = None;
    furnace.burn_ticks = 0;
    furnace.tick();
    assert_eq!(furnace.cook_ticks, 50);
}

#[test]
fn furnace_inventory_and_progress_round_trip_and_old_chunks_still_load() {
    let saves = temp_saves();
    let storage = WorldStorage::create(&saves, 99, "Furnace persistence").unwrap();
    let position = ChunkPos { x: -1, z: 2 };
    let mut generated = WorldGenerator::new(99).generate(position);
    generated.chunk.set(3, 32, 7, BlockId::Furnace);
    let index = (32 * 16 + 7) * 16 + 3;
    let furnace = generated.chunk.furnace_mut(index).unwrap();
    furnace.slots[0] = Some(block(BlockId::IronOre));
    furnace.slots[1] = Some(stack(ItemId::Coal));
    furnace.burn_ticks = 321;
    furnace.fuel_ticks = 1_600;
    furnace.cook_ticks = 87;
    let expected = furnace.clone();
    storage.save_chunk(position, &generated).unwrap();

    let loaded = storage.load_chunk(position).unwrap();
    let (_, restored) = loaded.chunk.furnaces().next().unwrap();
    assert_eq!(restored, &expected);

    let path = storage
        .root()
        .join(region_dir_name(region_of(position)))
        .join(chunk_file_name(position));
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("furnaces");
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let old = storage.load_chunk(position).unwrap();
    let (_, legacy_furnace) = old.chunk.furnaces().next().unwrap();
    assert_eq!(legacy_furnace, &Furnace::default());

    fs::remove_dir_all(saves).unwrap();
}

use game::block::blocks::Block;
use game::entity::drops::blocks::DropRoll;
use game::entity::drops::blocks::natural_drops_with_metadata;
use game::item::ItemId;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::plant_seeds;
use game::world::block_ticks::behaviors::crops::Crops;
use game::world::block_ticks::behaviors::crops::RIPE;

use super::TestWorld;
use super::at;

const NOON: u64 = 6000;

/// A 3×3 of chunks with a dirt floor at y = 63 across chunk (0, 0).
fn field() -> TestWorld {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 60, 0), at(15, 63, 15), Block::Dirt);
    world.time = NOON;
    world
}

struct Rolls(u32);

impl DropRoll for Rolls {
    fn next_int(&mut self, _bound: u32) -> u32 {
        self.0
    }
}

#[test]
fn seeds_plant_crops_only_on_top_of_farmland() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Farmland);
    let hit = |face| BlockHit {
        x: 8,
        y: 63,
        z: 8,
        face,
        block: Block::Farmland,
    };
    assert!(!plant_seeds(&mut world.chunks, hit(BlockFace::North)));
    assert!(plant_seeds(&mut world.chunks, hit(BlockFace::Up)));
    assert_eq!(world.block(at(8, 64, 8)), Block::Crops);
    assert!(
        !plant_seeds(&mut world.chunks, hit(BlockFace::Up)),
        "the cell is taken"
    );
    let dirt = BlockHit {
        x: 9,
        y: 63,
        z: 8,
        face: BlockFace::Up,
        block: Block::Dirt,
    };
    assert!(!plant_seeds(&mut world.chunks, dirt));
}

#[test]
fn lit_crops_grow_to_ripe_wheat() {
    let mut world = field();
    world.set_with_metadata(at(8, 63, 8), Block::Farmland, 7);
    world.set(at(8, 64, 8), Block::Crops);
    world.relight();
    world.random_ticks(at(8, 64, 8), 2000);
    assert_eq!(world.block(at(8, 64, 8)), Block::Crops);
    assert_eq!(world.metadata(at(8, 64, 8)), RIPE);
}

#[test]
fn crops_do_not_grow_at_night_but_stay_planted() {
    let mut world = field();
    world.set_with_metadata(at(8, 63, 8), Block::Farmland, 7);
    world.set(at(8, 64, 8), Block::Crops);
    world.relight();
    // Midnight dims sky light below the growth threshold of 9, while the
    // full light a plant needs to stay does not depend on the hour.
    world.time = 18_000;
    world.random_ticks(at(8, 64, 8), 500);
    assert_eq!(world.block(at(8, 64, 8)), Block::Crops);
    assert_eq!(world.metadata(at(8, 64, 8)), 0);
}

#[test]
fn crops_in_the_dark_pop_off() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Farmland);
    world.fill(at(7, 64, 7), at(9, 65, 9), Block::Stone);
    world.set(at(8, 64, 8), Block::Crops);
    world.relight();
    world.random_ticks(at(8, 64, 8), 1);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
}

#[test]
fn crop_growth_rate_rewards_wet_farmland_and_punishes_crowding() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Farmland);
    world.set(at(8, 64, 8), Block::Crops);
    {
        let world = world.ticks.world(&mut world.chunks, &mut world.light, 0);
        assert_eq!(Crops::growth_rate(&world, at(8, 64, 8)), 2.0);
    }
    world.set_with_metadata(at(8, 63, 8), Block::Farmland, 7);
    for (x, z) in [(7, 8), (9, 8), (8, 7), (8, 9)] {
        world.set(at(x, 63, z), Block::Farmland);
    }
    {
        let world = world.ticks.world(&mut world.chunks, &mut world.light, 0);
        assert_eq!(Crops::growth_rate(&world, at(8, 64, 8)), 5.0);
    }
    // Crops along both axes halve the rate.
    world.set(at(9, 64, 8), Block::Crops);
    world.set(at(8, 64, 9), Block::Crops);
    let world = world.ticks.world(&mut world.chunks, &mut world.light, 0);
    assert_eq!(Crops::growth_rate(&world, at(8, 64, 8)), 2.5);
}

#[test]
fn crops_pop_off_when_their_farmland_turns_to_dirt() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Farmland);
    world.set_with_metadata(at(8, 64, 8), Block::Crops, 5);
    world.relight();
    world.place(at(8, 63, 8), Block::Dirt);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(world.drops(), vec![(at(8, 64, 8), Block::Crops, 5)]);
}

#[test]
fn crop_drops_follow_their_growth_stage() {
    let item = |stacks: Vec<game::item::ItemStack>, item| {
        stacks.iter().filter(|stack| stack.item() == item).count()
    };
    let ripe = natural_drops_with_metadata(Block::Crops, RIPE, &mut Rolls(7));
    assert_eq!(item(ripe.clone(), ItemId::Wheat), 1);
    assert_eq!(item(ripe, ItemId::Seeds), 3);
    let young = natural_drops_with_metadata(Block::Crops, 2, &mut Rolls(3));
    assert_eq!(item(young.clone(), ItemId::Wheat), 0);
    assert_eq!(
        item(young, ItemId::Seeds),
        0,
        "a roll above the stage drops nothing"
    );
}

#[test]
fn flowers_need_their_ground_and_light() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Grass);
    world.set(at(8, 64, 8), Block::Rose);
    world.set(at(9, 63, 8), Block::Sand);
    world.set(at(9, 64, 8), Block::DeadBush);
    world.relight();
    world.random_ticks(at(8, 64, 8), 10);
    world.random_ticks(at(9, 64, 8), 10);
    assert_eq!(world.block(at(8, 64, 8)), Block::Rose);
    assert_eq!(world.block(at(9, 64, 8)), Block::DeadBush);

    // Replacing the ground pops each plant off as an item.
    world.place(at(8, 63, 8), Block::Stone);
    world.place(at(9, 63, 8), Block::Dirt);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(
        world.block(at(9, 64, 8)),
        Block::Air,
        "dead bushes want sand"
    );
    let dropped: Vec<_> = world
        .drops()
        .into_iter()
        .map(|(_, block, _)| block)
        .collect();
    assert_eq!(dropped, vec![Block::Rose, Block::DeadBush]);
}

#[test]
fn a_covered_flower_in_the_dark_dies_on_a_random_tick() {
    let mut world = field();
    world.set(at(8, 64, 8), Block::Dandelion);
    world.fill(at(7, 64, 7), at(9, 65, 9), Block::Stone);
    world.set(at(8, 64, 8), Block::Dandelion);
    world.relight();
    world.random_ticks(at(8, 64, 8), 1);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
}

#[test]
fn mushrooms_spread_in_the_shade_and_need_solid_ground() {
    let mut world = field();
    // A closed stone room with a mushroom inside.
    world.fill(at(2, 64, 2), at(12, 67, 12), Block::Stone);
    world.fill(at(3, 64, 3), at(11, 66, 11), Block::Air);
    world.fill(at(3, 63, 3), at(11, 63, 11), Block::Stone);
    world.set(at(7, 64, 7), Block::BrownMushroom);
    world.relight();
    world.random_ticks(at(7, 64, 7), 3000);
    let mushrooms = (3..=11)
        .flat_map(|x| (3..=11).map(move |z| (x, z)))
        .filter(|&(x, z)| world.block(at(x, 64, z)) == Block::BrownMushroom)
        .count();
    assert!(mushrooms > 1, "a mushroom spreads now and then");

    // In full daylight a mushroom cannot stay.
    world.set(at(1, 63, 1), Block::Stone);
    world.place(at(1, 64, 1), Block::RedMushroom);
    world.place(at(1, 63, 1), Block::Cobblestone);
    assert_eq!(world.block(at(1, 64, 1)), Block::Air);
}

#[test]
fn cactus_grows_three_tall_and_breaks_beside_a_solid_block() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Sand);
    world.set(at(8, 64, 8), Block::Cactus);
    // Sixteen random ticks grow one block.
    world.random_ticks(at(8, 64, 8), 16);
    assert_eq!(world.block(at(8, 65, 8)), Block::Cactus);
    assert_eq!(world.metadata(at(8, 64, 8)), 0);
    world.random_ticks(at(8, 65, 8), 16);
    assert_eq!(world.block(at(8, 66, 8)), Block::Cactus);
    world.random_ticks(at(8, 66, 8), 40);
    assert_eq!(
        world.block(at(8, 67, 8)),
        Block::Air,
        "no taller than three"
    );

    world.place(at(9, 66, 8), Block::Dirt);
    assert_eq!(
        world.block(at(8, 66, 8)),
        Block::Air,
        "a solid neighbor breaks the top"
    );
    assert_eq!(world.drops(), vec![(at(8, 66, 8), Block::Cactus, 0)]);
}

#[test]
fn sugar_cane_grows_and_breaks_when_its_water_goes() {
    let mut world = field();
    world.set(at(8, 63, 8), Block::Grass);
    world.set(at(9, 63, 8), Block::Water);
    world.set(at(8, 64, 8), Block::SugarCane);
    world.random_ticks(at(8, 64, 8), 16);
    assert_eq!(world.block(at(8, 65, 8)), Block::SugarCane);

    // The water is diagonal to the cane, so the cane only notices once its
    // own ground changes.
    world.place(at(9, 63, 8), Block::Dirt);
    assert_eq!(world.block(at(8, 64, 8)), Block::SugarCane);
    world.place(at(8, 63, 8), Block::Dirt);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(
        world.block(at(8, 65, 8)),
        Block::Air,
        "the stack comes down with its base"
    );
}

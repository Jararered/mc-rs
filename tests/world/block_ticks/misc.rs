use game::block::blocks::Block;
use game::world::biome::Biome;
use game::world::block_ticks::BlockEvent;
use game::world::chunk::ChunkPosition;

use super::TestWorld;
use super::at;

#[test]
fn touching_redstone_ore_lights_it_until_a_random_tick() {
    let mut world = TestWorld::new(1);
    let ore = at(8, 40, 8);
    for event in [
        BlockEvent::Clicked { position: ore },
        BlockEvent::Activated { position: ore },
        BlockEvent::Walked { position: ore },
    ] {
        world.set(ore, Block::RedstoneOre);
        world.event(event);
        assert_eq!(world.block(ore), Block::LitRedstoneOre, "{event:?}");
        world.random_ticks(ore, 1);
        assert_eq!(world.block(ore), Block::RedstoneOre);
    }
}

#[test]
fn ice_melts_beside_a_torch_but_not_in_sunlight() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 63, 8), Block::Stone);
    world.set(at(8, 64, 8), Block::Ice);
    world.set(at(4, 63, 8), Block::Stone);
    world.set(at(4, 64, 8), Block::Ice);
    world.set(at(9, 63, 8), Block::Stone);
    world.set(at(9, 64, 8), Block::Torch);
    world.relight();
    world.random_ticks(at(4, 64, 8), 5);
    assert_eq!(
        world.block(at(4, 64, 8)),
        Block::Ice,
        "sunlight never melts ice"
    );
    world.random_ticks(at(8, 64, 8), 1);
    assert_eq!(
        world.block(at(8, 64, 8)),
        Block::Water,
        "block light above 8 melts it"
    );
}

#[test]
fn harvested_ice_leaves_water_over_solid_ground() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 63, 8), Block::Stone);
    world.event(BlockEvent::Harvested {
        position: at(8, 64, 8),
        block: Block::Ice,
        metadata: 0,
    });
    assert_eq!(world.block(at(8, 64, 8)), Block::FlowingWater);

    world.event(BlockEvent::Harvested {
        position: at(8, 90, 8),
        block: Block::Ice,
        metadata: 0,
    });
    assert_eq!(world.block(at(8, 90, 8)), Block::Air, "not over air");
}

#[test]
fn snow_layers_melt_in_torch_light_and_need_solid_ground() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 63, 0), at(15, 63, 15), Block::Stone);
    world.set(at(8, 64, 8), Block::SnowLayer);
    world.set(at(9, 64, 8), Block::Torch);
    // A snow block is an opaque cube, so its own block light stays 0 and a
    // torch beside it never melts it, as in Beta.
    world.set(at(4, 64, 8), Block::Snow);
    world.set(at(5, 64, 8), Block::Torch);
    world.set(at(14, 64, 8), Block::SnowLayer);
    world.relight();
    world.random_ticks(at(8, 64, 8), 1);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(world.drops(), vec![(at(8, 64, 8), Block::SnowLayer, 0)]);
    world.random_ticks(at(4, 64, 8), 5);
    assert_eq!(world.block(at(4, 64, 8)), Block::Snow);
    world.random_ticks(at(14, 64, 8), 5);
    assert_eq!(
        world.block(at(14, 64, 8)),
        Block::SnowLayer,
        "sunlight never melts snow"
    );

    world.place(at(14, 63, 8), Block::Ice);
    assert_eq!(
        world.block(at(14, 64, 8)),
        Block::Air,
        "ice is solid but not an opaque cube"
    );
}

#[test]
fn torches_and_ladders_break_off_when_their_wall_goes() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 64, 8), Block::Stone);
    world.set(at(9, 64, 8), Block::TorchWest);
    world.set(at(8, 64, 9), Block::LadderNorth);
    world.set(at(8, 65, 8), Block::Torch);
    world.set(at(6, 65, 8), Block::Stone);
    world.set(at(7, 65, 8), Block::TorchWest);
    world.set(at(8, 66, 8), Block::Stone);

    world.place(at(8, 64, 8), Block::Air);
    assert_eq!(world.block(at(9, 64, 8)), Block::Air);
    assert_eq!(world.block(at(8, 64, 9)), Block::Air);
    assert_eq!(
        world.block(at(8, 65, 8)),
        Block::Air,
        "a floor torch needs the block below"
    );
    let mut dropped: Vec<_> = world
        .drops()
        .into_iter()
        .map(|(_, block, _)| block)
        .collect();
    dropped.sort_by_key(|block| block.as_u8());
    assert_eq!(
        dropped,
        vec![Block::Torch, Block::TorchWest, Block::LadderNorth]
    );
    assert_eq!(
        world.block(at(7, 65, 8)),
        Block::TorchWest,
        "other torches keep their walls"
    );
}

#[test]
fn still_water_in_a_snowy_biome_freezes_under_open_sky() {
    let mut world = TestWorld::with_biome(1, Biome::Tundra);
    world.fill(at(0, 62, 0), at(15, 62, 15), Block::Stone);
    world.fill(at(0, 63, 0), at(15, 63, 15), Block::Water);
    world.relight();
    world.run_with_random(400, &[ChunkPosition::ZERO]);
    let ice = (0..16)
        .flat_map(|x| (0..16).map(move |z| (x, z)))
        .filter(|&(x, z)| world.block(at(x, 63, z)) == Block::Ice)
        .count();
    assert!(ice > 0, "one column in sixteen chunk ticks tries to freeze");

    let mut warm = TestWorld::with_biome(1, Biome::Plains);
    warm.fill(at(0, 63, 0), at(15, 63, 15), Block::Water);
    warm.run_with_random(400, &[ChunkPosition::ZERO]);
    assert!(
        (0..16).all(|x| (0..16).all(|z| warm.block(at(x, 63, z)) == Block::Water)),
        "only snowy biomes freeze"
    );
}

#[test]
fn removing_a_sponge_wakes_the_water_around_it() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 64, 8), Block::Sponge);
    world.set(at(10, 64, 8), Block::Water);
    world.place(at(8, 64, 8), Block::Stone);
    assert_eq!(
        world.block(at(10, 64, 8)),
        Block::FlowingWater,
        "two blocks away is inside the sponge's reach"
    );
}

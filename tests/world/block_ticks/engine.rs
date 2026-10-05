use bevy::math::IVec3;
use game::block::blocks::Block;
use game::world::block_ticks::BlockChange;
use game::world::block_ticks::BlockTicks;
use game::world::block_ticks::MAX_SCHEDULED_PER_TICK;
use game::world::block_ticks::RANDOM_TICKS_PER_CHUNK;
use game::world::block_ticks::TickEffect;
use game::world::block_ticks::behavior;
use game::world::block_ticks::ticks_randomly;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::PendingTick;
use game::world::lighting::LightCache;

use super::TestWorld;
use super::at;

#[test]
fn chunk_metadata_is_a_nibble_that_resets_when_the_block_changes() {
    let mut chunk = Chunk::new();
    assert!(
        chunk.raw_metadata().is_none(),
        "no storage until a value is set"
    );
    chunk.set_with_metadata(3, 70, 5, Block::FlowingWater, 0x1b);
    assert_eq!(chunk.metadata(3, 70, 5), 0x0b, "only four bits are kept");
    assert_eq!(
        chunk.metadata(2, 70, 5),
        0,
        "neighbors sharing the byte keep zero"
    );
    chunk.set_metadata(2, 70, 5, 7);
    assert_eq!(chunk.metadata(3, 70, 5), 0x0b);
    assert_eq!(chunk.metadata(2, 70, 5), 7);

    // `setBlockID` zeroes the metadata of a different block, but setting
    // the same block keeps it.
    chunk.set(3, 70, 5, Block::FlowingWater);
    assert_eq!(chunk.metadata(3, 70, 5), 0x0b);
    chunk.set(3, 70, 5, Block::Stone);
    assert_eq!(chunk.metadata(3, 70, 5), 0);

    // Clones share storage until one is written.
    let copy = chunk.clone();
    chunk.set_metadata(2, 70, 5, 1);
    assert_eq!(copy.metadata(2, 70, 5), 7);
}

#[test]
fn a_scheduled_tick_runs_once_its_delay_has_passed() {
    let mut world = TestWorld::new(2);
    let sand = at(8, 70, 8);
    world.place(sand, Block::Sand);
    assert!(
        world.ticks.is_scheduled(sand, Block::Sand),
        "sand schedules itself when added"
    );

    world.run(2);
    assert!(
        world.effects().is_empty(),
        "the tick is not due before three ticks"
    );
    world.run(1);
    assert_eq!(
        world.effects(),
        vec![TickEffect::FallingBlock {
            position: sand,
            block: Block::Sand,
        }]
    );
    assert!(!world.ticks.is_scheduled(sand, Block::Sand));
}

#[test]
fn a_pending_tick_is_never_scheduled_twice_and_keeps_its_first_due_time() {
    let mut ticks = BlockTicks::new(0);
    let cell = at(1, 64, 1);
    ticks.schedule(cell, Block::Sand, 10);
    ticks.schedule(cell, Block::Sand, 5);
    ticks.schedule(cell, Block::Gravel, 7);
    let scheduled: Vec<_> = ticks.scheduled().collect();
    assert_eq!(
        scheduled.len(),
        2,
        "a different block at the cell is its own entry"
    );
    assert_eq!(scheduled[0].block, Block::Gravel);
    assert_eq!((scheduled[1].block, scheduled[1].due), (Block::Sand, 10));
}

#[test]
fn due_ticks_run_in_due_order_then_scheduling_order() {
    let mut world = TestWorld::new(2);
    // Three sand blocks over air. Their ticks come due together, and each
    // spawns its falling entity in the order the ticks were scheduled.
    let cells = [at(2, 70, 2), at(4, 70, 4), at(6, 70, 6)];
    for (index, cell) in cells.into_iter().enumerate() {
        world.set(cell, Block::Sand);
        world
            .ticks
            .schedule(cell, Block::Sand, if index == 0 { 2 } else { 1 });
    }
    world.run(2);
    let order: Vec<_> = world
        .effects()
        .into_iter()
        .map(|effect| match effect {
            TickEffect::FallingBlock { position, .. } => position,
            TickEffect::PrimedTnt { position, .. } => position,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(order, vec![cells[1], cells[2], cells[0]]);
}

#[test]
fn a_tick_for_a_block_that_was_replaced_does_nothing() {
    let mut world = TestWorld::new(2);
    let cell = at(8, 70, 8);
    world.place(cell, Block::Sand);
    world.set(cell, Block::Stone);
    world.run(5);
    assert!(world.effects().is_empty());
    assert_eq!(world.block(cell), Block::Stone);
}

#[test]
fn a_tick_waits_while_its_surroundings_are_not_loaded() {
    // Only chunk (0, 0) exists, so a cell near its edge lacks the eight
    // blocks around it that Beta requires.
    let mut world = TestWorld::new(0);
    let cell = at(1, 70, 8);
    world.set(cell, Block::Sand);
    world.ticks.schedule(cell, Block::Sand, 1);
    world.run(5);
    assert!(world.effects().is_empty());
    assert!(
        world.ticks.is_scheduled(cell, Block::Sand),
        "the tick is kept for when the area loads"
    );
}

#[test]
fn at_most_a_thousand_scheduled_ticks_run_per_world_tick() {
    let mut world = TestWorld::new(1);
    // Supported sand: every tick runs and nothing falls.
    world.fill(at(0, 0, 0), at(15, 0, 15), Block::Stone);
    let mut scheduled = 0;
    'fill: for x in 0..16 {
        for z in 0..16 {
            for y in 1..10 {
                world.set(at(x, y, z), Block::Sand);
                world.ticks.schedule(at(x, y, z), Block::Sand, 1);
                scheduled += 1;
                if scheduled == MAX_SCHEDULED_PER_TICK + 200 {
                    break 'fill;
                }
            }
        }
    }
    world.run(1);
    assert_eq!(world.ticks.scheduled_count(), 200);
}

#[test]
fn random_ticks_reach_eighty_cells_per_chunk_and_only_ticking_blocks() {
    let mut world = TestWorld::new(1);
    // Fill the whole chunk with lit redstone ore, which every random tick
    // puts out.
    world.fill(at(0, 0, 0), at(15, 127, 15), Block::LitRedstoneOre);
    world.run_with_random(1, &[ChunkPosition::ZERO]);
    let dimmed = world
        .changes()
        .iter()
        .filter(|change| change.block == Block::RedstoneOre)
        .count();
    assert!(
        (60..=RANDOM_TICKS_PER_CHUNK).contains(&dimmed),
        "80 samples, with a few repeats: {dimmed}"
    );

    assert!(ticks_randomly(Block::LitRedstoneOre.as_u8()));
    assert!(!ticks_randomly(Block::RedstoneOre.as_u8()));
    assert!(!ticks_randomly(Block::Stone.as_u8()));
    assert!(ticks_randomly(Block::Grass.as_u8()));
    assert!(behavior(Block::Grass).ticks_randomly(Block::Grass));
}

#[test]
fn unloading_a_chunk_saves_its_pending_ticks_with_their_remaining_delay() {
    let mut world = TestWorld::new(1);
    world.run(10);
    let cell = at(-3, 64, 5);
    world.set(cell, Block::Sand);
    world.ticks.schedule(cell, Block::Sand, world.time + 7);
    let other = at(20, 64, 5);
    world.ticks.schedule(other, Block::Sand, world.time + 2);

    let position = ChunkPosition::from_block(cell.x, cell.z);
    let mut generated = world.chunks.remove(position).unwrap();
    world.ticks.unload_chunk(position, &mut generated.chunk);
    assert_eq!(
        generated.chunk.pending_ticks(),
        &[PendingTick {
            index: Chunk::index(13, 64, 5) as u16,
            block: Block::Sand,
            delay: 7,
        }]
    );
    assert!(!world.ticks.is_scheduled(cell, Block::Sand));
    assert!(
        world.ticks.is_scheduled(other, Block::Sand),
        "other chunks keep theirs"
    );

    // Reloading much later still waits the remaining seven ticks.
    world.run(100);
    world.ticks.load_chunk(position, &mut generated.chunk);
    assert!(generated.chunk.pending_ticks().is_empty());
    let restored = world
        .ticks
        .scheduled()
        .find(|tick| tick.position == cell)
        .unwrap();
    assert_eq!(restored.due, world.time + 7);
}

#[test]
fn changing_a_block_notifies_all_six_neighbors() {
    let mut world = TestWorld::new(1);
    // Still water on every side of the changed cell turns flowing.
    let center = at(8, 64, 8);
    for offset in game::world::block_ticks::NEIGHBORS {
        world.set(center + offset, Block::Water);
    }
    world.place(center, Block::Stone);
    for offset in game::world::block_ticks::NEIGHBORS {
        assert_eq!(
            world.block(center + offset),
            Block::FlowingWater,
            "{offset}"
        );
        assert!(
            world
                .ticks
                .is_scheduled(center + offset, Block::FlowingWater)
        );
    }
}

#[test]
fn block_changes_know_when_they_change_light_or_meshes() {
    let change = |previous, previous_metadata, block, metadata| BlockChange {
        position: IVec3::ZERO,
        previous,
        previous_metadata,
        block,
        metadata,
    };
    assert!(change(Block::Stone, 0, Block::Air, 0).changes_light());
    assert!(change(Block::RedstoneOre, 0, Block::LitRedstoneOre, 0).changes_light());
    assert!(!change(Block::Water, 0, Block::FlowingWater, 0).changes_light());
    assert!(!change(Block::Crops, 1, Block::Crops, 2).changes_light());
    assert!(!change(Block::Grass, 0, Block::Dirt, 0).changes_light());

    // Only changes the mesher can see are remeshed.
    assert!(!change(Block::FlowingLava, 0, Block::Lava, 0).needs_remesh());
    assert!(change(Block::FlowingWater, 1, Block::FlowingWater, 2).needs_remesh());
    assert!(change(Block::Crops, 1, Block::Crops, 2).needs_remesh());
    assert!(!change(Block::Leaves, 0, Block::Leaves, 8).needs_remesh());
    assert!(!change(Block::Cactus, 3, Block::Cactus, 4).needs_remesh());
    assert!(!change(Block::Farmland, 7, Block::Farmland, 6).needs_remesh());
    assert!(change(Block::Farmland, 1, Block::Farmland, 0).needs_remesh());
    assert!(change(Block::Grass, 0, Block::Dirt, 0).needs_remesh());
    assert!(change(Block::RedstoneOre, 0, Block::LitRedstoneOre, 0).needs_remesh());
}

#[test]
fn the_light_cache_answers_from_a_relit_chunk() {
    let mut world = TestWorld::new(1);
    world.set(at(8, 64, 8), Block::Torch);
    let mut light = LightCache::default();
    assert_eq!(light.channels(8, 64, 9), None);
    light.relight(&world.chunks, ChunkPosition::ZERO);
    assert_eq!(light.channels(8, 64, 9), Some((15, 13)));
    assert_eq!(light.channels(8, -1, 8), Some((0, 0)));
    assert_eq!(light.channels(8, 200, 8), Some((15, 0)));
}

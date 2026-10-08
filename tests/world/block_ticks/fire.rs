use super::TestWorld;
use super::at;
use bevy::math::IVec3;
use game::block::blocks::Block;
use game::world::block_ticks::TickEffect;

fn count(world: &TestWorld, from: IVec3, to: IVec3, block: Block) -> usize {
    let mut total = 0;
    for x in from.x..=to.x {
        for y in from.y..=to.y {
            for z in from.z..=to.z {
                total += usize::from(world.block(at(x, y, z)) == block);
            }
        }
    }
    total
}

#[test]
fn fire_climbs_a_tree_and_burns_its_leaves_away() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 19, 0), at(15, 19, 15), Block::Stone);
    world.fill(at(8, 20, 8), at(8, 25, 8), Block::Wood);
    world.fill(at(6, 23, 6), at(10, 26, 10), Block::Leaves);
    world.fill(at(8, 23, 8), at(8, 25, 8), Block::Wood);
    let (low, high) = (at(4, 20, 4), at(12, 30, 12));
    let leaves = count(&world, low, high, Block::Leaves);

    // Flint and steel against the trunk, tried again if a flame dies young.
    let mut reached_canopy = false;
    for _ in 0..40 {
        if count(&world, low, high, Block::Fire) == 0 {
            world.place(at(9, 20, 8), Block::Fire);
        }
        world.run(200);
        reached_canopy |= count(&world, at(4, 23, 4), high, Block::Fire) > 0;
        if count(&world, low, high, Block::Leaves) == 0 {
            break;
        }
    }
    assert!(reached_canopy, "fire never reached the leaves");
    let left = count(&world, low, high, Block::Leaves);
    assert!(left * 4 < leaves, "{left} of {leaves} leaves survived");
    assert!(
        world.drops().is_empty(),
        "burnt blocks leave nothing behind"
    );
}

#[test]
fn fire_without_fuel_burns_out_except_on_netherrack() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 19, 0), at(15, 19, 15), Block::Stone);
    world.set(at(12, 19, 12), Block::Netherrack);
    world.place(at(4, 20, 4), Block::Fire);
    world.place(at(12, 20, 12), Block::Fire);
    world.run(2000);
    assert_eq!(world.block(at(4, 20, 4)), Block::Air);
    assert_eq!(world.block(at(12, 20, 12)), Block::Fire);
}

#[test]
fn fire_needs_something_to_stand_on_or_burn() {
    let mut world = TestWorld::new(1);
    world.place(at(8, 30, 8), Block::Fire);
    assert_eq!(world.block(at(8, 30, 8)), Block::Air);

    world.set(at(9, 30, 8), Block::WoodenPlanks);
    world.place(at(8, 30, 8), Block::Fire);
    assert_eq!(world.block(at(8, 30, 8)), Block::Fire);
    // Taking the planks away takes the fire with them.
    world.place(at(9, 30, 8), Block::Air);
    assert_eq!(world.block(at(8, 30, 8)), Block::Air);
}

#[test]
fn rain_puts_out_open_fire() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 19, 0), at(15, 19, 15), Block::Stone);
    world.set(at(8, 19, 8), Block::WoodenPlanks);
    world.place(at(8, 20, 8), Block::Fire);
    world.ticks.set_raining(true);
    world.run(41);
    assert_eq!(world.block(at(8, 20, 8)), Block::Air);
}

#[test]
fn still_lava_sets_fire_to_wood_above_it() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 19, 0), at(15, 20, 15), Block::Stone);
    world.set(at(8, 20, 8), Block::Lava);
    world.fill(at(6, 22, 6), at(10, 22, 10), Block::WoodenPlanks);
    for _ in 0..200 {
        world.random_ticks(at(8, 20, 8), 1);
        if count(&world, at(6, 21, 6), at(10, 21, 10), Block::Fire) > 0 {
            return;
        }
    }
    panic!("lava never lit the planks over it");
}

#[test]
fn burning_tnt_ignites_a_fuse_without_dropping_an_item() {
    let mut world = TestWorld::new(2);
    world.set(at(0, 19, 0), Block::Stone);
    world.set(at(1, 19, 0), Block::Stone);
    world.place(at(1, 20, 0), Block::Tnt);
    world.place(at(0, 20, 0), Block::Fire);
    for _ in 0..120 {
        world.random_ticks(at(0, 20, 0), 1);
        if world.block(at(1, 20, 0)) != Block::Tnt {
            break;
        }
    }
    // `tryToCatchBlockOnFire` leaves fire or air where the TNT was.
    assert_ne!(world.block(at(1, 20, 0)), Block::Tnt);
    assert!(world.effects().iter().any(|effect| matches!(effect,
        TickEffect::PrimedTnt { position, fuse: 80 } if *position == at(1,20,0))));
}

use super::TestWorld;
use super::at;
use game::block::blocks::Block;
use game::world::block_ticks::TickEffect;

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

use super::TestWorld;
use super::at;
use game::block::id::Id;
use game::world::block_ticks::TickEffect;

#[test]
fn burning_tnt_ignites_a_fuse_without_dropping_an_item() {
    let mut world = TestWorld::new(2);
    world.set(at(0, 19, 0), Id::Stone);
    world.set(at(1, 19, 0), Id::Stone);
    world.place(at(1, 20, 0), Id::Tnt);
    world.place(at(0, 20, 0), Id::Fire);
    for _ in 0..120 {
        world.random_ticks(at(0, 20, 0), 1);
        if world.block(at(1, 20, 0)) == Id::Air {
            break;
        }
    }
    assert_eq!(world.block(at(1, 20, 0)), Id::Air);
    assert!(world.effects().iter().any(|effect| matches!(effect,
        TickEffect::PrimedTnt { position, fuse: 80 } if *position == at(1,20,0))));
}

use bevy::prelude::Vec3;
use game::block::blocks::Block;
use game::entity::drops::blocks::natural_drops_with_metadata;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::player::place_selected_block;
use game::world::block_ticks::BlockEvent;

use super::TestWorld;
use super::at;

fn far_player() -> Aabb {
    Aabb::new(Vec3::new(0.0, 80.0, 0.0), Vec3::new(0.6, 81.8, 0.6))
}

fn click_top(x: i32, y: i32, z: i32, block: Block) -> BlockHit {
    BlockHit {
        x,
        y,
        z,
        face: BlockFace::Up,
        block,
    }
}

#[test]
fn cake_bounds_shrink_from_the_west_and_collision_is_shorter() {
    assert_eq!(
        Block::Cake.selection_bounds_for(0),
        ([0.0625, 0.0, 0.0625], [0.9375, 0.5, 0.9375])
    );
    assert_eq!(
        Block::Cake.collision_bounds_for(0),
        Some(([0.0625, 0.0, 0.0625], [0.9375, 0.4375, 0.9375]))
    );
    assert_eq!(Block::Cake.selection_bounds_for(1).0[0], 3.0 / 16.0);
    assert_eq!(Block::Cake.selection_bounds_for(5).0[0], 11.0 / 16.0);
    // A value past the last slice still draws as five bites.
    assert_eq!(
        Block::Cake.selection_bounds_for(15),
        Block::Cake.selection_bounds_for(5)
    );
    assert_eq!(Block::Cake.appearance_metadata(3), 3);
    assert_eq!(Block::Cake.appearance_metadata(9), 5);
}

#[test]
fn cake_places_only_on_a_solid_material_and_not_inside_the_player() {
    let mut world = TestWorld::new(0);
    world.set(at(8, 64, 8), Block::Stone);
    let player = far_player();
    assert!(place_selected_block(
        &mut world.chunks,
        click_top(8, 64, 8, Block::Stone),
        player,
        Block::Cake,
    ));
    assert_eq!(world.block(at(8, 65, 8)), Block::Cake);

    world.set(at(4, 64, 4), Block::Glass);
    assert!(place_selected_block(
        &mut world.chunks,
        click_top(4, 64, 4, Block::Glass),
        player,
        Block::Cake,
    ));
    world.set(at(5, 64, 4), Block::StoneSlab);
    assert!(place_selected_block(
        &mut world.chunks,
        click_top(5, 64, 4, Block::StoneSlab),
        player,
        Block::Cake,
    ));
    assert!(place_selected_block(
        &mut world.chunks,
        click_top(8, 65, 8, Block::Cake),
        player,
        Block::Cake,
    ));
    assert_eq!(world.block(at(8, 66, 8)), Block::Cake);

    let side = BlockHit {
        x: 8,
        y: 64,
        z: 8,
        face: BlockFace::West,
        block: Block::Stone,
    };
    assert!(
        !place_selected_block(&mut world.chunks, side, player, Block::Cake),
        "air under the cell"
    );
    assert_eq!(world.block(at(7, 64, 8)), Block::Air);

    world.set(at(2, 64, 2), Block::Torch);
    assert!(!place_selected_block(
        &mut world.chunks,
        click_top(2, 64, 2, Block::Torch),
        player,
        Block::Cake,
    ));
    world.set(at(3, 64, 2), Block::SnowLayer);
    assert!(!place_selected_block(
        &mut world.chunks,
        click_top(3, 64, 2, Block::SnowLayer),
        player,
        Block::Cake,
    ));

    let inside = Aabb::new(Vec3::new(6.2, 65.0, 6.2), Vec3::new(6.8, 66.8, 6.8));
    world.set(at(6, 64, 6), Block::Stone);
    assert!(!place_selected_block(
        &mut world.chunks,
        click_top(6, 64, 6, Block::Stone),
        inside,
        Block::Cake,
    ));
    assert_eq!(world.block(at(6, 65, 6)), Block::Air);
}

#[test]
fn cake_pops_with_no_item_when_its_support_goes() {
    let mut world = TestWorld::new(0);
    let below = at(8, 64, 8);
    let cake = at(8, 65, 8);
    world.set(below, Block::Glass);
    world.set(cake, Block::Cake);
    world.set_with_metadata(cake, Block::Cake, 2);
    // A neighbor changing does not remove a cake that still has support.
    world.set(at(7, 65, 8), Block::Stone);
    world.place(at(7, 65, 8), Block::Air);
    assert_eq!(world.block(cake), Block::Cake);
    assert_eq!(world.metadata(cake), 2);
    assert!(world.drops().is_empty());

    world.place(below, Block::Air);
    assert_eq!(world.block(cake), Block::Air);
    assert_eq!(world.drops(), vec![(cake, Block::Cake, 2)]);
    let mut rolls = Rolls;
    assert!(natural_drops_with_metadata(Block::Cake, 2, &mut rolls).is_empty());

    // Another cake is a solid material, so the upper one stays.
    world.set(below, Block::Cake);
    world.set(cake, Block::Cake);
    world.event(BlockEvent::Changed {
        position: at(9, 65, 8),
        previous: Block::Stone,
        metadata: 0,
    });
    assert_eq!(world.block(cake), Block::Cake);
}

/// `nextInt` is unused: cake never rolls a drop.
struct Rolls;

impl game::entity::drops::blocks::DropRoll for Rolls {
    fn next_int(&mut self, _bound: u32) -> u32 {
        0
    }
}

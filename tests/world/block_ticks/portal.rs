use bevy::math::IVec3;
use game::block::blocks::Block;
use game::world::dimension::Dimension;

use super::TestWorld;
use super::at;

/// An obsidian frame four wide and five tall whose 2×3 opening starts at
/// `origin` and runs along `step`.
fn frame(world: &mut TestWorld, origin: IVec3, step: IVec3, corners: bool) {
    for width in -1..=2 {
        for height in -1..=3 {
            let edge = width == -1 || width == 2 || height == -1 || height == 3;
            let corner = (width == -1 || width == 2) && (height == -1 || height == 3);
            if edge && (corners || !corner) {
                world.set(origin + step * width + IVec3::Y * height, Block::Obsidian);
            }
        }
    }
}

fn portal_cells(origin: IVec3, step: IVec3) -> Vec<IVec3> {
    (0..2)
        .flat_map(|width| (0..3).map(move |height| origin + step * width + IVec3::Y * height))
        .collect()
}

#[test]
fn fire_in_a_whole_frame_fills_it_with_portal_blocks_on_either_axis() {
    for step in [IVec3::X, IVec3::Z] {
        let mut world = TestWorld::new(1);
        let origin = at(4, 20, 4);
        frame(&mut world, origin, step, false);
        // Lit in the far column: the portal still starts from the near one.
        world.place(origin + step, Block::Fire);
        for cell in portal_cells(origin, step) {
            assert_eq!(
                world.block(cell),
                Block::NetherPortal,
                "{cell} along {step}"
            );
        }
        // A neighbor changing does not disturb a whole portal.
        world.place(origin + step * 3, Block::Stone);
        for cell in portal_cells(origin, step) {
            assert_eq!(world.block(cell), Block::NetherPortal);
        }
    }
}

#[test]
fn a_frame_with_a_gap_or_the_wrong_size_only_burns() {
    let origin = at(4, 20, 4);

    let mut gap = TestWorld::new(1);
    frame(&mut gap, origin, IVec3::X, true);
    gap.set(at(6, 21, 4), Block::Air);
    gap.place(origin, Block::Fire);
    assert_eq!(gap.block(origin), Block::Fire);
    assert_eq!(gap.block(origin + IVec3::Y), Block::Air);

    // Three wide inside instead of two.
    let mut wide = TestWorld::new(1);
    frame(&mut wide, origin, IVec3::X, true);
    for height in -1..=3 {
        wide.set(at(6, 20 + height, 4), Block::Air);
        wide.set(at(7, 20 + height, 4), Block::Obsidian);
    }
    wide.set(at(6, 19, 4), Block::Obsidian);
    wide.set(at(6, 23, 4), Block::Obsidian);
    wide.place(origin, Block::Fire);
    assert_eq!(wide.block(origin), Block::Fire);

    // Something standing in the opening.
    let mut blocked = TestWorld::new(1);
    frame(&mut blocked, origin, IVec3::X, true);
    blocked.set(at(5, 22, 4), Block::Stone);
    blocked.place(origin, Block::Fire);
    assert_eq!(blocked.block(origin), Block::Fire);
}

#[test]
fn fire_on_bare_obsidian_behaves_like_any_other_fire() {
    let mut world = TestWorld::new(1);
    world.set(at(4, 19, 4), Block::Obsidian);
    world.place(at(4, 20, 4), Block::Fire);
    assert_eq!(world.block(at(4, 20, 4)), Block::Fire);
    assert!(world.ticks.is_scheduled(at(4, 20, 4), Block::Fire));
}

#[test]
fn breaking_the_frame_takes_the_portal_with_it() {
    let origin = at(4, 20, 4);
    // The floor, the cap, and a side each hold the portal up.
    for broken in [at(4, 19, 4), at(5, 23, 4), at(3, 21, 4), at(6, 20, 4)] {
        let mut world = TestWorld::new(1);
        frame(&mut world, origin, IVec3::X, true);
        world.place(origin, Block::Fire);
        assert_eq!(world.block(origin), Block::NetherPortal);

        world.place(broken, Block::Air);
        for cell in portal_cells(origin, IVec3::X) {
            assert_eq!(world.block(cell), Block::Air, "{cell} outlived {broken}");
        }
    }
}

#[test]
fn nether_lava_flows_as_far_as_water() {
    let reach = |dimension: Dimension| {
        let mut world = TestWorld::new(2);
        world.ticks.set_dimension(dimension);
        world.fill(at(-20, 19, 0), at(20, 19, 0), Block::Stone);
        // A trough one block wide, so the lava can only run along x.
        world.fill(at(-20, 20, -1), at(20, 21, -1), Block::Stone);
        world.fill(at(-20, 20, 1), at(20, 21, 1), Block::Stone);
        world.place(at(0, 20, 0), Block::FlowingLava);
        world.run(2000);
        (1..20)
            .take_while(|x| matches!(world.block(at(*x, 20, 0)), Block::Lava | Block::FlowingLava))
            .count()
    };
    assert_eq!(reach(Dimension::Overworld), 3);
    assert_eq!(reach(Dimension::Nether), 7);
}

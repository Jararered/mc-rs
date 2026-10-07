//! Doors, trapdoors, slabs, stairs, fences, glass, and saplings: placement,
//! tick behavior, collision, tiles, and meshes.

use bevy::math::Vec3;
use game::block::blocks::Block;
use game::block::blocks::species;
use game::block::direction::Direction;
use game::physics::Aabb;
use game::physics::BlockFace;
use game::physics::BlockHit;
use game::physics::colliding_aabbs;
use game::player::place_door;
use game::player::place_selected_block_facing;
use game::rendering::meshing::mesh_chunk_with_settings;
use game::rendering::textures::block_tile;
use game::rendering::textures::door_tile;
use game::world::block_ticks::BlockEvent;
use game::world::block_ticks::behaviors::plants::SAPLING_READY;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;

use super::TestWorld;
use super::at;

const NOON: u64 = 6000;
const FAR_PLAYER: Aabb = Aabb {
    min: Vec3::new(2.0, 90.0, 2.0),
    max: Vec3::new(2.6, 91.8, 2.6),
};

/// A 3×3 of chunks with a stone floor at y = 63 across chunk (0, 0).
fn floor() -> TestWorld {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 63, 0), at(15, 63, 15), Block::Stone);
    world.time = NOON;
    world
}

fn hit(x: i32, y: i32, z: i32, face: BlockFace, block: Block) -> BlockHit {
    BlockHit {
        x,
        y,
        z,
        face,
        block,
    }
}

/// Place a door the way the item does, then run the placement events.
fn door(world: &mut TestWorld, front: Direction) -> bool {
    let placed = place_door(
        &mut world.chunks,
        hit(8, 63, 8, BlockFace::Up, Block::Stone),
        Block::WoodenDoor,
        front,
    );
    if placed {
        for y in [64, 65] {
            world.ticks.block_changed(at(8, y, 8), Block::Air, 0);
        }
        world.process_events();
    }
    placed
}

#[test]
fn a_door_item_stands_two_halves_on_a_full_cube() {
    let mut world = floor();
    assert!(door(&mut world, Direction::West));
    assert_eq!(world.block(at(8, 64, 8)), Block::WoodenDoor);
    assert_eq!(world.block(at(8, 65, 8)), Block::WoodenDoor);
    assert_eq!(world.metadata(at(8, 64, 8)), 0);
    assert_eq!(world.metadata(at(8, 65, 8)), 8);
    assert!(
        world.drops().is_empty(),
        "placing must not pop the door off"
    );
}

#[test]
fn a_door_needs_a_top_face_a_floor_and_headroom() {
    let mut world = floor();
    let side = hit(8, 63, 8, BlockFace::North, Block::Stone);
    assert!(!place_door(
        &mut world.chunks,
        side,
        Block::WoodenDoor,
        Direction::West
    ));
    world.set(at(8, 65, 8), Block::Stone);
    assert!(!door(&mut world, Direction::West), "no headroom");
    world.set(at(8, 65, 8), Block::Air);
    world.set(at(8, 63, 8), Block::Glass);
    assert!(!place_door(
        &mut world.chunks,
        hit(8, 63, 8, BlockFace::Up, Block::Glass),
        Block::WoodenDoor,
        Direction::West
    ));
}

#[test]
fn the_hinge_moves_to_the_side_with_more_wall() {
    let mut world = floor();
    // `front` West is Beta facing 0, whose "ahead" side is +z.
    world.fill(at(8, 64, 9), at(8, 65, 9), Block::Stone);
    assert!(door(&mut world, Direction::West));
    assert_eq!(world.metadata(at(8, 64, 8)), 7);
    assert_eq!(world.metadata(at(8, 65, 8)), 15);
}

#[test]
fn activating_either_half_swings_both() {
    let mut world = floor();
    assert!(door(&mut world, Direction::West));
    world.event(BlockEvent::Activated {
        position: at(8, 65, 8),
    });
    assert_eq!(world.metadata(at(8, 64, 8)), 4);
    assert_eq!(world.metadata(at(8, 65, 8)), 12);
    world.event(BlockEvent::Clicked {
        position: at(8, 64, 8),
    });
    assert_eq!(world.metadata(at(8, 64, 8)), 0);
    assert_eq!(world.metadata(at(8, 65, 8)), 8);
}

#[test]
fn an_iron_door_does_not_open_by_hand() {
    let mut world = floor();
    world.set_with_metadata(at(8, 64, 8), Block::IronDoor, 0);
    world.set_with_metadata(at(8, 65, 8), Block::IronDoor, 8);
    world.event(BlockEvent::Activated {
        position: at(8, 64, 8),
    });
    assert_eq!(world.metadata(at(8, 64, 8)), 0);
}

#[test]
fn a_door_drops_one_item_when_its_floor_goes() {
    let mut world = floor();
    assert!(door(&mut world, Direction::West));
    world.drops();
    world.place(at(8, 63, 8), Block::Air);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(world.block(at(8, 65, 8)), Block::Air);
    assert_eq!(world.drops(), vec![(at(8, 64, 8), Block::WoodenDoor, 0)]);
}

#[test]
fn breaking_one_half_takes_the_other() {
    let mut world = floor();
    assert!(door(&mut world, Direction::West));
    world.drops();
    world.place(at(8, 65, 8), Block::Air);
    assert_eq!(world.block(at(8, 64, 8)), Block::Air);
    assert_eq!(world.drops(), vec![(at(8, 64, 8), Block::WoodenDoor, 0)]);
}

#[test]
fn a_door_collides_as_a_thin_panel_that_turns_when_open() {
    let mut world = floor();
    assert!(door(&mut world, Direction::West));
    let cell = Aabb::new(Vec3::new(8.0, 64.0, 8.0), Vec3::new(9.0, 65.0, 9.0));
    let closed = colliding_aabbs(&world.chunks, cell);
    world.event(BlockEvent::Activated {
        position: at(8, 64, 8),
    });
    let open = colliding_aabbs(&world.chunks, cell);
    assert_ne!(closed, open);
    for boxes in [closed, open] {
        let panel = boxes
            .iter()
            .find(|aabb| aabb.min.y == 64.0)
            .expect("the lower half collides");
        let size = panel.max - panel.min;
        assert_eq!(size.x.min(size.z), 0.1875);
        assert_eq!(size.x.max(size.z), 1.0);
    }
}

#[test]
fn door_tiles_follow_beta_and_mirror_one_side() {
    // Metadata 0 is state 3: the panel lies along x = 0, faces west and east.
    assert_eq!(door_tile(Block::WoodenDoor, 0, 3), ((1, 6), false));
    assert_eq!(door_tile(Block::WoodenDoor, 0, 2), ((1, 6), true));
    assert_eq!(door_tile(Block::WoodenDoor, 8, 3), ((1, 5), false));
    assert_eq!(door_tile(Block::IronDoor, 0, 3), ((2, 6), false));
    // The thin edges and the top use the lower tile unmirrored.
    assert_eq!(door_tile(Block::WoodenDoor, 8, 4), ((1, 6), false));
    assert_eq!(door_tile(Block::WoodenDoor, 8, 0), ((1, 6), false));
}

#[test]
fn a_trapdoor_hangs_on_a_wall_opens_and_falls_with_it() {
    let mut world = floor();
    world.set(at(8, 64, 8), Block::Stone);
    let wall = |face| hit(8, 64, 8, face, Block::Stone);
    assert!(!place_selected_block_facing(
        &mut world.chunks,
        wall(BlockFace::Up),
        FAR_PLAYER,
        Block::Trapdoor,
        0,
        Direction::South,
    ));
    assert!(place_selected_block_facing(
        &mut world.chunks,
        wall(BlockFace::North),
        FAR_PLAYER,
        Block::Trapdoor,
        0,
        Direction::South,
    ));
    let trapdoor = at(8, 64, 7);
    assert_eq!(world.block(trapdoor), Block::Trapdoor);
    assert_eq!(world.metadata(trapdoor), 0);
    assert_eq!(
        Block::Trapdoor.collision_bounds_for(0),
        Some(([0.0; 3], [1.0, 0.1875, 1.0]))
    );
    world.event(BlockEvent::Activated { position: trapdoor });
    assert_eq!(world.metadata(trapdoor), 4);
    assert_eq!(
        Block::Trapdoor.collision_bounds_for(4),
        Some(([0.0, 0.0, 0.8125], [1.0, 1.0, 1.0]))
    );
    world.place(at(8, 64, 8), Block::Air);
    assert_eq!(world.block(trapdoor), Block::Air);
    assert_eq!(world.drops(), vec![(trapdoor, Block::Trapdoor, 4)]);
}

#[test]
fn a_slab_on_a_matching_slab_becomes_a_double_slab() {
    let mut world = floor();
    world.set_with_metadata(at(8, 64, 8), Block::StoneSlab, 2);
    world.set_with_metadata(at(8, 65, 8), Block::StoneSlab, 2);
    world.ticks.block_changed(at(8, 65, 8), Block::Air, 0);
    world.process_events();
    assert_eq!(world.block(at(8, 65, 8)), Block::Air);
    assert_eq!(world.block(at(8, 64, 8)), Block::DoubleStoneSlab);
    assert_eq!(world.metadata(at(8, 64, 8)), 2);

    // A different material stays two slabs.
    world.set_with_metadata(at(4, 64, 4), Block::StoneSlab, 0);
    world.set_with_metadata(at(4, 65, 4), Block::StoneSlab, 3);
    world.ticks.block_changed(at(4, 65, 4), Block::Air, 0);
    world.process_events();
    assert_eq!(world.block(at(4, 65, 4)), Block::StoneSlab);
}

#[test]
fn stairs_face_away_from_the_player_and_collide_as_two_boxes() {
    let mut world = floor();
    for (front, metadata) in [
        (Direction::North, 2),
        (Direction::East, 1),
        (Direction::South, 3),
        (Direction::West, 0),
    ] {
        world.set(at(8, 64, 8), Block::Air);
        assert!(place_selected_block_facing(
            &mut world.chunks,
            hit(8, 63, 8, BlockFace::Up, Block::Stone),
            FAR_PLAYER,
            Block::WoodenStairs,
            0,
            front,
        ));
        assert_eq!(world.metadata(at(8, 64, 8)), metadata);
    }
    // Metadata 0: a half step on the west, the full riser on the east.
    let low = Aabb::new(Vec3::new(8.1, 64.6, 8.1), Vec3::new(8.4, 64.9, 8.9));
    let high = Aabb::new(Vec3::new(8.6, 64.6, 8.1), Vec3::new(8.9, 64.9, 8.9));
    assert!(colliding_aabbs(&world.chunks, low).is_empty());
    assert_eq!(colliding_aabbs(&world.chunks, high).len(), 1);
}

#[test]
fn a_fence_needs_ground_and_stands_a_block_and_a_half_tall() {
    let mut world = floor();
    let place = |world: &mut TestWorld, target: BlockHit| {
        place_selected_block_facing(
            &mut world.chunks,
            target,
            FAR_PLAYER,
            Block::Fence,
            0,
            Direction::South,
        )
    };
    assert!(place(
        &mut world,
        hit(8, 63, 8, BlockFace::Up, Block::Stone)
    ));
    assert!(place(
        &mut world,
        hit(8, 64, 8, BlockFace::Up, Block::Fence)
    ));
    // Hanging off the side of the post, over air.
    assert!(!place(
        &mut world,
        hit(8, 65, 8, BlockFace::East, Block::Fence)
    ));
    let above = Aabb::new(Vec3::new(8.1, 66.1, 8.1), Vec3::new(8.9, 66.4, 8.9));
    assert_eq!(colliding_aabbs(&world.chunks, above).len(), 1);
}

#[test]
fn only_drawable_blocks_are_placeable_and_none_of_them_looks_like_stone() {
    for raw in 1..=96u8 {
        let block = Block::from_u8(raw).unwrap();
        let Some((placed, metadata)) = block.placed(0) else {
            continue;
        };
        if matches!(placed, Block::Stone | Block::Piston) {
            continue;
        }
        for face in 0..6 {
            assert_ne!(
                block_tile(placed, metadata, face, true),
                (1, 0),
                "{placed:?} face {face} falls back to the stone tile"
            );
        }
    }
    assert_eq!(block_tile(Block::Sapling, species::OAK, 0, true), (15, 0));
    assert_eq!(
        block_tile(Block::Sapling, species::SPRUCE | SAPLING_READY, 0, true),
        (15, 3)
    );
    assert_eq!(block_tile(Block::Sapling, species::BIRCH, 0, true), (15, 4));
    assert_eq!(block_tile(Block::Glass, 0, 0, true), (1, 3));
    assert_eq!(block_tile(Block::Wool, 0, 0, true), (0, 4));
    // Orange wool is Beta tile 210, black 113.
    assert_eq!(block_tile(Block::Wool, 1, 0, true), (2, 13));
    assert_eq!(block_tile(Block::Wool, 15, 0, true), (1, 7));
    assert_eq!(block_tile(Block::StoneSlab, 1, 2, true), (0, 12));
    assert_eq!(Block::Sapling.placed(2), Some((Block::Sapling, 2)));
    assert_eq!(Block::Wool.placed(14), Some((Block::Wool, 14)));
}

#[test]
fn glass_is_alpha_masked_and_shares_no_face_with_glass() {
    let mesh = |chunk: &Chunk| mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), true);
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Glass);
    let one = mesh(&chunk);
    assert_eq!(one.opaque.vertex_count(), 0);
    assert_eq!(one.masked.vertex_count(), 24);
    chunk.set(9, 64, 8, Block::Glass);
    // The pair merges into the six faces of one 2×1×1 box: no inner faces.
    assert_eq!(mesh(&chunk).masked.vertex_count(), 24);
    // Stone beside glass keeps the face seen through it.
    chunk.set(9, 64, 8, Block::Stone);
    assert_eq!(mesh(&chunk).opaque.vertex_count(), 24);
}

#[test]
fn shaped_blocks_mesh_as_boxes() {
    let mesh = |chunk: &Chunk| mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), true);
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::StoneSlab);
    chunk.set(8, 65, 8, Block::Stone);
    let meshes = mesh(&chunk);
    // The slab keeps its top at y + 0.5 under the stone, and the stone
    // keeps its bottom above the gap: 6 + 6 faces.
    assert_eq!(meshes.opaque.vertex_count(), 48);
    assert!(
        meshes
            .opaque
            .positions()
            .iter()
            .any(|position| position[1] == 64.5)
    );

    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::WoodenStairs);
    assert_eq!(mesh(&chunk).opaque.vertex_count(), 48, "two boxes");

    // A lone fence is a post and two stub rails; a pair joins rails.
    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Fence);
    assert_eq!(mesh(&chunk).opaque.vertex_count(), 3 * 24);
    chunk.set(8, 64, 9, Block::Fence);
    let joined = mesh(&chunk).opaque.positions();
    assert!(joined.iter().any(|position| position[2] == 9.0));

    let mut chunk = Chunk::new();
    chunk.set_with_metadata(8, 64, 8, Block::WoodenDoor, 0);
    chunk.set_with_metadata(8, 65, 8, Block::WoodenDoor, 8);
    let door = mesh(&chunk);
    assert_eq!(door.masked.vertex_count(), 48);
    assert_eq!(door.opaque.vertex_count(), 0);

    let mut chunk = Chunk::new();
    chunk.set(8, 64, 8, Block::Cobweb);
    assert_eq!(mesh(&chunk).masked.vertex_count(), 16);
}

#[test]
fn a_lit_sapling_grows_a_tree_of_its_species() {
    for kind in [species::OAK, species::SPRUCE, species::BIRCH] {
        let mut world = TestWorld::new(1);
        world.fill(at(0, 60, 0), at(15, 63, 15), Block::Dirt);
        world.time = NOON;
        let sapling = at(8, 64, 8);
        world.set_with_metadata(sapling, Block::Sapling, kind);
        world.relight();
        for _ in 0..4000 {
            world.random_ticks(sapling, 1);
            if world.block(sapling) != Block::Sapling {
                break;
            }
        }
        assert_eq!(world.block(sapling), Block::Wood, "species {kind}");
        assert_eq!(world.metadata(sapling) & 3, kind);
        assert!(world.drops().is_empty());
    }
}

#[test]
fn a_sapling_waits_a_stage_and_pops_off_bad_ground() {
    let mut world = TestWorld::new(1);
    world.fill(at(0, 60, 0), at(15, 63, 15), Block::Dirt);
    world.time = NOON;
    let sapling = at(8, 64, 8);
    world.set_with_metadata(sapling, Block::Sapling, species::BIRCH);
    world.relight();
    for _ in 0..4000 {
        world.random_ticks(sapling, 1);
        if world.metadata(sapling) != species::BIRCH {
            break;
        }
    }
    assert_eq!(world.block(sapling), Block::Sapling);
    assert_eq!(world.metadata(sapling), species::BIRCH | SAPLING_READY);

    world.place(at(8, 63, 8), Block::Air);
    assert_eq!(world.block(sapling), Block::Air);
    let drops = world.drops();
    assert_eq!(drops.len(), 1);
    assert_eq!(
        Block::Sapling.item_form(drops[0].2),
        (Block::Sapling, species::BIRCH)
    );
}

/// The texel of the vertex of a face (by normal) nearest `corner`.
fn texel_at(
    geometry: &game::rendering::meshing::BlockGeometry,
    normal: [f32; 3],
    corner: [f32; 3],
) -> ([u8; 2], [u8; 2]) {
    let vertex = geometry
        .vertices()
        .iter()
        .find(|vertex| vertex.normal == normal && vertex.position == corner)
        .expect("the face has a vertex at that corner");
    (vertex.texel.tile, vertex.texel.texel)
}

#[test]
fn a_double_chest_front_reads_left_to_right_from_outside() {
    let mesh = |chunk: &Chunk| mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), true);
    // A pair along x. Front tiles are 41 and 42: (9, 2) left, (10, 2) right.
    for (facing, normal, z, left_x) in [
        // Seen from the south, -x is on the left.
        (Direction::South, [0.0, 0.0, 1.0], 9.0 - 1.0 / 16.0, 8.0625),
        // Seen from the north, +x is on the left.
        (Direction::North, [0.0, 0.0, -1.0], 8.0 + 1.0 / 16.0, 9.9375),
    ] {
        let mut chunk = Chunk::new();
        let metadata = Block::Chest.facing_metadata(facing);
        chunk.set_with_metadata(8, 64, 8, Block::Chest, metadata);
        chunk.set_with_metadata(9, 64, 8, Block::Chest, metadata);
        let meshes = mesh(&chunk);
        // The outer bottom corner of the left half is the image's left edge.
        let (tile, texel) = texel_at(&meshes.opaque, normal, [left_x, 64.0, z]);
        assert_eq!(tile, [9, 2], "facing {facing:?}");
        assert_eq!(texel[0], 0, "facing {facing:?}");
        let right_x = 18.0 - left_x;
        let (tile, texel) = texel_at(&meshes.opaque, normal, [right_x, 64.0, z]);
        assert_eq!(tile, [10, 2], "facing {facing:?}");
        assert_eq!(texel[0], 16, "facing {facing:?}");
    }
}

#[test]
fn a_door_face_reads_the_same_way_round_from_both_sides() {
    let mesh = |chunk: &Chunk| mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), true);
    // Metadata 0: the panel lies along x = 0..3/16. Beta mirrors the east
    // side, so both sides put the same edge of the tile at the same z.
    let mut chunk = Chunk::new();
    chunk.set_with_metadata(8, 64, 8, Block::WoodenDoor, 0);
    chunk.set_with_metadata(8, 65, 8, Block::WoodenDoor, 8);
    let masked = mesh(&chunk).masked;
    let west = texel_at(&masked, [-1.0, 0.0, 0.0], [8.0, 64.0, 8.0]).1[0];
    let east = texel_at(&masked, [1.0, 0.0, 0.0], [8.1875, 64.0, 8.0]).1[0];
    assert_eq!(west, 0);
    assert_eq!(east, 0);
}

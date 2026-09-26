use game::block::id::Id;
use game::world::chunk::Chunk;
use game::world::generation::Heightmap;

#[test]
fn heightmap_records_top_occupying_block_by_x_then_z() {
    let mut chunk = Chunk::new();
    chunk.set(2, 7, 3, Id::Stone);
    chunk.set(2, 12, 3, Id::Water);
    chunk.set(4, 5, 9, Id::Dirt);

    let heights = Heightmap::from_chunk(&chunk);
    assert_eq!(heights.get(2, 3), 8);
    assert_eq!(heights.get(4, 9), 6);
    assert_eq!(heights.get(3, 2), 0);
    assert_eq!(heights.max(), 8);
}

#[test]
fn non_solid_air_plants_torches_and_ladders_do_not_raise_the_heightmap() {
    let mut chunk = Chunk::new();
    chunk.set(1, 20, 1, Id::TallGrass);
    chunk.set(2, 30, 2, Id::Torch);
    chunk.set(3, 40, 3, Id::LadderNorth);
    chunk.set(4, 50, 4, Id::Water);
    chunk.set(5, 60, 5, Id::Stone);
    chunk.set(5, 90, 5, Id::Torch);

    let heights = Heightmap::from_chunk(&chunk);
    assert_eq!(heights.get(1, 1), 0);
    assert_eq!(heights.get(2, 2), 0);
    assert_eq!(heights.get(3, 3), 0);
    assert_eq!(heights.get(4, 4), 0);
    assert_eq!(heights.get(5, 5), 61);
}

#[test]
fn recomputing_after_removing_the_top_block_reveals_the_next_surface() {
    let mut chunk = Chunk::new();
    chunk.set(8, 3, 6, Id::Dirt);
    chunk.set(8, 9, 6, Id::Stone);
    let mut heights = Heightmap::from_chunk(&chunk);
    assert_eq!(heights.get(8, 6), 10);

    chunk.set(8, 9, 6, Id::Air);
    heights.recompute_column(&chunk, 8, 6);
    assert_eq!(heights.get(8, 6), 4);

    chunk.set(8, 3, 6, Id::Air);
    heights.recompute_column(&chunk, 8, 6);
    assert_eq!(heights.get(8, 6), 0);
}

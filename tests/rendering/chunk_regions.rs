use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use game::rendering::chunk_regions::DrawList;
use game::rendering::chunk_regions::REGION_CHUNKS;
use game::rendering::chunk_regions::SectionLayer;
use game::rendering::chunk_regions::region_of;
use game::rendering::chunk_regions::section_offset;
use game::rendering::chunk_regions::visible_facings;
use game::rendering::meshing::QUAD_WORDS;
use game::rendering::meshing::UNASSIGNED_FACING;
use game::world::chunk::ChunkPosition;

// `geometry::FACE_*` order.
const UP: u8 = 1 << 0;
const DOWN: u8 = 1 << 1;
const EAST: u8 = 1 << 2;
const WEST: u8 = 1 << 3;
const SOUTH: u8 = 1 << 4;
const NORTH: u8 = 1 << 5;
const UNASSIGNED: u8 = 1 << UNASSIGNED_FACING;

fn layer(header: u32, facings: [u32; 7]) -> SectionLayer {
    SectionLayer {
        header,
        facings,
        aabb: Aabb::from_min_max(Vec3::ZERO, Vec3::splat(16.0)),
    }
}

#[test]
fn regions_cover_eight_by_eight_chunks_including_negative_ones() {
    assert_eq!(region_of(ChunkPosition { x: 0, z: 7 }), IVec2::new(0, 0));
    assert_eq!(region_of(ChunkPosition { x: 8, z: -1 }), IVec2::new(1, -1));
    assert_eq!(
        region_of(ChunkPosition { x: -8, z: -9 }),
        IVec2::new(-1, -2)
    );
    let offset = section_offset(ChunkPosition { x: -1, z: 9 }, 5);
    assert_eq!(offset & 7, (REGION_CHUNKS - 1) as u32);
    assert_eq!(offset >> 3 & 7, 5);
    assert_eq!(offset >> 6 & 7, 1);
}

#[test]
fn a_camera_inside_a_section_sees_every_facing() {
    let all = UP | DOWN | EAST | WEST | SOUTH | NORTH | UNASSIGNED;
    assert_eq!(
        visible_facings(Vec3::splat(8.0), Vec3::ZERO, Vec3::splat(16.0)),
        all
    );
}

#[test]
fn a_camera_above_and_east_sees_only_tops_and_east_faces() {
    let mask = visible_facings(Vec3::new(40.0, 30.0, 8.0), Vec3::ZERO, Vec3::splat(16.0));
    assert_eq!(mask, UP | EAST | SOUTH | NORTH | UNASSIGNED);
    assert_eq!(mask & (DOWN | WEST), 0);
}

#[test]
fn neighboring_visible_facings_join_into_one_run() {
    let mut list = DrawList::default();
    // Records: up 3, down 2, east 0, west 4, south 1, north 0, unassigned 2.
    let facings = [3, 2, 0, 4, 1, 0, 2];
    list.push(&layer(100, facings), UP | WEST | SOUTH | UNASSIGNED, 9);
    // Up is one run; west and south touch (east is empty) and so do south
    // and unassigned (north is empty).
    assert_eq!(list.len(), 2);
    assert_eq!(list.quads(), 3 + 4 + 1 + 2);
    // The first record after the header is the first up quad.
    assert_eq!(list.resolve(0), Some((101, 9)));
    assert_eq!(list.resolve(2), Some((103, 9)));
    // West starts after up (3) and down (2).
    assert_eq!(list.resolve(3), Some((106, 9)));
    // The last unassigned quad is the layer's last record.
    assert_eq!(list.resolve(9), Some((112, 9)));
    assert_eq!(list.resolve(10), None);
}

#[test]
fn layers_follow_each_other_in_one_list() {
    let mut list = DrawList::default();
    list.push(&layer(0, [1, 0, 0, 0, 0, 0, 0]), u8::MAX, 1);
    list.push(&layer(50, [0, 0, 0, 0, 0, 0, 3]), u8::MAX, 2);
    assert_eq!(list.resolve(0), Some((1, 1)));
    assert_eq!(list.resolve(1), Some((51, 2)));
    assert_eq!(list.resolve(3), Some((53, 2)));
}

#[test]
fn hidden_facings_add_nothing() {
    let mut list = DrawList::default();
    list.push(&layer(0, [5, 0, 0, 0, 0, 0, 0]), DOWN | UNASSIGNED, 0);
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}

#[test]
fn list_bytes_hold_the_header_then_two_entries_per_record() {
    let mut list = DrawList::default();
    for header in 0..3 {
        list.push(&layer(header * 10, [2, 0, 0, 0, 0, 0, 0]), u8::MAX, header);
    }
    let bytes = list.bytes();
    assert_eq!(DrawList::records_for(3), 3);
    assert_eq!(bytes.len(), 3 * QUAD_WORDS * 4);
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert_eq!(&words[..2], &[3, 6]);
    assert_eq!(
        &words[QUAD_WORDS..QUAD_WORDS + 12],
        &[
            2, 1, 0, 0, //
            4, 11, 1, 0, //
            6, 21, 2, 0,
        ]
    );
}

#[test]
fn water_sorts_farthest_quad_first() {
    use game::block::blocks::Block;
    use game::rendering::meshing::sort_back_to_front;
    use game::world::chunk::Chunk;
    use game::world::lighting::Skylight;

    let mut chunk = Chunk::new();
    for x in 0..16 {
        for z in 0..16 {
            chunk.set_with_metadata(x, 2, z, Block::Water, 0);
        }
    }
    let meshes = game::rendering::meshing::mesh_chunk_with_settings(
        &chunk,
        &Skylight::from_chunk(&chunk),
        true,
    );
    let mut records = meshes.water.into_quads();
    assert!(records.len() > 4);
    let eye = [3.0, 10.0, 12.0];
    sort_back_to_front(&mut records, eye);
    let distances: Vec<f32> = records
        .iter()
        .map(|record| {
            let centre = record.centre();
            (0..3).map(|axis| (centre[axis] - eye[axis]).powi(2)).sum()
        })
        .collect();
    assert!(distances.windows(2).all(|pair| pair[0] >= pair[1]));
}

#[test]
fn near_water_sorts_per_block_and_far_water_per_section() {
    use game::world::streaming::needs_sort;
    let camera = ChunkPosition { x: 0, z: 0 };
    let near = ChunkPosition { x: 2, z: -1 };
    let far = ChunkPosition { x: 9, z: 0 };
    let sorted = IVec3::new(5, 70, 5);
    let step = IVec3::new(6, 70, 5);
    assert!(needs_sort(None, sorted, camera, far));
    assert!(!needs_sort(Some(sorted), sorted, camera, near));
    assert!(needs_sort(Some(sorted), step, camera, near));
    // One block over, same section: far water keeps its order.
    assert!(!needs_sort(Some(sorted), step, camera, far));
    assert!(needs_sort(Some(sorted), IVec3::new(17, 70, 5), camera, far));
}

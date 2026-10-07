use bevy::math::IVec3;
use bevy::math::Vec3;
use bevy::mesh::Mesh;
use bevy::mesh::VertexAttributeValues;
use game::block::blocks::Block;
use game::rendering::weather::FANCY_RADIUS;
use game::rendering::weather::FAST_RADIUS;
use game::rendering::weather::Precipitation;
use game::rendering::weather::PrecipitationColumn;
use game::rendering::weather::bolt_mesh;
use game::rendering::weather::column_scroll;
use game::rendering::weather::precipitation_columns;
use game::rendering::weather::precipitation_mesh;
use game::rendering::weather::precipitation_tag;
use game::world::biome::Biome;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::chunk::WorldChunks;

/// One chunk with a stone floor whose top face is at y = 64.
fn floor_world(biome: Biome, edit: impl FnOnce(&mut Chunk)) -> WorldChunks {
    let mut chunk = Chunk::new();
    for x in 0..16 {
        for z in 0..16 {
            chunk.set(x, 63, z, Block::Stone);
        }
    }
    edit(&mut chunk);
    let mut chunks = WorldChunks::default();
    chunks.insert(
        ChunkPosition::ZERO,
        crate::block_ticks::generated(chunk, biome),
    );
    chunks
}

fn columns(chunks: &WorldChunks, radius: i32) -> Vec<PrecipitationColumn> {
    let mut columns = Vec::new();
    precipitation_columns(
        chunks,
        None,
        Vec3::new(8.5, 70.0, 8.5),
        radius,
        0,
        &mut columns,
    );
    columns
}

fn column(columns: &[PrecipitationColumn], x: i32, z: i32) -> Option<PrecipitationColumn> {
    columns
        .iter()
        .copied()
        .find(|column| column.x == x && column.z == z)
}

fn positions(mesh: &Mesh) -> Vec<[f32; 3]> {
    let Some(VertexAttributeValues::Float32x3(values)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("positions should be float triples");
    };
    values.clone()
}

#[test]
fn rain_columns_start_at_the_ground_and_span_the_radius() {
    let chunks = floor_world(Biome::Plains, |_| {});
    let fast = columns(&chunks, FAST_RADIUS);
    assert_eq!(fast.len(), 11 * 11);
    for column in &fast {
        assert_eq!(column.kind, Precipitation::Rain);
        // Five below the eye would be 65; the ground at 64 is lower still.
        assert_eq!((column.bottom, column.top), (65, 75));
    }
    // Fancy reaches ten blocks, which here runs off the one loaded chunk.
    let fancy = columns(&chunks, FANCY_RADIUS);
    assert_eq!(fancy.len(), 16 * 16);
    assert_eq!(
        column(&fancy, 8, 8).map(|c| (c.bottom, c.top)),
        Some((64, 80))
    );
}

#[test]
fn rain_stops_at_roofs_and_glass() {
    let chunks = floor_world(Biome::Plains, |chunk| {
        chunk.set(8, 90, 8, Block::Stone);
        chunk.set(9, 72, 8, Block::Glass);
        chunk.set(10, 72, 8, Block::Torch);
    });
    let columns = columns(&chunks, FAST_RADIUS);
    assert_eq!(column(&columns, 8, 8), None, "nothing falls under a roof");
    assert_eq!(
        column(&columns, 9, 8).map(|c| (c.bottom, c.top)),
        Some((73, 75)),
        "glass is solid to rain"
    );
    assert_eq!(
        column(&columns, 10, 8).map(|c| (c.bottom, c.top)),
        Some((65, 75)),
        "a torch is not"
    );
}

#[test]
fn biome_picks_rain_snow_or_nothing() {
    assert_eq!(Precipitation::of(Biome::Forest), Some(Precipitation::Rain));
    assert_eq!(Precipitation::of(Biome::Taiga), Some(Precipitation::Snow));
    assert_eq!(Precipitation::of(Biome::Tundra), Some(Precipitation::Snow));
    assert_eq!(Precipitation::of(Biome::Desert), None);

    let snowy = columns(&floor_world(Biome::Tundra, |_| {}), FAST_RADIUS);
    assert_eq!(snowy.len(), 11 * 11);
    assert!(snowy.iter().all(|c| c.kind == Precipitation::Snow));
    assert!(columns(&floor_world(Biome::Desert, |_| {}), FAST_RADIUS).is_empty());
}

#[test]
fn rain_dims_with_the_sky_but_never_goes_black() {
    let chunks = floor_world(Biome::Plains, |_| {});
    let mut day = Vec::new();
    let mut night = Vec::new();
    let eye = Vec3::new(8.5, 70.0, 8.5);
    precipitation_columns(&chunks, None, eye, FAST_RADIUS, 0, &mut day);
    precipitation_columns(&chunks, None, eye, FAST_RADIUS, 11, &mut night);
    assert!((day[0].brightness - 1.0).abs() < 1e-5);
    assert!(night[0].brightness < day[0].brightness);
    assert!(night[0].brightness > 0.15);
}

#[test]
fn sheet_mesh_has_two_quads_per_column_relative_to_its_origin() {
    let chunks = floor_world(Biome::Plains, |_| {});
    let columns = columns(&chunks, FAST_RADIUS);
    let origin = IVec3::new(8, 70, 8);
    assert!(precipitation_mesh(&columns, Precipitation::Snow, origin).is_none());
    let mesh = precipitation_mesh(&columns, Precipitation::Rain, origin).unwrap();
    assert_eq!(mesh.count_vertices(), columns.len() * 8);
    let positions = positions(&mesh);
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for [x, y, z] in &positions {
        assert!((-5.0..=6.0).contains(x) && (-5.0..=6.0).contains(z));
        low = low.min(*y);
        high = high.max(*y);
    }
    assert_eq!((low, high), (-5.0, 5.0));
    let Some(VertexAttributeValues::Float32x2(scroll)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
    else {
        panic!("scroll rates should ride in the second UV set");
    };
    assert!(scroll.iter().all(|rate| rate[0] == 0.0 && rate[1] > 0.0));
}

#[test]
fn column_scroll_follows_beta_rates() {
    for (x, z) in [(0, 0), (-7, 12), (100_000, -250_000)] {
        let (offset, rate) = column_scroll(x, z, Precipitation::Rain);
        // `(3 + nextFloat) / 32` texture heights a tick, straight down.
        assert_eq!((offset.x, rate.x), (0.0, 0.0));
        assert!((3.0 / 32.0..4.0 / 32.0).contains(&rate.y));
        assert_eq!(column_scroll(x, z, Precipitation::Rain), (offset, rate));

        let (offset, rate) = column_scroll(x, z, Precipitation::Snow);
        assert!((0.0..1.0).contains(&offset.x) && (0.0..1.0).contains(&offset.y));
        // One texture height every 512 ticks, give or take a small drift.
        assert!((rate.y - 1.0 / 512.0).abs() < 0.01);
        assert!(rate.x.abs() < 0.1);
    }
    assert_ne!(
        column_scroll(0, 0, Precipitation::Rain),
        column_scroll(1, 0, Precipitation::Rain)
    );
}

#[test]
fn tag_packs_strength_radius_and_kind() {
    assert_eq!(precipitation_tag(0.0, 10, Precipitation::Rain).0, 10 << 8);
    assert_eq!(
        precipitation_tag(1.0, 5, Precipitation::Rain).0,
        255 | 5 << 8
    );
    assert_eq!(
        precipitation_tag(1.0, 10, Precipitation::Snow).0,
        255 | 10 << 8 | 1 << 16
    );
}

#[test]
fn bolt_mesh_is_a_seeded_128_block_strike() {
    let bolt = bolt_mesh(42);
    // Four widths of an eight-segment trunk and two three-segment branches,
    // four sides each.
    assert_eq!(bolt.count_vertices(), 4 * (8 + 3 + 3) * 4 * 4);
    let points = positions(&bolt);
    assert_eq!(points, positions(&bolt_mesh(42)));
    assert_ne!(points, positions(&bolt_mesh(43)));
    let low = points.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let high = points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    assert_eq!((low, high), (0.0, 128.0));
    // The trunk lands on the struck block: its lowest ring hugs the origin.
    assert!(
        points
            .iter()
            .any(|p| p[1] == 0.0 && p[0].abs() < 1.0 && p[2].abs() < 1.0)
    );
}

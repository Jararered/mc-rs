use game::block::id::Id;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;
use game::world::meshing::BlockLighting;
use game::world::meshing::mesh_chunk_with_settings;
use game::world::meshing::unpack_vertex;

const X: usize = 8;
const Y: usize = 8;
const Z: usize = 8;
const PISTON_FACE: [usize; 6] = [1, 0, 5, 4, 3, 2];
const OPPOSITE: [usize; 6] = [1, 0, 3, 2, 5, 4];

fn mesh(chunk: &Chunk) -> game::world::meshing::ChunkMeshes {
    mesh_chunk_with_settings(chunk, &Skylight::from_chunk(chunk), false)
}

#[test]
fn isolated_wire_uses_tinted_trace_and_separate_highlight_without_slab_sides() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Id::RedstoneWire);
    let unpowered = mesh(&chunk);
    assert_eq!(unpowered.masked.vertex_count(), 8);
    assert!(
        unpowered
            .masked
            .vertices()
            .iter()
            .all(|v| v.normal == [0.0, 1.0, 0.0])
    );
    assert!(
        unpowered.masked.vertices()[..4]
            .iter()
            .all(|v| v.texel.tile == [4, 10])
    );
    assert!(
        unpowered.masked.vertices()[4..]
            .iter()
            .all(|v| v.texel.tile == [4, 11])
    );
    let trace = unpack_vertex(unpowered.masked.vertices()[0].pack());
    let highlight = unpack_vertex(unpowered.masked.vertices()[4].pack());
    assert_eq!(trace.position[1], Y as f32 + 1.0 / 64.0);
    assert_eq!(highlight.position[1], Y as f32 + 3.0 / 128.0);
    assert_eq!(trace.texel.tile, [4, 10]);
    assert_eq!(highlight.texel.tile, [4, 11]);
    let off = unpowered.masked.colors(BlockLighting {
        old_lighting: false,
        ..Default::default()
    });
    assert!(off[0][0] < 0.1 && off[0][1] == 0.0 && off[0][2] == 0.0);
    assert_eq!(off[4], [1.0; 4]);

    chunk.set_metadata(X, Y, Z, 15);
    let powered = mesh(&chunk);
    let on = powered.masked.colors(BlockLighting {
        old_lighting: false,
        ..Default::default()
    });
    assert!(on[0][0] > 0.9 && on[0][1] > 0.02);
    assert_eq!(on[4], [1.0; 4]);
}

#[test]
fn wire_chooses_straight_and_cropped_junction_tiles() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Id::RedstoneWire);
    chunk.set(X + 1, Y, Z, Id::RedstoneWire);
    let straight = mesh(&chunk);
    assert!(
        straight.masked.vertices()[..4]
            .iter()
            .all(|v| v.texel.tile == [5, 10])
    );

    chunk.set(X + 1, Y, Z, Id::Air);
    chunk.set(X, Y, Z + 1, Id::RedstoneWire);
    let north_south = mesh(&chunk);
    let texels = &north_south.masked.vertices()[..4];
    assert!(texels.iter().all(|v| v.texel.tile == [5, 10]));
    assert_eq!(texels[0].texel.texel, [0, 0]);
    assert_eq!(texels[1].texel.texel, [16, 0]);

    chunk.set(X + 1, Y, Z, Id::RedstoneWire);
    let junction = mesh(&chunk);
    let corners = &junction.masked.vertices()[..4];
    assert!(corners.iter().all(|v| v.texel.tile == [4, 10]));
    assert_eq!(
        corners[0].position,
        [
            X as f32 + 5.0 / 16.0,
            Y as f32 + 1.0 / 64.0,
            Z as f32 + 5.0 / 16.0
        ]
    );
    assert_eq!(
        corners[2].position,
        [X as f32 + 1.0, Y as f32 + 1.0 / 64.0, Z as f32 + 1.0]
    );
}

#[test]
fn wire_climbs_a_solid_neighbor_to_the_next_wire() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Id::RedstoneWire);
    chunk.set(X + 1, Y, Z, Id::Stone);
    chunk.set(X + 1, Y + 1, Z, Id::RedstoneWire);
    let meshes = mesh(&chunk);
    let wall = meshes.masked.vertices().chunks_exact(4).find(|quad| {
        quad.iter().all(|v| v.normal == [-1.0, 0.0, 0.0])
            && quad.iter().any(|v| v.position[1] == Y as f32)
            && quad.iter().any(|v| v.position[1] == (Y + 1) as f32)
    });
    assert!(wall.is_some_and(|quad| quad.iter().all(|v| v.texel.tile == [5, 10])));
}

#[test]
fn piston_faces_use_front_back_side_and_inner_tiles_for_every_facing() {
    for block in [Id::Piston, Id::StickyPiston] {
        for facing in 0..6 {
            let mut chunk = Chunk::new();
            chunk.set(X, Y, Z, block);
            chunk.set_metadata(X, Y, Z, facing);
            let retracted = mesh(&chunk);
            let tiles: Vec<_> = retracted
                .opaque
                .vertices()
                .chunks_exact(4)
                .map(|quad| quad[0].texel.tile)
                .collect();
            assert_eq!(tiles.len(), 6, "{block:?} facing {facing}");
            let front = PISTON_FACE[facing as usize];
            assert_eq!(
                tiles[front],
                [if block == Id::StickyPiston { 10 } else { 11 }, 6]
            );
            assert_eq!(tiles[OPPOSITE[front]], [13, 6]);
            assert_eq!(tiles.iter().filter(|tile| **tile == [12, 6]).count(), 4);

            chunk.set_metadata(X, Y, Z, facing | 8);
            let extended = mesh(&chunk);
            let tiles: Vec<_> = extended
                .opaque
                .vertices()
                .chunks_exact(4)
                .map(|quad| quad[0].texel.tile)
                .collect();
            assert_eq!(tiles.len(), 6, "extended {block:?} facing {facing}");
            assert_eq!(tiles[front], [14, 6]);
            assert_eq!(tiles[OPPOSITE[front]], [13, 6]);
        }
    }
}

#[test]
fn piston_head_has_a_four_sided_rod_reaching_into_the_base() {
    for facing in 0..6 {
        let mut chunk = Chunk::new();
        chunk.set(X, Y, Z, Id::PistonHead);
        chunk.set_metadata(X, Y, Z, facing);
        let meshes = mesh(&chunk);
        let quads: Vec<_> = meshes.opaque.vertices().chunks_exact(4).collect();
        assert_eq!(quads.len(), 10, "facing {facing}");
        assert_eq!(quads[PISTON_FACE[facing as usize]][0].texel.tile, [11, 6]);
        assert!(
            quads[6..]
                .iter()
                .all(|quad| quad.iter().all(|v| v.texel.tile == [12, 6]))
        );
        for vertex in meshes.opaque.vertices() {
            let packed = unpack_vertex(vertex.pack());
            assert_eq!(packed.position, vertex.position);
            assert_eq!(packed.texel, vertex.texel);
        }
        let positions: Vec<_> = quads[6..]
            .iter()
            .flat_map(|quad| quad.iter().map(|v| v.position))
            .collect();
        let axis = [1, 1, 2, 2, 0, 0][facing as usize];
        let base_end = if facing % 2 == 0 { 1.25 } else { -0.25 };
        assert!(
            positions
                .iter()
                .any(|p| p[axis] == [X, Y, Z][axis] as f32 + base_end)
        );
        for position in positions {
            for cross in (0..3).filter(|cross| *cross != axis) {
                assert!((0.375..=0.625).contains(&(position[cross] - [X, Y, Z][cross] as f32)));
            }
        }
    }
}

#[test]
fn sticky_piston_head_uses_the_sticky_face_of_its_base() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Id::StickyPiston);
    chunk.set_metadata(X, Y, Z, 5 | 8);
    chunk.set(X + 1, Y, Z, Id::PistonHead);
    chunk.set_metadata(X + 1, Y, Z, 5);
    let meshes = mesh(&chunk);
    assert!(
        meshes
            .opaque
            .vertices()
            .chunks_exact(4)
            .any(|quad| { quad[0].normal == [1.0, 0.0, 0.0] && quad[0].texel.tile == [10, 6] })
    );
}

use game::block::id::Id;
use game::world::chunk::Chunk;
use game::world::lighting::Skylight;
use game::world::meshing::BlockLighting;
use game::world::meshing::ChunkNeighbors;
use game::world::meshing::mesh_chunk_with_neighbors;
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
            for (face, quad) in retracted.opaque.vertices().chunks_exact(4).enumerate() {
                if face != front && face != OPPOSITE[front] {
                    assert_eq!(quad.iter().map(|v| v.texel.texel[1]).min(), Some(0));
                    assert_eq!(quad.iter().map(|v| v.texel.texel[1]).max(), Some(16));
                }
            }

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
            for (face, quad) in extended.opaque.vertices().chunks_exact(4).enumerate() {
                if face != front && face != OPPOSITE[front] {
                    assert_eq!(quad.iter().map(|v| v.texel.texel[1]).min(), Some(4));
                    assert_eq!(quad.iter().map(|v| v.texel.texel[1]).max(), Some(16));
                }
            }
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
        for (face, quad) in quads[..6].iter().enumerate() {
            if face != PISTON_FACE[facing as usize]
                && face != OPPOSITE[PISTON_FACE[facing as usize]]
            {
                assert_eq!(quad.iter().map(|v| v.texel.texel[1]).min(), Some(0));
                assert_eq!(quad.iter().map(|v| v.texel.texel[1]).max(), Some(4));
            }
        }
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

#[test]
fn stone_faces_remain_visible_across_an_extended_pistons_recess() {
    let mut chunk = Chunk::new();
    chunk.set(X, Y, Z, Id::Piston);
    chunk.set(X + 1, Y, Z, Id::Stone);
    chunk.set_metadata(X, Y, Z, 5 | 8);
    let extended = mesh(&chunk);
    let stone_face = |meshes: &game::world::meshing::ChunkMeshes| {
        meshes.opaque.vertices().chunks_exact(4).any(|quad| {
            quad.iter().all(|v| {
                v.normal == [-1.0, 0.0, 0.0]
                    && v.position[0] == (X + 1) as f32
                    && v.texel.tile == [1, 0]
            })
        })
    };
    assert!(stone_face(&extended));
    assert!(extended.opaque.vertices().chunks_exact(4).any(|quad| {
        quad.iter().all(|v| {
            v.normal == [1.0, 0.0, 0.0]
                && v.position[0] == X as f32 + 0.75
                && v.texel.tile == [14, 6]
        })
    }));

    chunk.set_metadata(X, Y, Z, 5);
    assert!(!stone_face(&mesh(&chunk)));
}

#[test]
fn extended_piston_recess_does_not_cull_stone_across_chunk_boundary() {
    let mut center = Chunk::new();
    center.set(15, Y, Z, Id::Stone);
    let mut east = Chunk::new();
    east.set(0, Y, Z, Id::Piston);
    east.set_metadata(0, Y, Z, 4 | 8);
    let light = Skylight::from_chunk(&center);
    let stone_face_visible = |east: &Chunk| {
        mesh_chunk_with_neighbors(
            &center,
            &ChunkNeighbors {
                east: Some(east),
                ..Default::default()
            },
            &light,
        )
        .vertices()
        .chunks_exact(4)
        .any(|quad| {
            quad.iter().all(|v| {
                v.normal == [1.0, 0.0, 0.0] && v.position[0] == 16.0 && v.texel.tile == [1, 0]
            })
        })
    };
    assert!(stone_face_visible(&east));
    east.set_metadata(0, Y, Z, 4);
    assert!(!stone_face_visible(&east));
    // East is now perpendicular to the piston facing, so the recessed
    // quarter of its side must expose the stone across the chunk boundary.
    east.set_metadata(0, Y, Z, 1 | 8);
    assert!(stone_face_visible(&east));
    east.set_metadata(0, Y, Z, 1);
    assert!(!stone_face_visible(&east));
}

#[test]
fn repeaters_rotate_the_plate_and_move_two_torches_with_delay_and_power() {
    use game::world::textures::block_tile;

    for block in [Id::Repeater, Id::PoweredRepeater] {
        let top_tile = if block == Id::Repeater {
            [3, 8]
        } else {
            [3, 9]
        };
        let torch_tile = if block == Id::Repeater {
            [3, 7]
        } else {
            [3, 6]
        };
        assert_eq!(block_tile(block, 1, false), (torch_tile[0], torch_tile[1]));
        assert_eq!(block_tile(block, 2, false), (5, 0));
        for facing in 0..4_u8 {
            for delay in 0..4_u8 {
                let mut chunk = Chunk::new();
                chunk.set(X, Y, Z, block);
                chunk.set_metadata(X, Y, Z, facing | delay << 2);
                let meshes = mesh(&chunk);
                let quads: Vec<_> = meshes.masked.vertices().chunks_exact(4).collect();
                assert_eq!(quads.len(), 15, "{block:?}, facing {facing}, delay {delay}");
                assert!(meshes.opaque.is_empty());

                let plate = quads[0];
                assert!(plate.iter().all(|v| v.normal == [0.0, 1.0, 0.0]
                    && v.position[1] == Y as f32 + 2.0 / 16.0
                    && v.texel.tile == top_tile));
                let expected_top_uvs = match facing {
                    0 => [[0, 0], [0, 16], [16, 16], [16, 0]],
                    1 => [[0, 16], [16, 16], [16, 0], [0, 0]],
                    2 => [[16, 16], [16, 0], [0, 0], [0, 16]],
                    _ => [[16, 0], [0, 0], [0, 16], [16, 16]],
                };
                assert_eq!(
                    plate.iter().map(|v| v.texel.texel).collect::<Vec<_>>(),
                    expected_top_uvs
                );
                assert!(quads[1..5].iter().all(|quad| quad.iter().all(|v| {
                    v.texel.tile == [5, 0] && v.position[1] <= Y as f32 + 2.0 / 16.0
                })));
                let cap_centers: Vec<_> = [quads[5], quads[10]]
                    .map(|quad| {
                        assert!(quad.iter().all(|v| {
                            v.normal == [0.0, 1.0, 0.0]
                                && v.position[1] == Y as f32 + 7.0 / 16.0
                                && v.texel.tile == torch_tile
                        }));
                        [
                            quad.iter().map(|v| v.position[0]).sum::<f32>() / 4.0 - X as f32,
                            quad.iter().map(|v| v.position[2]).sum::<f32>() / 4.0 - Z as f32,
                        ]
                    })
                    .into();
                let movable = [-1.0, 1.0, 3.0, 5.0][delay as usize] / 16.0;
                let offsets = match facing {
                    0 => [[0.0, -5.0 / 16.0], [0.0, movable]],
                    1 => [[5.0 / 16.0, 0.0], [-movable, 0.0]],
                    2 => [[0.0, 5.0 / 16.0], [0.0, -movable]],
                    _ => [[-5.0 / 16.0, 0.0], [movable, 0.0]],
                };
                assert_eq!(cap_centers, offsets.map(|[dx, dz]| [0.5 + dx, 0.5 + dz]));
                assert!(
                    quads[5..]
                        .iter()
                        .all(|quad| { quad.iter().all(|v| v.texel.tile == torch_tile) })
                );
            }
        }
    }
}

#[test]
fn extended_piston_exposes_neighbors_along_all_sides_but_not_the_back() {
    let directions: [[i32; 3]; 6] = [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ];
    for block in [Id::Piston, Id::StickyPiston] {
        for (facing, forward) in directions.iter().enumerate() {
            for adjacent in directions {
                let mut chunk = Chunk::new();
                chunk.set(X, Y, Z, block);
                chunk.set_metadata(X, Y, Z, facing as u8 | 8);
                chunk.set(
                    (X as i32 + adjacent[0]) as usize,
                    (Y as i32 + adjacent[1]) as usize,
                    (Z as i32 + adjacent[2]) as usize,
                    Id::Stone,
                );
                let meshes = mesh(&chunk);
                let stone_face = meshes.opaque.vertices().chunks_exact(4).any(|quad| {
                    quad.iter().all(|v| {
                        v.normal == adjacent.map(|n| -(n as f32)) && v.texel.tile == [1, 0]
                    })
                });
                assert_eq!(
                    stone_face,
                    adjacent != forward.map(|n| -n),
                    "{block:?} facing {facing} adjacent {adjacent:?}"
                );
            }
        }
    }
}

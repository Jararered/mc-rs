use game::world::block::block::BlockId;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPos;
use game::world::generation::Biome;
use game::world::generation::WorldGenerator;

fn count(chunk: &Chunk, block: BlockId) -> usize {
    let mut total = 0;
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if chunk.get(x, y, z) == Some(block) {
                    total += 1;
                }
            }
        }
    }
    total
}

fn is_leaf(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves
    )
}

fn is_wood(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Wood | BlockId::SpruceWood | BlockId::BirchWood
    )
}

#[test]
fn forest_chunk_contains_grounded_trees_with_canopies() {
    let generated = WorldGenerator::new(0).generate(ChunkPos::ZERO);
    assert_eq!(generated.biomes.get(8, 8).biome, Biome::Forest);
    let chunk = &generated.chunk;

    let mut trunks = 0;
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let Some(base) = (0..CHUNK_HEIGHT).find(|&y| is_wood(chunk.get(x, y, z).unwrap()))
            else {
                continue;
            };
            // Branch columns have air below; only inspect grounded trunks.
            if !matches!(
                chunk.get(x, base - 1, z),
                Some(BlockId::Dirt | BlockId::Grass)
            ) {
                continue;
            }
            assert!(
                is_wood(chunk.get(x, base + 1, z).unwrap()),
                "trunk at ({x}, {base}, {z}) should stack wood above its base"
            );
            let has_canopy = (base..base + 8).any(|y| {
                (-3..=3).any(|dx| {
                    (-3..=3).any(|dz| {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        (0..CHUNK_SIZE as i32).contains(&nx)
                            && (0..CHUNK_SIZE as i32).contains(&nz)
                            && is_leaf(chunk.get(nx as usize, y, nz as usize).unwrap())
                    })
                })
            });
            assert!(has_canopy, "trunk at ({x}, {base}, {z}) has no canopy");
            trunks += 1;
        }
    }
    assert!(trunks > 0, "a forest chunk should contain grounded trees");
}

#[test]
fn taiga_chunks_generate_spruce_trees() {
    for seed in 0..8 {
        let generator = WorldGenerator::new(seed);
        for z in (-8..=8).step_by(2) {
            for x in (-8..=8).step_by(2) {
                let generated = generator.generate(ChunkPos { x, z });
                let contains_taiga = generated
                    .biomes
                    .cells()
                    .iter()
                    .any(|climate| climate.biome == Biome::Taiga);
                if !contains_taiga {
                    continue;
                }
                let spruce_trunks = count(&generated.chunk, BlockId::SpruceWood);
                let spruce_leaves = count(&generated.chunk, BlockId::SpruceLeaves);
                if spruce_trunks > 0 {
                    assert!(spruce_leaves > 0, "spruce trunks need spruce canopies");
                    return;
                }
            }
        }
    }
    panic!("expected a sampled taiga region to generate spruce trees");
}

#[test]
fn tree_generation_is_deterministic() {
    let position = ChunkPos { x: -2, z: 3 };
    let first = WorldGenerator::new(7).generate(position);
    let second = WorldGenerator::new(7).generate(position);
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                assert_eq!(first.chunk.get(x, y, z), second.chunk.get(x, y, z));
            }
        }
    }
}

#[test]
fn desert_chunks_generate_dead_bushes_on_sand() {
    for seed in 0..128 {
        let generated = WorldGenerator::new(seed).generate(ChunkPos::ZERO);
        if !generated
            .biomes
            .cells()
            .iter()
            .any(|climate| climate.biome == Biome::Desert)
        {
            continue;
        }

        let bushes = count(&generated.chunk, BlockId::DeadBush);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                for y in 1..CHUNK_HEIGHT {
                    if generated.chunk.get(x, y, z) == Some(BlockId::DeadBush) {
                        assert_eq!(generated.chunk.get(x, y - 1, z), Some(BlockId::Sand));
                    }
                }
            }
        }
        if bushes > 0 {
            return;
        }
    }
    panic!("seeded desert chunks should eventually generate dead bushes");
}

#[test]
fn dry_biomes_have_no_trees() {
    // Seed 1 puts the tree biome at the chunk's far corner in the desert, which
    // subtracts more trees than the density noise can add.
    let generated = WorldGenerator::new(1).generate(ChunkPos::ZERO);
    assert_eq!(count(&generated.chunk, BlockId::Wood), 0);
    assert_eq!(count(&generated.chunk, BlockId::Leaves), 0);
}

#[test]
fn column_top_matches_generated_terrain() {
    let generator = WorldGenerator::new(0);
    // Include a neighbouring chunk: decoration queries columns outside the chunk
    // it is generating, so the query must match that neighbour's terrain too.
    for position in [
        ChunkPos::ZERO,
        ChunkPos { x: 1, z: 0 },
        ChunkPos { x: -1, z: 2 },
    ] {
        let generated = generator.generate(position);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let expected = (0..CHUNK_HEIGHT)
                    .rev()
                    .find(|&y| {
                        matches!(
                            generated.chunk.get(x, y, z),
                            Some(
                                BlockId::Stone
                                    | BlockId::Dirt
                                    | BlockId::Grass
                                    | BlockId::Sand
                                    | BlockId::Gravel
                                    | BlockId::Bedrock
                            )
                        )
                    })
                    .unwrap();
                let world_x = position.x * CHUNK_SIZE as i32 + x as i32;
                let world_z = position.z * CHUNK_SIZE as i32 + z as i32;
                assert_eq!(
                    generator.column_top(world_x, world_z),
                    expected,
                    "column height mismatch at ({world_x}, {world_z})"
                );
            }
        }
    }
}

fn grounded_trunk_base(chunk: &Chunk, x: usize, z: usize) -> Option<usize> {
    let base = (0..CHUNK_HEIGHT).find(|&y| is_wood(chunk.get(x, y, z).unwrap()))?;
    matches!(
        chunk.get(x, base - 1, z),
        Some(BlockId::Dirt | BlockId::Grass)
    )
    .then_some(base)
}

fn has_nearby_leaf(chunk: &Chunk, x0: usize, z0: usize, base: usize, along_x: bool) -> bool {
    (base..base + 20).any(|y| {
        (0..4).any(|offset| {
            (-1..=1).any(|side| {
                let (lx, lz) = if along_x {
                    (x0 as i32 + offset as i32, z0 as i32 + side)
                } else {
                    (x0 as i32 + side, z0 as i32 + offset as i32)
                };
                (0..CHUNK_SIZE as i32).contains(&lx)
                    && (0..CHUNK_SIZE as i32).contains(&lz)
                    && (0..CHUNK_HEIGHT).contains(&y)
                    && is_leaf(chunk.get(lx as usize, y, lz as usize).unwrap())
            })
        })
    })
}

/// A trunk on a chunk face always grows leaves one block into the neighbour.
/// Check every seed and both sides of the +x and +z seams; returning after the
/// first matching tree hid most cut-off canopies.
#[test]
fn canopies_continue_across_chunk_boundaries() {
    let mut boundary_trunks = 0;
    for seed in 0..16 {
        let generator = WorldGenerator::new(seed);
        let west = generator.generate(ChunkPos::ZERO);
        let east = generator.generate(ChunkPos { x: 1, z: 0 });
        let north = generator.generate(ChunkPos { x: 0, z: 1 });

        for z in 0..CHUNK_SIZE {
            if let Some(base) = grounded_trunk_base(&west.chunk, CHUNK_SIZE - 1, z) {
                boundary_trunks += 1;
                assert!(
                    has_nearby_leaf(&east.chunk, 0, z, base, true),
                    "seed {seed}: west trunk at ({}, {base}, {z}) has no canopy in the east chunk",
                    CHUNK_SIZE - 1
                );
            }
            if let Some(base) = grounded_trunk_base(&east.chunk, 0, z) {
                boundary_trunks += 1;
                assert!(
                    has_nearby_leaf(&west.chunk, CHUNK_SIZE - 4, z, base, true),
                    "seed {seed}: east trunk at (0, {base}, {z}) has no canopy in the west chunk"
                );
            }
        }

        for x in 0..CHUNK_SIZE {
            if let Some(base) = grounded_trunk_base(&west.chunk, x, CHUNK_SIZE - 1) {
                boundary_trunks += 1;
                assert!(
                    has_nearby_leaf(&north.chunk, x, 0, base, false),
                    "seed {seed}: south trunk at ({x}, {base}, {}) has no canopy in the north chunk",
                    CHUNK_SIZE - 1
                );
            }
        }
    }
    assert!(
        boundary_trunks > 0,
        "no tree against a chunk border was found across seeds"
    );
}

fn terrain_top(chunk: &Chunk, x: usize, z: usize) -> (usize, BlockId) {
    let y = (0..CHUNK_HEIGHT)
        .rev()
        .find(|&y| {
            matches!(
                chunk.get(x, y, z),
                Some(
                    BlockId::Stone
                        | BlockId::Dirt
                        | BlockId::Grass
                        | BlockId::Sand
                        | BlockId::Gravel
                        | BlockId::Bedrock
                        | BlockId::Water
                        | BlockId::Ice
                )
            )
        })
        .unwrap_or(0);
    (y, chunk.get(x, y, z).unwrap())
}

#[test]
fn trees_do_not_plant_on_sand() {
    let mut sand_columns = 0;
    for seed in 0..4 {
        let generator = WorldGenerator::new(seed);
        for z in -4..4 {
            for x in -4..4 {
                let generated = generator.generate(ChunkPos { x, z });
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        let (top, block) = terrain_top(&generated.chunk, lx, lz);
                        if block != BlockId::Sand {
                            continue;
                        }
                        sand_columns += 1;
                        for y in top + 1..CHUNK_HEIGHT {
                            let placed = generated.chunk.get(lx, y, lz).unwrap();
                            assert!(
                                !is_wood(placed),
                                "seed {seed}: trunk on sand at chunk ({x},{z}) column ({lx}, {y}, {lz})"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(sand_columns > 0, "expected to sample some beach columns");
}

/// Leaves may hang over sand, but they belong to a tree whose trunk is on
/// grass or dirt. A canopy with no nearby trunk is the neighbour planting a
/// tree the origin chunk rejected because the column was a beach.
#[test]
fn canopies_over_sand_still_have_a_trunk() {
    for seed in 0..8 {
        let generator = WorldGenerator::new(seed);
        let mut chunks = Vec::new();
        for z in 0..2 {
            for x in 0..2 {
                chunks.push((ChunkPos { x, z }, generator.generate(ChunkPos { x, z })));
            }
        }

        let block_at = |wx: i32, y: usize, wz: i32| -> Option<BlockId> {
            let pos = ChunkPos {
                x: wx.div_euclid(CHUNK_SIZE as i32),
                z: wz.div_euclid(CHUNK_SIZE as i32),
            };
            let lx = wx.rem_euclid(CHUNK_SIZE as i32) as usize;
            let lz = wz.rem_euclid(CHUNK_SIZE as i32) as usize;
            chunks
                .iter()
                .find(|(chunk_pos, _)| *chunk_pos == pos)
                .and_then(|(_, generated)| generated.chunk.get(lx, y, lz))
        };

        for (pos, generated) in &chunks {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let (top, block) = terrain_top(&generated.chunk, x, z);
                    if block != BlockId::Sand {
                        continue;
                    }
                    let has_leaf = (top + 1..CHUNK_HEIGHT)
                        .any(|y| is_leaf(generated.chunk.get(x, y, z).unwrap()));
                    if !has_leaf {
                        continue;
                    }
                    let wx = pos.x * CHUNK_SIZE as i32 + x as i32;
                    let wz = pos.z * CHUNK_SIZE as i32 + z as i32;
                    let mut found_trunk = false;
                    'search: for dz in -6..=6 {
                        for dx in -6..=6 {
                            let nx = wx + dx;
                            let nz = wz + dz;
                            for y in 0..CHUNK_HEIGHT {
                                if !is_wood(block_at(nx, y, nz).unwrap_or(BlockId::Air)) {
                                    continue;
                                }
                                if matches!(
                                    block_at(nx, y.saturating_sub(1), nz),
                                    Some(BlockId::Dirt | BlockId::Grass)
                                ) {
                                    found_trunk = true;
                                    break 'search;
                                }
                            }
                        }
                    }
                    assert!(
                        found_trunk,
                        "seed {seed}: leaves over sand at ({wx}, {wz}) have no nearby trunk"
                    );
                }
            }
        }
    }
}

#[test]
fn timing_probe() {
    use std::time::Instant;
    let generator = WorldGenerator::new(0);
    let start = Instant::now();
    let mut blocks = 0usize;
    for z in -5..5 {
        for x in -5..5 {
            let generated = generator.generate(ChunkPos { x, z });
            blocks += count(&generated.chunk, BlockId::Leaves);
        }
    }
    let elapsed = start.elapsed();
    println!("100 chunks in {elapsed:?} ({blocks} leaves)");
}

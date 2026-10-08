//! The Nether generator. `persistence::original` compares its terrain with the
//! Beta 1.7.3 server's own `DIM-1` chunks when the reference checkout is
//! present; the hashes here pin the same output without it.

use game::block::blocks::Block;
use game::world::biome::Biome;
use game::world::chunk::CHUNK_HEIGHT;
use game::world::chunk::CHUNK_SIZE;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::generation::ChunkGenerator;
use game::world::generation::generate_area;
use game::world::generation::nether::NetherGenerator;

use super::overworld::beta_reference::SEED;

/// FNV-1a over the raw block bytes.
fn hash(chunk: &Chunk) -> u64 {
    chunk
        .raw_blocks()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, value| {
            (hash ^ u64::from(*value)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// `(x, z, hash)` of base chunks for the reference seed: terrain, the soul
/// sand, gravel and bedrock pass, and caves.
const BASE_CHUNKS: [(i32, i32, u64); 4] = [
    (0, 0, 0xc09c_8431_d966_943b),
    (-3, 2, 0x3262_3a56_0d2a_5052),
    (5, -4, 0x3a47_22c0_6082_1d01),
    (-7, -7, 0xddac_a6b9_a8ff_87eb),
];

#[test]
fn base_chunks_keep_their_pinned_blocks() {
    let generator = NetherGenerator::new(SEED as u64);
    for (x, z, expected) in BASE_CHUNKS {
        let base = generator.generate_base(ChunkPosition { x, z });
        assert_eq!(
            hash(&base.chunk),
            expected,
            "Nether base chunk ({x}, {z}) changed: {:#018x}",
            hash(&base.chunk)
        );
    }
}

#[test]
fn the_nether_is_sealed_in_bedrock_and_made_of_nether_blocks() {
    let generator = NetherGenerator::new(SEED as u64);
    let base = generator.generate_base(ChunkPosition { x: 2, z: -1 });
    assert!(!base.populated);
    assert_eq!(base.biomes.get(3, 12).biome, Biome::Hell);
    let mut lava = 0;
    let mut open = 0;
    for x in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            assert_eq!(base.chunk.get(x, 0, z), Some(Block::Bedrock));
            assert_eq!(base.chunk.get(x, CHUNK_HEIGHT - 1, z), Some(Block::Bedrock));
            for y in 0..CHUNK_HEIGHT {
                let block = base.chunk.get(x, y, z).unwrap();
                assert!(
                    matches!(
                        block,
                        Block::Air
                            | Block::Bedrock
                            | Block::Netherrack
                            | Block::SoulSand
                            | Block::Gravel
                            | Block::Lava
                    ),
                    "{block:?} at ({x}, {y}, {z})"
                );
                // The lava sea fills what is open below y 32 and nothing above.
                if block == Block::Lava {
                    assert!(y < 32);
                    lava += 1;
                }
                open += usize::from(block == Block::Air);
            }
        }
    }
    assert!(open > 1000, "the chunk has caverns");
    assert!(lava > 0 || open > 0);
}

#[test]
fn population_is_the_same_whatever_else_was_generated_first() {
    let position = ChunkPosition { x: 1, z: 1 };
    let alone = NetherGenerator::new(SEED as u64);
    let first = generate_area(&alone, position, 0);

    // Beta's `hellRNG` carries over from the last chunk provided; this
    // generator's population must not.
    let busy = NetherGenerator::new(SEED as u64);
    for x in -4..0 {
        busy.generate_base(ChunkPosition { x, z: 9 });
    }
    let second = generate_area(&busy, position, 0);
    assert_eq!(
        first[&position].chunk.raw_blocks(),
        second[&position].chunk.raw_blocks()
    );
    assert!(first[&position].populated);
}

#[test]
fn population_hangs_glowstone_and_lights_fires_on_netherrack() {
    let generator = NetherGenerator::new(SEED as u64);
    let area = generate_area(&generator, ChunkPosition::ZERO, 2);
    let mut glowstone = 0;
    let mut fire = 0;
    for (position, generated) in &area {
        if !generated.populated {
            continue;
        }
        for (index, raw) in generated.chunk.raw_blocks().iter().enumerate() {
            let (x, z, y) = (index % 16, index / 16 % 16, index / 256);
            match Block::from(*raw) {
                Block::Glowstone => glowstone += 1,
                Block::Fire => {
                    fire += 1;
                    assert_eq!(
                        generated.chunk.get(x, y - 1, z),
                        Some(Block::Netherrack),
                        "fire at ({x}, {y}, {z}) of {position:?}"
                    );
                }
                block => assert!(
                    !matches!(block, Block::Water | Block::FlowingWater | Block::Stone),
                    "{block:?} in the Nether"
                ),
            }
        }
    }
    assert!(glowstone > 20, "only {glowstone} glowstone in 25 chunks");
    assert!(fire > 0, "no fire in 25 chunks");
}

//! Chunks saved by the real Beta 1.7.3 server, in the world under
//! `refs/mc_b1.7.3_release/1.7.3-LTS/jars/world`.
//!
//! Each hash covers every block of one chunk, species included. The saved
//! world ran for 606 ticks after generating, in which gravel placed by
//! population fell, springs flowed, and lava settled or hardened. Spring
//! lava uses the world RNG (not the population RNG), so the exact spread
//! differs between server runs. Air, fluids, gravel, and obsidian are
//! therefore hashed alike. The server populated its spawn area `x`-major, as
//! [`OverworldGenerator::generate_area`] does.

use game::block::blocks::Block;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::generation::overworld::OverworldGenerator;

pub(crate) const SEED: i64 = -5_779_659_068_535_663_308;

/// `(x, z, hash)`: two forests, oak and birch, a dungeon with two chests,
/// reeds, clay, and every ore between them.
pub(crate) const CHUNKS: [(i32, i32, u64); 7] = [
    (-12, -8, 0xba11_92c0_a2ab_994d),
    (-6, -2, 0xcea6_153a_ba18_27dc),
    (-4, -8, 0xd9d2_cebc_7ee3_f065),
    (-1, 3, 0x6a6f_4e09_7733_c495),
    (1, -9, 0x51f1_4c9a_5458_1ed1),
    (3, 9, 0xe383_af3c_3765_72a0),
    (9, -6, 0x8020_6c77_0dca_e690),
];

/// A block as the saved world stores it: Beta's ID in the high bits and, for
/// logs, leaves, and tall grass, its species.
fn beta_value(block: Block, metadata: u8) -> u16 {
    let (id, species) = match block {
        Block::Air
        | Block::Water
        | Block::FlowingWater
        | Block::Lava
        | Block::FlowingLava
        | Block::Gravel
        | Block::Obsidian => (0, 0),
        Block::Wood | Block::Leaves => (u16::from(block.as_u8()), metadata & 3),
        // Tall grass with no metadata is Beta's metadata 1.
        Block::TallGrass => (31, if metadata & 3 == 2 { 2 } else { 1 }),
        // Facing is not part of what the hash pins.
        Block::Pumpkin | Block::Chest => (u16::from(block.as_u8()), 0),
        other => (u16::from(other.as_u8()), 0),
    };
    id << 4 | u16::from(species)
}

/// FNV-1a over [`beta_value`] in [`Chunk::index`] order.
pub(crate) fn hash(chunk: &Chunk) -> u64 {
    chunk
        .raw_blocks()
        .iter()
        .enumerate()
        .map(|(index, &raw)| {
            let (x, z, y) = (index % 16, index / 16 % 16, index / 256);
            beta_value(Block::from(raw), chunk.metadata(x, y, z))
        })
        .fold(0xcbf2_9ce4_8422_2325, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

#[test]
fn chunks_match_the_beta_server_world() {
    let generator = OverworldGenerator::new(SEED as u64);
    for (x, z, expected) in CHUNKS {
        let position = ChunkPosition { x, z };
        let area = generator.generate_area(position, 1);
        assert_eq!(
            hash(&area[&position].chunk),
            expected,
            "chunk ({x}, {z}) no longer matches the Beta 1.7.3 server world"
        );
    }
}

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

use game::block::id::Id;
use game::world::chunk::Chunk;
use game::world::chunk::ChunkPosition;
use game::world::generation::overworld::OverworldGenerator;

const SEED: i64 = -5_779_659_068_535_663_308;

/// `(x, z, hash)`: two forests, oak and birch, a dungeon with two chests,
/// reeds, clay, and every ore between them.
const CHUNKS: [(i32, i32, u64); 7] = [
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
fn beta_value(block: Id) -> u16 {
    let (id, species) = match block {
        Id::Air
        | Id::Water
        | Id::FlowingWater
        | Id::Lava
        | Id::FlowingLava
        | Id::Gravel
        | Id::Obsidian => (0, 0),
        Id::Wood => (17, 0),
        Id::SpruceWood => (17, 1),
        Id::BirchWood => (17, 2),
        Id::Leaves => (18, 0),
        Id::SpruceLeaves => (18, 1),
        Id::BirchLeaves => (18, 2),
        Id::TallGrass => (31, 1),
        Id::Fern => (31, 2),
        Id::PumpkinNorth | Id::PumpkinEast | Id::PumpkinSouth | Id::PumpkinWest => (86, 0),
        Id::ChestNorth | Id::ChestEast | Id::ChestSouth | Id::ChestWest => (54, 0),
        other => (u16::from(other.as_u8()), 0),
    };
    id << 4 | species
}

/// FNV-1a over [`beta_value`] in [`Chunk::index`] order.
fn hash(chunk: &Chunk) -> u64 {
    chunk
        .raw_blocks()
        .iter()
        .map(|&raw| beta_value(Id::from(raw)))
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

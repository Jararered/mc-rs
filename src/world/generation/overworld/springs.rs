//! Overworld `WorldGenLiquids`: water and lava springs at the end of population.

use bevy::math::IVec3;

use crate::block::id::Id;
use crate::random::JavaRandom;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::TickWorld;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::lighting::LightCache;

use crate::world::chunk::GeneratedChunk;

/// Java `WorldGenLiquids.generate`. Only a stone pocket with exactly one
/// horizontal air opening receives a spring. Its first tick runs directly;
/// while that tick runs, scheduled fluid updates run immediately as in Beta.
/// The boolean reports whether the pocket passed the Java preconditions,
/// *not* whether a block was placed.
pub fn generate_spring(world: &mut TickWorld, position: IVec3, liquid: Id) -> bool {
    assert!(matches!(liquid, Id::FlowingWater | Id::FlowingLava));
    if world.block(position + IVec3::Y) != Id::Stone
        || world.block(position - IVec3::Y) != Id::Stone
        || !matches!(world.block(position), Id::Air | Id::Stone)
    {
        return false;
    }
    let sides = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z];
    let stones = sides
        .into_iter()
        .filter(|&offset| world.block(position + offset) == Id::Stone)
        .count();
    let air = sides
        .into_iter()
        .filter(|&offset| world.is_air(position + offset))
        .count();
    if stones == 3 && air == 1 {
        world.set_block_notify(position, liquid);
        world.set_immediate(true);
        world.update_tick(position);
        world.flush_deferred();
        world.set_immediate(false);
    }
    true
}

/// Populate the four live chunks, before the final snow pass. The Java
/// coordinate RNG is independent of the world's RNG used by nested fluid
/// ticks; unlike Java's runtime-seeded world RNG, ours is deterministically
/// seeded per source chunk so independently generated chunks agree.
pub(super) fn populate(
    source: ChunkPosition,
    seed: u64,
    rand: &mut JavaRandom,
    chunks: [GeneratedChunk; 4],
) -> [GeneratedChunk; 4] {
    let positions = [
        source,
        ChunkPosition {
            x: source.x + 1,
            z: source.z,
        },
        ChunkPosition {
            x: source.x,
            z: source.z + 1,
        },
        ChunkPosition {
            x: source.x + 1,
            z: source.z + 1,
        },
    ];
    let mut loaded = WorldChunks::default();
    for (position, generated) in positions.into_iter().zip(chunks) {
        loaded.insert(position, generated);
    }
    let flow_seed = seed
        ^ (source.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (source.z as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut ticks = BlockTicks::new(flow_seed);
    let mut light = LightCache::default();
    for position in positions {
        ticks.load_chunk(position, &mut loaded.get_mut(position).unwrap().chunk);
    }
    {
        let mut world = ticks.world(&mut loaded, &mut light, 0);
        generate_attempts(&mut world, rand, source);
    }
    positions.map(|position| {
        let mut result = loaded.remove(position).unwrap();
        ticks.unload_chunk(position, &mut result.chunk);
        result
    })
}

/// Java `ChunkProviderGenerate.populate`'s 50 water attempts followed by
/// 20 lava attempts. Every coordinate draw happens even if the pocket fails.
pub fn generate_attempts(world: &mut TickWorld, rand: &mut JavaRandom, source: ChunkPosition) {
    let ox = source.x * 16;
    let oz = source.z * 16;
    for _ in 0..50 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let bound = rand.next_int(120) + 8;
        let y = rand.next_int(bound) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        generate_spring(world, IVec3::new(x, y, z), Id::FlowingWater);
    }
    for _ in 0..20 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let bound = rand.next_int(112) + 8;
        let bound = rand.next_int(bound) + 8;
        let y = rand.next_int(bound) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        generate_spring(world, IVec3::new(x, y, z), Id::FlowingLava);
    }
}

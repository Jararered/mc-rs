//! Beta `ChunkProviderHell.populate`: lava falls, fire, glowstone and
//! mushrooms, in the reference's order on one random sequence.
//!
//! Beta never reseeds `hellRNG` here, so its Nether decoration depends on
//! which chunk happened to generate last. This port seeds the sequence per
//! chunk, as the Overworld does, so a chunk decorates the same way whatever
//! order the world streams in.

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::random::JavaRandom;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::TickWorld;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::GeneratedChunk;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::Dimension;
use crate::world::lighting::LightCache;

use super::super::overworld::plants::flower_patch;
use super::super::population_footprint;
use super::world::PopulationWorld;

/// `WorldGenHellLava`: a netherrack pocket open on exactly one side or below
/// becomes a lava source, whose first tick runs at once with scheduled
/// updates immediate, as in Beta. Returns whether the cell passed the
/// preconditions, not whether lava was placed.
pub fn generate_lava_fall(world: &mut TickWorld, position: IVec3) -> bool {
    if world.block(position + IVec3::Y) != Block::Netherrack
        || !matches!(world.block(position), Block::Air | Block::Netherrack)
    {
        return false;
    }
    let around = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_Y];
    let netherrack = around
        .into_iter()
        .filter(|&offset| world.block(position + offset) == Block::Netherrack)
        .count();
    let air = around
        .into_iter()
        .filter(|&offset| world.is_air(position + offset))
        .count();
    if netherrack == 4 && air == 1 {
        world.set_block_notify(position, Block::FlowingLava);
        world.set_immediate(true);
        world.update_tick(position);
        world.flush_deferred();
        world.set_immediate(false);
    }
    true
}

/// The eight lava attempts that open `populate`. They need the block update
/// system, so the four chunks are lent to a temporary tick world.
pub(super) fn lava_falls(
    source: ChunkPosition,
    seed: u64,
    rand: &mut JavaRandom,
    chunks: [GeneratedChunk; 4],
) -> [GeneratedChunk; 4] {
    let positions = population_footprint(source);
    let mut loaded = WorldChunks::default();
    for (position, generated) in positions.into_iter().zip(chunks) {
        loaded.insert(position, generated);
    }
    let flow_seed = seed
        ^ (source.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (source.z as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let mut ticks = BlockTicks::new(flow_seed);
    ticks.set_dimension(Dimension::Nether);
    let mut light = LightCache::default();
    for position in positions {
        ticks.load_chunk(position, &mut loaded.get_mut(position).unwrap().chunk);
    }
    {
        let mut world = ticks.world(&mut loaded, &mut light, 0);
        let ox = source.x * 16;
        let oz = source.z * 16;
        for _ in 0..8 {
            let x = ox + rand.next_int(16) as i32 + 8;
            let y = rand.next_int(120) as i32 + 4;
            let z = oz + rand.next_int(16) as i32 + 8;
            generate_lava_fall(&mut world, IVec3::new(x, y, z));
        }
    }
    positions.map(|position| {
        let mut result = loaded.remove(position).unwrap();
        ticks.unload_chunk(position, &mut result.chunk);
        result
    })
}

/// `WorldGenFire`: 64 attempts at air over netherrack.
fn fire_patch(world: &mut PopulationWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32) {
    for _ in 0..64 {
        let x = x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = y + rand.next_int(4) as i32 - rand.next_int(4) as i32;
        let z = z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        if world.is_air(x, y, z) && world.get(x, y - 1, z) == Block::Netherrack {
            world.set(x, y, z, Block::Fire);
        }
    }
}

/// `WorldGenGlowStone1` and `WorldGenGlowStone2`, which are the same class
/// twice: a seed under a netherrack ceiling, then 1500 attempts that each
/// extend the cluster only where a cell touches exactly one glowstone, which
/// is what makes it grow as hanging tendrils rather than a blob.
fn glowstone_cluster(world: &mut PopulationWorld, rand: &mut JavaRandom, x: i32, y: i32, z: i32) {
    if !world.is_air(x, y, z) || world.get(x, y + 1, z) != Block::Netherrack {
        return;
    }
    world.set(x, y, z, Block::Glowstone);
    for _ in 0..1500 {
        let x = x + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        let y = y - rand.next_int(12) as i32;
        let z = z + rand.next_int(8) as i32 - rand.next_int(8) as i32;
        if !world.is_air(x, y, z) {
            continue;
        }
        let touching = [
            (x - 1, y, z),
            (x + 1, y, z),
            (x, y - 1, z),
            (x, y + 1, z),
            (x, y, z - 1),
            (x, y, z + 1),
        ]
        .into_iter()
        .filter(|&(x, y, z)| world.get(x, y, z) == Block::Glowstone)
        .count();
        if touching == 1 {
            world.set(x, y, z, Block::Glowstone);
        }
    }
}

/// Everything in `populate` after the lava falls.
pub(super) fn decorate(world: &mut PopulationWorld, rand: &mut JavaRandom) {
    let source = world.origin();
    let ox = source.x * 16;
    let oz = source.z * 16;

    let bound = rand.next_int(10) + 1;
    let count = rand.next_int(bound) + 1;
    for _ in 0..count {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(120) as i32 + 4;
        let z = oz + rand.next_int(16) as i32 + 8;
        fire_patch(world, rand, x, y, z);
    }

    let bound = rand.next_int(10) + 1;
    let count = rand.next_int(bound);
    for _ in 0..count {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(120) as i32 + 4;
        let z = oz + rand.next_int(16) as i32 + 8;
        glowstone_cluster(world, rand, x, y, z);
    }

    for _ in 0..10 {
        let x = ox + rand.next_int(16) as i32 + 8;
        let y = rand.next_int(128) as i32;
        let z = oz + rand.next_int(16) as i32 + 8;
        glowstone_cluster(world, rand, x, y, z);
    }

    // `nextInt(1) == 0` always holds; the draw is kept for the sequence.
    for mushroom in [Block::BrownMushroom, Block::RedMushroom] {
        if rand.next_int(1) == 0 {
            let x = ox + rand.next_int(16) as i32 + 8;
            let y = rand.next_int(128) as i32;
            let z = oz + rand.next_int(16) as i32 + 8;
            flower_patch(world, rand, x, y, z, mushroom);
        }
    }
}

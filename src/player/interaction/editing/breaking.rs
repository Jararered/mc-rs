//! Breaking a block: its drops, what was attached to it, and the edit itself.

use super::notify_edit;
use super::push_event;
use crate::block::blocks::Block;
use crate::block::direction::Direction;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::blocks::player_break_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::inventory::Hotbar;
use crate::item::tools::break_durability;
use crate::item::tools::can_harvest;
use crate::physics::BlockHit;
use crate::random::ItemRng;
use crate::rendering::particles::block::BlockParticles;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use bevy::prelude::*;

pub(super) fn apply_break(
    commands: &mut Commands,
    rng: &mut ItemRng,
    chunks: &mut WorldChunks,
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    ticks: &mut Option<ResMut<BlockTicks>>,
    particles: &mut Option<ResMut<BlockParticles>>,
    hotbar: &mut Hotbar,
    hit: BlockHit,
) {
    let metadata = chunks.metadata_at(hit.x, hit.y, hit.z);
    let mut attached = attached_blocks(chunks, hit);
    if let Some(plant) = chunks
        .block_at(hit.x, hit.y + 1, hit.z)
        .filter(|block| (*block).is_crossed_plant())
    {
        let plant_metadata = chunks.metadata_at(hit.x, hit.y + 1, hit.z);
        attached.push((hit.x, hit.y + 1, hit.z, plant, plant_metadata));
    }
    let furnace_drops = chunks
        .furnace_at(hit.x, hit.y, hit.z)
        .map(|furnace| furnace.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let chest_drops = chunks
        .chest_at(hit.x, hit.y, hit.z)
        .map(|chest| chest.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let dispenser_drops = chunks
        .dispenser_at(hit.x, hit.y, hit.z)
        .map(|dispenser| dispenser.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let light_edit = hit.block.is_torch()
        || matches!(hit.block, Block::RedstoneTorch | Block::UnlitRedstoneTorch)
        || hit.block == Block::LitFurnace
        || attached
            .iter()
            .any(|&(_, _, _, attached_block, _)| attached_block.is_torch());
    let tool = hotbar.selected_stack();
    // `canHarvestBlock` gates the harvest drop. The tool still takes durability
    // when the block comes out, including a block the tool cannot harvest.
    // TNT's player-destroy drop is not part of that gate.
    if break_block(chunks, hit) {
        for stack in furnace_drops {
            spawn_block_drop(commands, rng, IVec3::new(hit.x, hit.y, hit.z), stack);
        }
        spawn_chest_drops(commands, rng, IVec3::new(hit.x, hit.y, hit.z), chest_drops);
        spawn_chest_drops(
            commands,
            rng,
            IVec3::new(hit.x, hit.y, hit.z),
            dispenser_drops,
        );
        for stack in player_break_drops_with_metadata(hit.block, metadata, tool, rng) {
            spawn_block_drop(commands, rng, IVec3::new(hit.x, hit.y, hit.z), stack);
        }
        let position = IVec3::new(hit.x, hit.y, hit.z);
        push_event(
            ticks,
            BlockEvent::Changed {
                position,
                previous: hit.block,
                metadata,
            },
        );
        if can_harvest(tool, hit.block) {
            push_event(
                ticks,
                BlockEvent::Harvested {
                    position,
                    block: hit.block,
                    metadata,
                },
            );
        }
        for (x, y, z, attached_block, attached_metadata) in attached {
            for stack in natural_drops_with_metadata(attached_block, attached_metadata, rng) {
                spawn_block_drop(commands, rng, IVec3::new(x, y, z), stack);
            }
            push_event(
                ticks,
                BlockEvent::Changed {
                    position: IVec3::new(x, y, z),
                    previous: attached_block,
                    metadata: attached_metadata,
                },
            );
        }
        if let Some(particles) = particles.as_deref_mut() {
            particles.emit_break_state(hit, metadata);
        }
        notify_edit(streaming, persistence, hit.x, hit.y, hit.z, light_edit);
        if let Some(tool) = tool {
            let cost = break_durability(tool, hit.block);
            if cost > 0 {
                hotbar.damage_selected(cost);
            }
        }
    }
}

/// Torches and ladders hanging on the block `hit` targets, and a torch
/// standing on it, with their position and metadata.
pub(super) fn attached_blocks(
    chunks: &WorldChunks,
    hit: BlockHit,
) -> Vec<(i32, i32, i32, Block, u8)> {
    // A torch on top has no facing; one beside the block hangs on its side.
    let mut attached = Vec::new();
    let mut push = |dx: i32, dy: i32, dz: i32, block: Block, facing: Option<Direction>| {
        let (x, y, z) = (hit.x + dx, hit.y + dy, hit.z + dz);
        if chunks.block_at(x, y, z) != Some(block) {
            return;
        }
        let metadata = chunks.metadata_at(x, y, z);
        if block.facing(metadata) == facing {
            attached.push((x, y, z, block, metadata));
        }
    };
    push(0, 1, 0, Block::Torch, None);
    for side in Direction::ALL {
        // The torch or ladder at `side` of the block names the block as its
        // support, which lies on the opposite side of the attached cell.
        let [dx, dy, dz] = side.offset();
        let facing = match side {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
        };
        push(dx, dy, dz, Block::Torch, Some(facing));
        push(dx, dy, dz, Block::Ladder, Some(facing));
    }
    attached
}

/// Remove a targeted block. Bedrock and missing chunks are left unchanged.
pub fn break_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if !hit.block.is_breakable() {
        return false;
    }
    let attached = attached_blocks(chunks, hit);
    let broken = chunks
        .set_block(hit.x, hit.y, hit.z, Block::Air)
        .is_some_and(|previous| previous != Block::Air);
    if broken {
        for (x, y, z, _, _) in attached {
            chunks.set_block(x, y, z, Block::Air);
        }
        if chunks
            .block_at(hit.x, hit.y + 1, hit.z)
            .is_some_and(Block::is_crossed_plant)
        {
            chunks.set_block(hit.x, hit.y + 1, hit.z, Block::Air);
        }
    }
    broken
}

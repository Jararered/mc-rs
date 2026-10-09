//! What a held hoe, seed, or bucket does to the block it is used on.

use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::inventory::Hotbar;
use crate::item::tools::is_hoe;
use crate::physics::BLOCK_REACH;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::physics::raycast_blocks_or_liquid;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;
use bevy::prelude::*;

pub fn till_with_selected_hoe(
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    hit: BlockHit,
) -> bool {
    if !hotbar
        .selected_stack()
        .is_some_and(|stack| is_hoe(stack.item()))
    {
        return false;
    }
    if !till_block(chunks, hit) {
        return false;
    }
    hotbar.damage_selected(1);
    true
}

/// `ItemSeeds.onItemUse`: plant crops on the top face of farmland with air
/// above.
pub fn plant_seeds(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if hit.face != BlockFace::Up
        || chunks.block_at(hit.x, hit.y, hit.z) != Some(Block::Farmland)
        || chunks.block_at(hit.x, hit.y + 1, hit.z) != Some(Block::Air)
    {
        return false;
    }
    chunks
        .set_block(hit.x, hit.y + 1, hit.z, Block::Crops)
        .is_some()
}

pub fn till_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if chunks.block_at(hit.x, hit.y, hit.z) != Some(hit.block) {
        return false;
    }
    let can_till = match hit.block {
        Block::Dirt => true,
        Block::Grass => {
            hit.face != BlockFace::Down
                && chunks
                    .block_at(hit.x, hit.y + 1, hit.z)
                    .is_none_or(|block| block == Block::Air)
        }
        _ => false,
    };
    if !can_till {
        return false;
    }
    chunks
        .set_block(hit.x, hit.y, hit.z, Block::Farmland)
        .is_some_and(|previous| previous == hit.block)
}

/// `ItemBucket.onItemRightClick` when empty: pick up the water or lava
/// source the camera ray hits first. Unlike the normal block pick, this
/// raycast also stops on fluid so it can target one at all. Flowing
/// (non-source) fluid, a solid block, or nothing in reach leaves the bucket
/// empty, matching Beta's `getBlockMetadata(...) == 0` gate.
pub fn pick_up_fluid(
    chunks: &mut WorldChunks,
    origin: Vec3,
    direction: Vec3,
) -> Option<(i32, i32, i32, Block, Fluid)> {
    let hit = raycast_blocks_or_liquid(chunks, origin, direction, BLOCK_REACH)?;
    let fluid = Fluid::of(hit.block)?;
    if chunks.metadata_at(hit.x, hit.y, hit.z) != 0 {
        return None;
    }
    let previous = chunks.set_block(hit.x, hit.y, hit.z, Block::Air)?;
    Some((hit.x, hit.y, hit.z, previous, fluid))
}

/// `ItemBucket.onItemRightClick` when full: empty the held fluid into the
/// non-solid cell beside the hit face, the same target a torch would attach
/// to but without requiring a solid block behind it. Use the flowing block
/// value so `onBlockAdded` schedules its first spread tick.
pub fn place_fluid(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    fluid: Fluid,
) -> Option<(i32, i32, i32, Block, u8)> {
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return None;
    }
    let current = chunks.block_at(x, y, z)?;
    if current.is_solid_material() {
        return None;
    }
    let metadata = chunks.metadata_at(x, y, z);
    let previous = chunks.set_block(x, y, z, fluid.flowing())?;
    Some((x, y, z, previous, metadata))
}

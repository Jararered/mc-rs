//! `ItemBucket`: scooping a water or lava source, and pouring one back out.
//!
//! Beta has no `onItemUse` for buckets. Every bit of the behavior lives in
//! `onItemRightClick`, which raycasts for itself from the eye over
//! `getBlockReachDistance` and takes a `hitFluids` flag equal to
//! `isFull == 0`. Only `BlockFluid.canCollideCheck` reads that flag, and it
//! returns `hitFluids && metadata == 0`, so an empty bucket stops on a fluid
//! *source* while a filled one looks straight through liquids and hits the
//! block behind. That is why a filled bucket is aimed at a pool's floor and
//! an empty one at the pool itself.
//!
//! `isFull` is overloaded to carry the block a filled bucket places, so water
//! and lava are separate item ids rather than one item with a data value.
//!
//! Two branches of `ItemBucket` have no counterpart here and are left out on
//! purpose: the Nether `random.fizz` for a water bucket needs a dimension we
//! do not model, and filling a bucket from a cow needs mobs, which do not
//! exist yet.

use bevy::math::IVec3;
use bevy::prelude::Vec3;

use crate::block::fluids::Fluid;
use crate::block::id::Id;
use crate::block::properties::is_solid_material;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::physics::BLOCK_REACH;
use crate::physics::raycast_blocks;
use crate::physics::raycast_blocks_including_liquids;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;

/// A cell a bucket changed, and the stack the hand should hold afterwards.
///
/// Beta returns the replacement stack from `onItemRightClick` and
/// `PlayerController.sendUseItem` compares it by identity, so a bucket is
/// swapped for a different item rather than decremented.
pub struct BucketUse {
    pub position: IVec3,
    /// The block the cell held, so the caller can report the write to the
    /// block tick pass.
    pub previous: Id,
    pub previous_metadata: u8,
    pub result: ItemStack,
}

/// What a bucket holds, standing in for `ItemBucket.isFull`.
enum Contents {
    Empty,
    Full(Fluid),
}

fn contents(item: ItemId) -> Option<Contents> {
    match item {
        ItemId::Bucket => Some(Contents::Empty),
        ItemId::WaterBucket => Some(Contents::Full(Fluid::Water)),
        ItemId::LavaBucket => Some(Contents::Full(Fluid::Lava)),
        _ => None,
    }
}

fn bucket_stack(item: ItemId) -> ItemStack {
    ItemStack::new(item, 1).expect("the four bucket ids are registered with a stack size of one")
}

/// `ItemBucket.onItemRightClick`. `None` for a non-bucket stack, and for a
/// click the bucket cannot act on, in which case the hand keeps what it held.
pub fn use_bucket(
    chunks: &mut WorldChunks,
    origin: Vec3,
    direction: Vec3,
    stack: ItemStack,
) -> Option<BucketUse> {
    match contents(stack.item())? {
        Contents::Empty => scoop(chunks, origin, direction),
        Contents::Full(fluid) => pour(chunks, origin, direction, fluid),
    }
}

/// `ItemBucket` with `isFull == 0`: take a source out of the world and
/// replace the hand with the matching filled bucket.
///
/// The ray already stopped on a source, so `Fluid::of` is the whole of
/// Beta's `getBlockMaterial == water || getBlockMetadata == 0` test, and a
/// solid block hit simply is not a fluid.
fn scoop(chunks: &mut WorldChunks, origin: Vec3, direction: Vec3) -> Option<BucketUse> {
    let hit = raycast_blocks_including_liquids(chunks, origin, direction, BLOCK_REACH)?;
    let fluid = Fluid::of(hit.block)?;
    let (x, y, z) = (hit.x, hit.y, hit.z);
    let previous = chunks.block_at(x, y, z)?;
    let previous_metadata = chunks.metadata_at(x, y, z);
    chunks.set_block(x, y, z, Id::Air)?;
    Some(BucketUse {
        position: IVec3::new(x, y, z),
        previous,
        previous_metadata,
        result: bucket_stack(match fluid {
            Fluid::Water => ItemId::WaterBucket,
            Fluid::Lava => ItemId::LavaBucket,
        }),
    })
}

/// `ItemBucket` with `isFull > 0`: fill the cell the ray would enter next.
///
/// Beta writes the *flowing* id at metadata zero rather than the still id,
/// and `BlockFlowing.onBlockAdded` schedules a tick that hardens it. Our
/// `Flowing` behavior does the same, and it is also the only route to the
/// first spread tick: `Stationary.onAdded` schedules nothing, so pouring in
/// `Id::Water` directly would leave a pool that never flows.
fn pour(
    chunks: &mut WorldChunks,
    origin: Vec3,
    direction: Vec3,
    fluid: Fluid,
) -> Option<BucketUse> {
    // `hitLiquids` is false for a filled bucket, so the ray sees through the
    // pool and stops on the block behind it.
    let hit = raycast_blocks(chunks, origin, direction, BLOCK_REACH)?;
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return None;
    }
    let previous = chunks.block_at(x, y, z)?;
    // `isAirBlock || !getBlockMaterial(...).isSolid()`. Liquids, plants, and
    // torches are all replaceable this way.
    if previous != Id::Air && is_solid_material(previous) {
        return None;
    }
    let previous_metadata = chunks.metadata_at(x, y, z);
    // Beta ignores what `setBlockAndMetadataWithNotify` returns and consumes
    // the bucket either way, so pouring into a cell that already holds this
    // exact fluid still empties the bucket and changes nothing.
    chunks.set_block(x, y, z, fluid.flowing())?;
    Some(BucketUse {
        position: IVec3::new(x, y, z),
        previous,
        previous_metadata,
        result: bucket_stack(ItemId::Bucket),
    })
}

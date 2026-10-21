//! Normal and sticky pistons. Face ordering follows PistonBlockTextures.
//!
//! A move is Beta's: the cells it touches become `MovingPiston` blocks whose
//! [`MovingPiston`] state slides the blocks in over two ticks
//! (`TileEntityPiston`, ticked by `TickWorld::tick_moving_pistons`).
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use crate::world::moving_piston::MovingPiston;
use crate::world::sound::Pitch;
use bevy::math::IVec3;
use bevy::math::Vec3;

pub struct Piston;
pub static PISTON: Piston = Piston;
pub struct PistonHead;
pub static HEAD: PistonHead = PistonHead;

const OFFSETS: [IVec3; 6] = [
    IVec3::NEG_Y,
    IVec3::Y,
    IVec3::NEG_Z,
    IVec3::Z,
    IVec3::NEG_X,
    IVec3::X,
];
/// Metadata bit of an extended base, and of a sticky head.
const EXTENDED: u8 = 8;
/// Beta's limit on the blocks one piston pushes.
const MAX_PUSHED: usize = 12;

fn facing(meta: u8) -> Option<usize> {
    ((meta & 7) < 6).then_some((meta & 7) as usize)
}

fn is_base(block: Block) -> bool {
    matches!(block, Block::Piston | Block::StickyPiston)
}

fn is_powered(world: &mut TickWorld, pos: IVec3, face: usize) -> bool {
    for (side, offset) in OFFSETS.into_iter().enumerate() {
        if side != face && world.block_indirectly_providing_power_to(pos + offset, side as u8) {
            return true;
        }
    }
    // Beta 1.7.3's quasi-connectivity above and around the piston.
    if world.block_indirectly_providing_power_to(pos, 0) {
        return true;
    }
    let top = pos + IVec3::Y;
    if world.block_indirectly_providing_power_to(top + IVec3::Y, 1) {
        return true;
    }
    for (side, offset) in OFFSETS.into_iter().enumerate().skip(2) {
        if world.block_indirectly_providing_power_to(top + offset, side as u8) {
            return true;
        }
    }
    false
}

/// `Block.getMobilityFlag`: 0 is pushed, 1 is broken by the push and cannot
/// be pulled, 2 never moves. Beta takes it from the material (plants,
/// circuits, liquids, leaves, fire, snow, cactus, pumpkin, cake, web) with
/// overrides for beds, doors, and plates; rails override theirs back to 0.
fn mobility(block: Block) -> u8 {
    match block {
        Block::Bedrock
        | Block::Obsidian
        | Block::NetherPortal
        | Block::PistonHead
        | Block::MovingPiston => 2,
        Block::Torch
        | Block::RedstoneTorchActive
        | Block::RedstoneTorchInactive
        | Block::Redstone
        | Block::RepeaterInactive
        | Block::RepeaterActive
        | Block::Lever
        | Block::StoneButton
        | Block::Ladder
        | Block::StonePressurePlate
        | Block::WoodenPressurePlate
        | Block::WoodenDoor
        | Block::IronDoor
        | Block::Bed
        | Block::Sapling
        | Block::TallGrass
        | Block::DeadBush
        | Block::Dandelion
        | Block::Rose
        | Block::BrownMushroom
        | Block::RedMushroom
        | Block::Crops
        | Block::SugarCane
        | Block::Water
        | Block::WaterFlowing
        | Block::Lava
        | Block::LavaFlowing
        | Block::Leaves
        | Block::Fire
        | Block::SnowLayer
        | Block::Cactus
        | Block::Pumpkin
        | Block::JackOLantern
        | Block::Cake
        | Block::Cobweb => 1,
        _ => 0,
    }
}

/// `BlockPistonBase.canPushBlock` without its mobility 1 case, which the
/// callers decide: a push breaks such a block and a pull leaves it.
fn can_push(world: &TickWorld, pos: IVec3, block: Block) -> bool {
    if !world.is_loaded(pos)
        || mobility(block) == 2
        // Blocks with a tile entity stay put.
        || matches!(
            block,
            Block::Chest
                | Block::Dispenser
                | Block::FurnaceInactive
                | Block::FurnaceActive
                | Block::NoteBlock
                | Block::MobSpawner
                | Block::Jukebox
                | Block::Sign
                | Block::WallSign
        )
    {
        return false;
    }
    !(is_base(block) && world.metadata(pos) & EXTENDED != 0)
}

/// What an extension moves, from `BlockPistonBase.canExtend`.
struct Push {
    /// A block the push breaks at the far end.
    broken: Option<(IVec3, Block, u8)>,
    /// The cell past the last pushed block.
    end: IVec3,
}

fn plan_push(world: &TickWorld, pos: IVec3, dir: IVec3) -> Option<Push> {
    let mut pushed = 0;
    let mut next = pos + dir;
    loop {
        if !(1..127).contains(&next.y) || !world.is_loaded(next) {
            return None;
        }
        let block = world.block(next);
        if block == Block::Air {
            return Some(Push {
                broken: None,
                end: next,
            });
        }
        if !can_push(world, next, block) {
            return None;
        }
        if mobility(block) == 1 {
            return Some(Push {
                broken: Some((next, block, world.metadata(next))),
                end: next,
            });
        }
        if pushed == MAX_PUSHED {
            return None;
        }
        pushed += 1;
        next += dir;
    }
}

/// `BlockPistonBase.tryExtend`: break the end block, then turn every cell
/// from the end back to the piston into a block on its way out.
fn try_extend(world: &mut TickWorld, pos: IVec3, dir: IVec3, face: usize, sticky: bool) -> bool {
    let Some(push) = plan_push(world, pos, dir) else {
        return false;
    };
    if let Some((cell, block, metadata)) = push.broken {
        world.drop_block_as_item(cell, block, metadata);
        world.set_block_notify(cell, Block::Air);
    }
    let facing = face as u8;
    let mut cell = push.end;
    while cell != pos {
        let behind = cell - dir;
        let moving = if behind == pos {
            let head = facing | if sticky { EXTENDED } else { 0 };
            MovingPiston::new(Block::PistonHead, head, facing, true, false)
        } else {
            MovingPiston::new(
                world.block(behind),
                world.metadata(behind),
                facing,
                true,
                false,
            )
        };
        world.place_moving_piston(cell, moving);
        cell = behind;
    }
    true
}

/// `BlockPistonBase.playBlock`'s retraction: the base and a sticky
/// piston's block slide back, and the head goes.
fn retract(world: &mut TickWorld, pos: IVec3, block: Block, dir: IVec3, face: usize) {
    let sticky = block == Block::StickyPiston;
    let facing = face as u8;
    let head = pos + dir;
    world.clear_moving_piston(head);
    world.place_moving_piston(pos, MovingPiston::new(block, facing, facing, false, true));
    if !sticky {
        world.set_ignore_piston_updates(sticky, false);
        world.set_block_notify(head, Block::Air);
        world.set_ignore_piston_updates(sticky, true);
        return;
    }
    let pulled = head + dir;
    let mut stored = world.block(pulled);
    let mut metadata = world.metadata(pulled);
    // A block this piston is still pushing out lands where it is.
    let mut landed = false;
    if stored == Block::MovingPiston
        && let Some(moving) = world.moving_piston(pulled)
        && moving.facing == facing
        && moving.extending
    {
        world.clear_moving_piston(pulled);
        stored = moving.block;
        metadata = moving.metadata;
        landed = true;
    }
    if landed
        || stored == Block::Air
        || !can_push(world, pulled, stored)
        || (mobility(stored) != 0 && !is_base(stored))
    {
        if !landed {
            world.set_ignore_piston_updates(sticky, false);
            world.set_block_notify(head, Block::Air);
            world.set_ignore_piston_updates(sticky, true);
        }
    } else {
        world.set_ignore_piston_updates(sticky, false);
        world.set_block_notify(pulled, Block::Air);
        world.set_ignore_piston_updates(sticky, true);
        world.place_moving_piston(
            head,
            MovingPiston::new(stored, metadata, facing, false, false),
        );
    }
}

/// `BlockPistonBase.playBlock`'s sound: `nextFloat() * span + 0.6F`.
fn thump(world: &mut TickWorld, pos: IVec3, name: &'static str, span: f32) {
    world.play_sound(
        name,
        pos.as_vec3() + Vec3::splat(0.5),
        0.5,
        Pitch::Up(0.6, span),
    );
}

/// `World.playNoteAt` and `BlockPistonBase.playBlock`: 0 extends, 1
/// retracts.
fn play_block(world: &mut TickWorld, pos: IVec3, block: Block, action: u8, face: usize) {
    let sticky = block == Block::StickyPiston;
    let dir = OFFSETS[face];
    world.emit(TickEffect::PistonPlayed {
        position: pos,
        action,
        facing: face as u8,
    });
    world.set_ignore_piston_updates(sticky, true);
    if action == 0 {
        if try_extend(world, pos, dir, face, sticky) {
            world.set_metadata_notify(pos, face as u8 | EXTENDED);
            thump(world, pos, "tile.piston.out", 0.25);
        }
    } else {
        retract(world, pos, block, dir, face);
        thump(world, pos, "tile.piston.in", 0.15);
    }
    world.set_ignore_piston_updates(sticky, false);
}

/// `BlockPistonBase.updatePistonState`.
fn update_state(world: &mut TickWorld, pos: IVec3) {
    let block = world.block(pos);
    let meta = world.metadata(pos);
    let Some(face) = facing(meta) else {
        return;
    };
    let powered = is_powered(world, pos, face);
    if powered && meta & EXTENDED == 0 {
        if plan_push(world, pos, OFFSETS[face]).is_some() {
            world.set_metadata(pos, face as u8 | EXTENDED);
            play_block(world, pos, block, 0, face);
        }
    } else if !powered && meta & EXTENDED != 0 {
        world.set_metadata(pos, face as u8);
        play_block(world, pos, block, 1, face);
    }
}

impl BlockBehavior for Piston {
    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        if world.moving_piston(pos).is_none() {
            update_state(world, pos);
        }
    }
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, _: Block) {
        if !world.ignores_piston_updates(world.block(pos) == Block::StickyPiston) {
            update_state(world, pos);
        }
    }
    fn on_removed(&self, world: &mut TickWorld, pos: IVec3, _: Block, metadata: u8) {
        if metadata & EXTENDED != 0
            && let Some(face) = facing(metadata)
        {
            let head = pos + OFFSETS[face];
            if world.block(head) == Block::PistonHead {
                world.set_block_notify(head, Block::Air);
            }
        }
    }
}
impl BlockBehavior for PistonHead {
    /// `BlockPistonExtension.onNeighborBlockChange`: a head without its base
    /// goes, and otherwise the base hears about the change.
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, neighbor: Block) {
        let Some(face) = facing(world.metadata(pos)) else {
            return;
        };
        let base = pos - OFFSETS[face];
        if is_base(world.block(base)) {
            PISTON.neighbor_changed(world, base, neighbor);
        } else {
            world.set_block_notify(pos, Block::Air);
        }
    }
    /// `BlockPistonExtension.onBlockRemoval`: breaking the head takes the
    /// extended base with it, which drops as a piston.
    fn on_removed(&self, world: &mut TickWorld, pos: IVec3, _: Block, metadata: u8) {
        if let Some(face) = facing(metadata) {
            let base = pos - OFFSETS[face];
            let block = world.block(base);
            let base_metadata = world.metadata(base);
            if is_base(block) && base_metadata & EXTENDED != 0 {
                world.drop_block_as_item(base, block, base_metadata);
                world.set_block_notify(base, Block::Air);
            }
        }
    }
}

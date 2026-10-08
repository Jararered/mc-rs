//! Normal and sticky pistons. Face ordering follows PistonBlockTextures.
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

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
        | Block::RedstoneTorch
        | Block::UnlitRedstoneTorch
        | Block::RedstoneWire
        | Block::Repeater
        | Block::PoweredRepeater
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
        | Block::FlowingWater
        | Block::Lava
        | Block::FlowingLava
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
                | Block::Furnace
                | Block::LitFurnace
                | Block::NoteBlock
                | Block::MobSpawner
                | Block::Jukebox
                | Block::StandingSign
                | Block::WallSign
        )
    {
        return false;
    }
    !(is_base(block) && world.metadata(pos) & EXTENDED != 0)
}

/// What an extension moves, from `BlockPistonBase.canExtend`.
struct Push {
    /// Pushed blocks, nearest the piston first.
    blocks: Vec<(IVec3, Block, u8)>,
    /// A block the push breaks at the far end.
    broken: Option<(IVec3, Block, u8)>,
    /// The cell past the last pushed block.
    end: IVec3,
}

fn plan_push(world: &TickWorld, pos: IVec3, dir: IVec3) -> Option<Push> {
    let mut blocks = Vec::new();
    let mut next = pos + dir;
    loop {
        if !(1..127).contains(&next.y) || !world.is_loaded(next) {
            return None;
        }
        let block = world.block(next);
        if block == Block::Air {
            return Some(Push {
                blocks,
                broken: None,
                end: next,
            });
        }
        if !can_push(world, next, block) {
            return None;
        }
        if mobility(block) == 1 {
            return Some(Push {
                blocks,
                broken: Some((next, block, world.metadata(next))),
                end: next,
            });
        }
        if blocks.len() == MAX_PUSHED {
            return None;
        }
        blocks.push((next, block, world.metadata(next)));
        next += dir;
    }
}

fn extend(world: &mut TickWorld, pos: IVec3, dir: IVec3, head: u8, push: Push) {
    if let Some((cell, block, metadata)) = push.broken {
        world.drop_block_as_item(cell, block, metadata);
        world.set_block_notify(cell, Block::Air);
    }
    for &(from, block, metadata) in push.blocks.iter().rev() {
        world.set_block_and_metadata_notify(from + dir, block, metadata);
        world.set_block_notify(from, Block::Air);
    }
    world.set_block_and_metadata_notify(pos + dir, Block::PistonHead, head);
    // The farthest newly occupied cell was air (or was destroyed). Bodies
    // standing in it must be displaced rather than embedded in the block.
    world.piston_push(push.end, dir);
}

fn retract(world: &mut TickWorld, pos: IVec3, dir: IVec3, sticky: bool) {
    let head = pos + dir;
    if world.block(head) == Block::PistonHead {
        world.set_block_notify(head, Block::Air);
    }
    if sticky {
        let pulled = head + dir;
        let block = world.block(pulled);
        if block != Block::Air && can_push(world, pulled, block) && mobility(block) == 0 {
            let metadata = world.metadata(pulled);
            world.set_block_and_metadata_notify(head, block, metadata);
            world.set_block_notify(pulled, Block::Air);
        }
    }
}

/// `BlockPistonBase.updatePistonState`. Beta marks the base first and then
/// moves the blocks over a few ticks, ignoring neighbor changes while it
/// starts the move. The move is immediate here, so a change that arrives in
/// the middle of any piston's move is looked at on the next tick instead.
fn update_state(world: &mut TickWorld, pos: IVec3) {
    let block = world.block(pos);
    if world.piston_moving() {
        world.schedule(pos, block, 1);
        return;
    }
    let meta = world.metadata(pos);
    let Some(face) = facing(meta) else {
        return;
    };
    let dir = OFFSETS[face];
    let sticky = block == Block::StickyPiston;
    let powered = is_powered(world, pos, face);
    if powered && meta & EXTENDED == 0 {
        let Some(push) = plan_push(world, pos, dir) else {
            return;
        };
        world.set_metadata(pos, meta | EXTENDED);
        world.begin_piston_move();
        let head = face as u8 | if sticky { EXTENDED } else { 0 };
        extend(world, pos, dir, head, push);
        world.end_piston_move();
        world.set_metadata_notify(pos, meta | EXTENDED);
    } else if !powered && meta & EXTENDED != 0 {
        world.set_metadata(pos, meta & 7);
        world.begin_piston_move();
        retract(world, pos, dir, sticky);
        world.end_piston_move();
        world.set_metadata_notify(pos, meta & 7);
    }
}

impl BlockBehavior for Piston {
    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        update_state(world, pos);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, _: Block) {
        update_state(world, pos);
    }
    /// Only scheduled by [`update_state`], for a change put off during a move.
    fn update_tick(&self, world: &mut TickWorld, pos: IVec3) {
        update_state(world, pos);
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

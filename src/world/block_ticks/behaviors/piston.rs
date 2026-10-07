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
fn facing(meta: u8) -> Option<usize> {
    ((meta & 7) < 6).then_some((meta & 7) as usize)
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

fn mobility(block: Block) -> u8 {
    if matches!(
        block,
        Block::Bedrock
            | Block::Obsidian
            | Block::NetherPortal
            | Block::PistonHead
            | Block::MovingPiston
    ) {
        return 2;
    }
    if matches!(
        block,
        Block::Torch
            | Block::RedstoneTorch
            | Block::UnlitRedstoneTorch
            | Block::RedstoneWire
            | Block::Repeater
            | Block::PoweredRepeater
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::StoneButton
            | Block::WoodenDoor
            | Block::IronDoor
            | Block::Trapdoor
            | Block::Dandelion
            | Block::Rose
    ) {
        return 1;
    }
    0
}
fn can_push(world: &TickWorld, pos: IVec3, block: Block) -> bool {
    if !world.is_loaded(pos)
        || mobility(block) == 2
        || matches!(
            block,
            Block::Chest | Block::Dispenser | Block::Furnace | Block::LitFurnace | Block::NoteBlock
        )
    {
        return false;
    }
    if matches!(block, Block::Piston | Block::StickyPiston) && world.metadata(pos) & 8 != 0 {
        return false;
    }
    true
}
fn extend(world: &mut TickWorld, pos: IVec3, dir: IVec3, face: u8) -> bool {
    let mut pushed = Vec::new();
    let mut next = pos + dir;
    for _ in 0..=12 {
        if !(1..127).contains(&next.y) || !world.is_loaded(next) {
            return false;
        }
        let block = world.block(next);
        if block == Block::Air {
            break;
        }
        if !can_push(world, next, block) {
            return false;
        }
        if mobility(block) == 1 {
            world.drop_block_as_item(next, block, world.metadata(next));
            world.set_block_notify(next, Block::Air);
            break;
        }
        if pushed.len() == 12 {
            return false;
        }
        pushed.push((next, block, world.metadata(next)));
        next += dir;
    }
    for &(from, block, metadata) in pushed.iter().rev() {
        world.set_block_and_metadata_notify(from + dir, block, metadata);
        world.set_block_notify(from, Block::Air);
    }
    world.set_block_and_metadata_notify(pos + dir, Block::PistonHead, face);
    // The farthest newly occupied cell was air (or was destroyed). Bodies
    // standing in it must be displaced rather than embedded in the block.
    world.piston_push(next, dir);
    true
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
impl BlockBehavior for Piston {
    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        self.neighbor_changed(world, pos, Block::Air);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, _: Block) {
        let block = world.block(pos);
        let meta = world.metadata(pos);
        let Some(face) = facing(meta) else {
            return;
        };
        let powered = is_powered(world, pos, face);
        if powered && meta & 8 == 0 {
            // Set the base state before notifying neighbors of moving blocks.
            // The extension is committed only if its whole destination is loaded.
            if extend(world, pos, OFFSETS[face], face as u8) {
                world.set_metadata_notify(pos, meta | 8);
            }
        } else if !powered && meta & 8 != 0 {
            world.set_metadata_notify(pos, meta & 7);
            retract(world, pos, OFFSETS[face], block == Block::StickyPiston);
        }
    }
    fn on_removed(&self, world: &mut TickWorld, pos: IVec3, _: Block, metadata: u8) {
        if metadata & 8 != 0
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
    fn on_removed(&self, world: &mut TickWorld, pos: IVec3, _: Block, metadata: u8) {
        if let Some(face) = facing(metadata) {
            let base = pos - OFFSETS[face];
            if matches!(world.block(base), Block::Piston | Block::StickyPiston)
                && world.metadata(base) & 8 != 0
            {
                world.set_block_notify(base, Block::Air);
            }
        }
    }
}

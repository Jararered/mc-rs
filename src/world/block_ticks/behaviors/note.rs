//! Beta BlockNote and TileEntityNote: pitch cycles 0..24 and rising edges play.
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Note;
pub static NOTE: Note = Note;

/// `BlockNote.blockEventReceived`: the instrument follows the material of the
/// block underneath. 1 bass drum, 2 snare, 3 click, 4 bass guitar.
fn instrument(block: Block) -> u8 {
    match block {
        Block::Stone
        | Block::Cobblestone
        | Block::MossyCobblestone
        | Block::Bedrock
        | Block::GoldOre
        | Block::IronOre
        | Block::CoalOre
        | Block::LapisOre
        | Block::LapisBlock
        | Block::DiamondOre
        | Block::RedstoneOre
        | Block::LitRedstoneOre
        | Block::Dispenser
        | Block::Sandstone
        | Block::DoubleStoneSlab
        | Block::StoneSlab
        | Block::Bricks
        | Block::Obsidian
        | Block::MobSpawner
        | Block::CobblestoneStairs
        | Block::Furnace
        | Block::LitFurnace
        | Block::Netherrack => 1,
        Block::Sand | Block::Gravel | Block::SoulSand => 2,
        Block::Glass | Block::Glowstone => 3,
        Block::WoodenPlanks
        | Block::Wood
        | Block::Bookshelf
        | Block::NoteBlock
        | Block::Jukebox
        | Block::Chest
        | Block::LockedChest
        | Block::CraftingTable
        | Block::Fence
        | Block::WoodenStairs
        | Block::WoodenDoor
        | Block::Trapdoor
        | Block::StandingSign
        | Block::WallSign => 4,
        _ => 0,
    }
}
fn play(world: &mut TickWorld, pos: IVec3) {
    if world.block(pos + IVec3::Y) != Block::Air {
        return;
    }
    let pitch = world.note_mut(pos).map(|note| note.pitch);
    if let Some(pitch) = pitch {
        world.emit(TickEffect::Note {
            position: pos,
            instrument: instrument(world.block(pos - IVec3::Y)),
            pitch,
        });
    }
}
impl BlockBehavior for Note {
    fn activated(&self, world: &mut TickWorld, pos: IVec3) {
        if let Some(note) = world.note_mut(pos) {
            note.pitch = (note.pitch + 1) % 25;
            world.mark_state_dirty(pos);
            play(world, pos);
        }
    }
    fn clicked(&self, world: &mut TickWorld, pos: IVec3) {
        play(world, pos);
    }
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, neighbor: Block) {
        if !super::super::behavior::behavior(neighbor).can_provide_power() {
            return;
        }
        let powered = world.block_getting_powered(pos);
        if let Some(note) = world.note_mut(pos) {
            if note.previous_powered != powered {
                note.previous_powered = powered;
                world.mark_state_dirty(pos);
                if powered {
                    play(world, pos);
                }
            }
        }
    }
}

//! Beta BlockNote and TileEntityNote: pitch cycles 0..24 and rising edges play.
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Note;
pub static NOTE: Note = Note;

fn instrument(block: Id) -> u8 {
    match block {
        Id::Stone | Id::Cobblestone | Id::Sandstone | Id::StoneSlab | Id::Bricks => 1,
        Id::Sand | Id::Gravel => 2,
        Id::Glass => 3,
        Id::Wood | Id::WoodenPlanks | Id::SpruceWood | Id::BirchWood => 4,
        _ => 0,
    }
}
fn play(world: &mut TickWorld, pos: IVec3) {
    if world.block(pos + IVec3::Y) != Id::Air {
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
    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, neighbor: Id) {
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

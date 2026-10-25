//! Beta `BlockJukeBox` and `TileEntityRecordPlayer`: a jukebox holds one
//! disc, and metadata 1 says that it does. `ItemRecord.onItemUse` puts the
//! disc in (`player::interaction::editing`); a right-click or the block's
//! removal throws it out again and stops the music.
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

/// `World.playAuxSFX`'s number for a record starting, or stopping when its
/// data is zero.
pub const RECORD_EFFECT: i32 = 1005;

pub struct Jukebox;

/// `BlockJukeBox.func_28038_b_`.
fn eject(world: &mut TickWorld, position: IVec3) {
    let Some(record) = world.take_record(position) else {
        return;
    };
    world.emit(TickEffect::Aux {
        position,
        effect: RECORD_EFFECT,
        data: 0,
    });
    if world.block(position) == Block::Jukebox {
        world.set_metadata_notify(position, 0);
    }
    world.emit(TickEffect::EjectRecord { position, record });
}

impl BlockBehavior for Jukebox {
    fn activated(&self, world: &mut TickWorld, position: IVec3) {
        if world.metadata(position) != 0 {
            eject(world, position);
        }
    }

    fn on_removed(&self, world: &mut TickWorld, position: IVec3, _: Block, _: u8) {
        eject(world, position);
    }
}

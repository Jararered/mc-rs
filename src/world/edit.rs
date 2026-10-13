//! Writing blocks from outside the tick pass.
//!
//! A write to [`WorldChunks`] that the block tick pass did not make has three
//! things to report: the hooks and neighbor updates ([`BlockTicks`]), the
//! relight and remesh ([`WorldStreaming`]) and the chunk's save
//! ([`WorldPersistence`]). [`WorldEdit`] carries all four so a system takes
//! one parameter and reports a write with one call.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::block::blocks::Block;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;

/// The world's blocks and everything a write to them has to tell. Only
/// `chunks` has to exist: a headless world without ticks, streaming or
/// persistence reports to whichever of them it has.
#[derive(SystemParam)]
pub struct WorldEdit<'w> {
    pub chunks: ResMut<'w, WorldChunks>,
    pub ticks: Option<ResMut<'w, BlockTicks>>,
    pub streaming: Option<ResMut<'w, WorldStreaming>>,
    pub persistence: Option<ResMut<'w, WorldPersistence>>,
}

impl WorldEdit<'_> {
    /// Report a block already written at `position`, which held `previous`
    /// with `previous_metadata`: its hooks run with the next tick pass, its
    /// light and mesh are rebuilt, and its chunk is saved.
    pub fn changed(&mut self, position: IVec3, previous: Block, previous_metadata: u8) {
        if let Some(ticks) = self.ticks.as_deref_mut() {
            ticks.block_changed(position, previous, previous_metadata);
        }
        self.notify(position.x, position.y, position.z, false);
    }

    /// Queue a block event for the next tick pass.
    pub fn push_event(&mut self, event: BlockEvent) {
        if let Some(ticks) = self.ticks.as_deref_mut() {
            ticks.push_event(event);
        }
    }

    /// Remesh and save around a written block, without running its hooks.
    /// `light_edit` also saves the chunks a light at the block can reach
    /// across a border.
    pub fn notify(&mut self, x: i32, y: i32, z: i32, light_edit: bool) {
        if let Some(persistence) = self.persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPosition::from_block(x, z));
            if light_edit {
                // A detached wall torch may belong to the neighboring chunk.
                for position in remesh_chunks_touching(x, z) {
                    persistence.mark_dirty(position);
                }
            }
        }
        // Relighting decides which sections actually changed, so torches and
        // plain blocks take the same path.
        if let Some(streaming) = self.streaming.as_deref_mut() {
            streaming.request_block_update(x, y, z);
        }
    }

    /// Save `position` with the next drain.
    pub fn mark_dirty(&mut self, position: ChunkPosition) {
        if let Some(persistence) = self.persistence.as_deref_mut() {
            persistence.mark_dirty(position);
        }
    }
}

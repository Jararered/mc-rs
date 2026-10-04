//! Beta's scheduled tick list: `World.scheduledTickTreeSet` ordered by due
//! time and then scheduling order, with `scheduledTickSet` rejecting a second
//! entry for the same cell and block.

use std::collections::BTreeMap;
use std::collections::HashSet;

use bevy::math::IVec3;

use crate::block::blocks::Block;
use crate::world::chunk::ChunkPosition;

/// One `NextTickListEntry`: run `block`'s update at `position` once the world
/// time reaches the entry's due time, if the cell still holds `block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledTick {
    pub position: IVec3,
    pub block: Block,
    pub due: u64,
}

#[derive(Default)]
pub struct TickScheduler {
    /// Keyed by `(due, sequence)`, matching `NextTickListEntry.comparer`.
    queue: BTreeMap<(u64, u64), (IVec3, Block)>,
    /// Entries by identity. Beta's `NextTickListEntry.equals` compares only
    /// the cell and block, so a pending tick is never scheduled twice.
    pending: HashSet<(IVec3, Block)>,
    next_sequence: u64,
}

impl TickScheduler {
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn contains(&self, position: IVec3, block: Block) -> bool {
        self.pending.contains(&(position, block))
    }

    /// Add an entry unless the same cell and block is already pending, which
    /// keeps the earlier entry and its due time, as Beta does.
    pub fn schedule(&mut self, position: IVec3, block: Block, due: u64) -> bool {
        if !self.pending.insert((position, block)) {
            return false;
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.queue.insert((due, sequence), (position, block));
        true
    }

    /// Remove and return the earliest entry if it is due at `time`.
    pub fn pop_due(&mut self, time: u64) -> Option<ScheduledTick> {
        let entry = self.queue.first_entry()?;
        let (due, _) = *entry.key();
        if due > time {
            return None;
        }
        let (position, block) = entry.remove();
        self.pending.remove(&(position, block));
        Some(ScheduledTick {
            position,
            block,
            due,
        })
    }

    /// Remove every entry inside `chunk`, in due order.
    pub fn take_chunk(&mut self, chunk: ChunkPosition) -> Vec<ScheduledTick> {
        let mut taken = Vec::new();
        self.queue.retain(|&(due, _), &mut (position, block)| {
            if ChunkPosition::from_block(position.x, position.z) != chunk {
                return true;
            }
            taken.push(ScheduledTick {
                position,
                block,
                due,
            });
            false
        });
        for tick in &taken {
            self.pending.remove(&(tick.position, tick.block));
        }
        taken
    }

    /// Every pending entry in due order.
    pub fn iter(&self) -> impl Iterator<Item = ScheduledTick> + '_ {
        self.queue
            .iter()
            .map(|(&(due, _), &(position, block))| ScheduledTick {
                position,
                block,
                due,
            })
    }

    /// Keep remaining delays when a command moves the world clock. Retain
    /// sequence keys so entries that become due together keep their order.
    pub fn rebase_time(&mut self, previous: u64, time: u64) {
        self.queue = std::mem::take(&mut self.queue)
            .into_iter()
            .map(|((due, sequence), entry)| {
                (
                    (time.saturating_add(due.saturating_sub(previous)), sequence),
                    entry,
                )
            })
            .collect();
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.pending.clear();
    }
}

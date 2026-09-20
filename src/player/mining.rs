//! Survival punching, matching Beta `PlayerControllerSP` with an empty hand.

use crate::physics::BlockHit;
use crate::world::block::block::BlockId;
use crate::world::block::properties::hand_mine_progress_per_tick;
use crate::world::block::properties::is_breakable;

/// Ticks after a break before mining can start again (`blockHitWait`).
const BLOCK_HIT_WAIT_TICKS: i32 = 5;

/// Held-button mining progress against one block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MiningState {
    target: Option<(i32, i32, i32)>,
    damage: f32,
    wait: i32,
}

impl MiningState {
    pub fn damage(&self) -> f32 {
        self.damage
    }

    pub fn reset(&mut self) {
        self.target = None;
        self.damage = 0.0;
        self.wait = 0;
    }

    /// Instant break on click when `blockStrength >= 1`, as in `clickBlock`.
    pub fn try_instant(
        &mut self,
        hit: BlockHit,
        on_ground: bool,
        in_water: bool,
    ) -> Option<BlockHit> {
        if !is_breakable(hit.block) {
            return None;
        }
        if hand_mine_progress_per_tick(hit.block, on_ground, in_water) < 1.0 {
            return None;
        }
        self.reset();
        self.wait = BLOCK_HIT_WAIT_TICKS;
        Some(hit)
    }

    /// One 20 Hz tick of `sendBlockRemoving`. Returns the block to remove.
    pub fn tick(
        &mut self,
        looked_at: Option<BlockHit>,
        on_ground: bool,
        in_water: bool,
    ) -> Option<BlockHit> {
        if self.wait > 0 {
            self.wait -= 1;
            return None;
        }
        let Some(hit) = looked_at else {
            self.target = None;
            self.damage = 0.0;
            return None;
        };
        if !is_breakable(hit.block) {
            self.target = None;
            self.damage = 0.0;
            return None;
        }
        let pos = (hit.x, hit.y, hit.z);
        if self.target != Some(pos) {
            self.target = Some(pos);
            self.damage = 0.0;
            return None;
        }
        let step = hand_mine_progress_per_tick(hit.block, on_ground, in_water);
        if step <= 0.0 {
            return None;
        }
        self.damage += step;
        if self.damage >= 1.0 {
            self.target = None;
            self.damage = 0.0;
            self.wait = BLOCK_HIT_WAIT_TICKS;
            Some(hit)
        } else {
            None
        }
    }
}

/// Ticks of punching needed to break `block` by hand, or `None` if it cannot.
pub fn hand_ticks_to_break(block: BlockId, on_ground: bool, in_water: bool) -> Option<u32> {
    if !is_breakable(block) {
        return None;
    }
    let step = hand_mine_progress_per_tick(block, on_ground, in_water);
    if !step.is_finite() || step >= 1.0 {
        return Some(1);
    }
    if step <= 0.0 {
        return None;
    }
    Some((1.0 / step).ceil() as u32)
}

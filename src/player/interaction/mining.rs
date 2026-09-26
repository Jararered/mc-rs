//! Survival mining, matching Beta `PlayerControllerSP`.
//!
//! The held stack is sampled every tick. Switching tools changes the rate and
//! does not reset progress. Looking at a different block does.

use crate::block::id::BlockId;
use crate::block::properties::is_breakable;
use crate::item::ItemStack;
use crate::item::tools::mine_step;
use crate::item::tools::ticks_to_break;
use crate::physics::BlockHit;

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

    pub fn target(&self) -> Option<(i32, i32, i32)> {
        self.target
    }

    /// Destroy-stage tile 0–9 from current damage, or `None` when not mining.
    pub fn destroy_stage(&self) -> Option<u8> {
        destroy_stage(self.damage)
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
        tool: Option<ItemStack>,
        on_ground: bool,
        in_water: bool,
    ) -> Option<BlockHit> {
        if !is_breakable(hit.block) {
            return None;
        }
        if mine_step(hit.block, tool, on_ground, in_water) < 1.0 {
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
        tool: Option<ItemStack>,
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
        let step = mine_step(hit.block, tool, on_ground, in_water);
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

/// Atlas tile index along the bottom row of `terrain.png` for this damage.
///
/// Matches `240 + (int)(damagePartialTime * 10)` in `RenderGlobal.drawBlockBreaking`.
pub fn destroy_stage(damage: f32) -> Option<u8> {
    if damage <= 0.0 {
        None
    } else {
        Some((damage * 10.0).min(9.0) as u8)
    }
}

/// Ticks of punching needed to break `block` by hand, or `None` if it cannot.
pub fn hand_ticks_to_break(block: BlockId, on_ground: bool, in_water: bool) -> Option<u32> {
    ticks_to_break(block, None, on_ground, in_water)
}

//! The 20 Hz world clock, matching Beta's `Timer`.
//!
//! One accumulator drives mining, the arm swing, dropped items, particles, and
//! the water and lava animations. Rendering reads [`WorldTick::partial`] to interpolate
//! between the last two ticks. The clock runs only while a world is being
//! played, so the menu does not spend ticks or dump a catch-up burst later.

use bevy::prelude::*;

use crate::app::state::AppScreen;

/// Beta's `ticksPerSecond`.
pub const TICKS_PER_SECOND: f32 = 20.0;
/// Seconds in one tick.
pub const TICK_SECONDS: f32 = 1.0 / TICKS_PER_SECOND;
/// `Timer` drops anything beyond this many ticks in a single frame.
pub const MAX_TICKS_PER_FRAME: u32 = 10;
/// One Minecraft day, in ticks.
pub const DAY_LENGTH: u64 = 24_000;

/// Shared simulation clock. `world_time` is the persisted day counter.
#[derive(Resource, Debug, Clone)]
pub struct WorldTick {
    accumulator: f32,
    ticks_this_frame: u32,
    partial: f32,
    world_time: u64,
}

impl Default for WorldTick {
    fn default() -> Self {
        Self {
            accumulator: 0.0,
            ticks_this_frame: 0,
            partial: 0.0,
            world_time: 0,
        }
    }
}

impl WorldTick {
    pub fn ticks_this_frame(&self) -> u32 {
        self.ticks_this_frame
    }

    /// Fraction of the way from the previous tick to the next, in `0..1`.
    pub fn partial(&self) -> f32 {
        self.partial
    }

    pub fn world_time(&self) -> u64 {
        self.world_time
    }

    pub fn set_world_time(&mut self, time: u64) {
        self.world_time = time;
    }

    /// Consume a frame delta the way `Timer.updateTimer` does.
    ///
    /// The delta is clamped to one second. At most [`MAX_TICKS_PER_FRAME`]
    /// ticks are emitted; any further whole ticks are discarded, and the
    /// leftover fraction stays in [`Self::partial`].
    pub fn advance(&mut self, delta_secs: f32) -> u32 {
        let delta = delta_secs.clamp(0.0, 1.0);
        self.accumulator += delta * TICKS_PER_SECOND;
        let mut ticks = self.accumulator.floor() as u32;
        self.accumulator -= ticks as f32;
        if ticks > MAX_TICKS_PER_FRAME {
            ticks = MAX_TICKS_PER_FRAME;
        }
        self.accumulator = self.accumulator.clamp(0.0, 1.0);
        self.ticks_this_frame = ticks;
        self.partial = self.accumulator;
        self.world_time = self.world_time.wrapping_add(u64::from(ticks));
        ticks
    }

    /// Forget a frame that should not simulate, without changing the fraction.
    pub fn idle(&mut self) {
        self.ticks_this_frame = 0;
    }
}

pub fn advance_world_tick(
    mut tick: ResMut<WorldTick>,
    time: Res<Time>,
    state: Option<Res<State<AppScreen>>>,
) {
    let playing = matches!(state.as_deref().map(State::get), Some(AppScreen::Playing));
    if playing {
        tick.advance(time.delta_secs());
    } else {
        tick.idle();
    }
}

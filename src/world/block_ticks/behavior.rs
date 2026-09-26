//! The per-block hooks of Beta's `Block` class and the table that maps each
//! compact block value to its implementation.

use bevy::math::IVec3;

use crate::block::id::Id;

use super::behaviors;
use super::world::TickWorld;

/// The update hooks Beta's `Block` exposes to `World`. Every method has a
/// no-op default, so a block implements only the events it reacts to.
///
/// Implementations are stateless unit structs registered in [`behavior`]'s
/// table. One implementation may serve several compact values, such as every
/// torch facing or every leaf species, and receives the value it was called
/// for through the world. See `docs/BLOCK_TICKS.md` for a walkthrough.
pub trait BlockBehavior: Sync {
    /// Beta `Block.tickOnLoad`: whether the world's random ticks reach this
    /// block. Random ticks call [`Self::update_tick`], like scheduled ticks.
    fn ticks_randomly(&self, _block: Id) -> bool {
        false
    }

    /// Beta `Block.tickRate`: the delay, in world ticks, a block usually
    /// passes to [`TickWorld::schedule`] for itself.
    fn tick_rate(&self, _block: Id) -> u32 {
        10
    }

    /// Beta `Block.updateTick`. Runs when a scheduled tick for this block
    /// comes due, and on random ticks when [`Self::ticks_randomly`] is set.
    fn update_tick(&self, _world: &mut TickWorld, _position: IVec3) {}

    /// Beta `Block.onNeighborBlockChange`. `neighbor` is the block that
    /// changed next to `position`, as passed to
    /// [`TickWorld::notify_neighbors`].
    fn neighbor_changed(&self, _world: &mut TickWorld, _position: IVec3, _neighbor: Id) {}

    /// Beta `Block.onBlockAdded`. Runs after this block is written into the
    /// world, before neighbors are notified.
    fn on_added(&self, _world: &mut TickWorld, _position: IVec3) {}

    /// Beta `Block.onBlockRemoval`. Runs after `previous` has been replaced
    /// at `position`, so the world already holds the new block.
    fn on_removed(&self, _world: &mut TickWorld, _position: IVec3, _previous: Id, _metadata: u8) {}

    /// Beta `Block.harvestBlock`'s world side effect after a player broke this
    /// block with a tool that can harvest it. The block is already gone and
    /// its drops are handled by `entity::drops`.
    fn harvested(&self, _world: &mut TickWorld, _position: IVec3, _block: Id, _metadata: u8) {}

    /// Beta `Block.onBlockClicked`: the player started mining this block.
    fn clicked(&self, _world: &mut TickWorld, _position: IVec3) {}

    /// Beta `Block.blockActivated`: the player right-clicked this block.
    fn activated(&self, _world: &mut TickWorld, _position: IVec3) {}

    /// Beta `Block.onEntityWalking`: an entity took a step on this block.
    fn entity_walked(&self, _world: &mut TickWorld, _position: IVec3) {}
}

/// A block with no update behavior.
pub(super) struct Inert;

impl BlockBehavior for Inert {}

static INERT: Inert = Inert;

/// The implementation for every compact block value. Families that share
/// behavior share an entry, as the block definition table does.
static BEHAVIORS: std::sync::LazyLock<[&dyn BlockBehavior; 256]> =
    std::sync::LazyLock::new(behaviors::table);

/// The update behavior for a block. Unregistered values are inert.
#[inline]
pub fn behavior(block: Id) -> &'static dyn BlockBehavior {
    match block {
        Id::Unknown(_) => &INERT,
        _ => BEHAVIORS[usize::from(block.as_u8())],
    }
}

/// [`BlockBehavior::ticks_randomly`] for every raw block byte, so the random
/// tick sampler can test chunk bytes without decoding them.
static RANDOM_TICKS: std::sync::LazyLock<[bool; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|raw| {
        let block = Id::from(raw as u8);
        behavior(block).ticks_randomly(block)
    })
});

/// Whether random ticks reach this raw block byte.
#[inline]
pub fn ticks_randomly(raw: u8) -> bool {
    RANDOM_TICKS[usize::from(raw)]
}

/// Start a behavior table with every value inert. `behaviors::table` fills
/// in the registered blocks.
pub(super) const fn inert_table() -> [&'static dyn BlockBehavior; 256] {
    [&INERT as &dyn BlockBehavior; 256]
}

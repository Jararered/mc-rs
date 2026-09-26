//! Static block behavior definitions.
//!
//! Chunks continue to store [`BlockId`](super::block::BlockId). This layer
//! resolves that compact value to an allocation-free family definition and
//! provides the gameplay properties consumed by physics, mining, picking, and
//! lighting.

use super::id::BlockId;
use std::sync::LazyLock;

pub type BlockBounds = ([f32; 3], [f32; 3]);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockProperties {
    pub hardness: f32,
    pub harvestable_by_hand: bool,
    pub targetable: bool,
    pub replaceable: bool,
    pub opaque_cube: bool,
    pub blocks_movement: bool,
    pub slipperiness: f32,
    pub collision_bounds: Option<BlockBounds>,
    pub selection_bounds: BlockBounds,
    pub crossed_plant: bool,
    pub torch: bool,
    pub light_opacity: u8,
    pub light_emission: u8,
}

impl BlockProperties {
    pub const FULL_BOUNDS: BlockBounds = ([0.0; 3], [1.0; 3]);

    /// Default opaque, colliding cube behavior. Family definitions override
    /// only the fields which make a block special.
    pub const fn solid(hardness: f32) -> Self {
        Self {
            hardness,
            harvestable_by_hand: true,
            targetable: true,
            replaceable: false,
            opaque_cube: true,
            blocks_movement: true,
            slipperiness: 0.6,
            collision_bounds: Some(Self::FULL_BOUNDS),
            selection_bounds: Self::FULL_BOUNDS,
            crossed_plant: false,
            torch: false,
            light_opacity: 15,
            light_emission: 0,
        }
    }

    pub const fn unknown() -> Self {
        Self::solid(0.0)
    }

    pub const fn non_colliding(hardness: f32) -> Self {
        Self {
            opaque_cube: false,
            blocks_movement: false,
            collision_bounds: None,
            light_opacity: 0,
            ..Self::solid(hardness)
        }
    }

    pub const fn fluid(hardness: f32) -> Self {
        Self {
            targetable: false,
            replaceable: true,
            ..Self::non_colliding(hardness)
        }
    }
}

/// Behavior shared by concrete block definitions or state families.
///
/// The full compact state is passed to each method so oriented and species
/// variants can share an implementation while preserving their distinct
/// behavior.
pub trait BlockDefinition: Sync {
    fn name(&self, state: BlockId) -> &'static str;
    fn in_world(&self, state: BlockId) -> bool;
    fn properties(&self, state: BlockId) -> BlockProperties;

    /// Hot-path scalar queries have defaults for ordinary full opaque cubes.
    /// Special families override them so lighting and meshing do not build a
    /// complete property record for each sampled voxel.
    fn opaque_cube(&self, _state: BlockId) -> bool {
        true
    }

    fn light_opacity(&self, _state: BlockId) -> u8 {
        15
    }

    fn light_emission(&self, _state: BlockId) -> u8 {
        0
    }

    fn crossed_plant(&self, _state: BlockId) -> bool {
        false
    }

    fn torch(&self, _state: BlockId) -> bool {
        false
    }
}

/// Resolve a compact block value to its static family definition.
#[inline]
pub fn definition(state: BlockId) -> &'static dyn BlockDefinition {
    super::registry::definition(state)
}

/// Cached block properties for hot voxel queries. The table is initialized
/// from the static definitions once, without heap storage.
static BLOCK_PROPERTIES: LazyLock<[BlockProperties; 256]> = LazyLock::new(|| {
    std::array::from_fn(|raw| {
        let raw = raw as u8;
        let state = BlockId::from_u8(raw).unwrap_or(BlockId::Unknown(raw));
        definition(state).properties(state)
    })
});

pub fn properties_table() -> &'static [BlockProperties; 256] {
    &BLOCK_PROPERTIES
}

#[inline]
pub fn properties(state: BlockId) -> BlockProperties {
    match state {
        BlockId::Unknown(_) => BlockProperties::unknown(),
        _ => properties_table()[state.as_u8() as usize],
    }
}

#[inline]
pub fn light_opacity(state: BlockId) -> u8 {
    properties(state).light_opacity
}

#[inline]
pub fn light_emission(state: BlockId) -> u8 {
    properties(state).light_emission
}

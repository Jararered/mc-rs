//! Static block properties: one record per block id, consumed by physics,
//! mining, picking, meshing, and lighting. Everything here depends on the id
//! alone; orientation and species live in chunk metadata (see
//! [`super::direction`]).

use super::blocks::Block;
use super::direction::Direction;
use super::properties::torch_selection_bounds;
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
    pub light_opacity: u8,
    pub light_emission: u8,
}

impl BlockProperties {
    pub const FULL_BOUNDS: BlockBounds = ([0.0; 3], [1.0; 3]);

    /// Default opaque, colliding cube behavior. [`build`] overrides only the
    /// fields which make a block special.
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
            light_opacity: 15,
            light_emission: 0,
        }
    }

    /// A solid block that drops nothing unless mined with the right tool.
    pub const fn tool_only(hardness: f32) -> Self {
        Self {
            harvestable_by_hand: false,
            ..Self::solid(hardness)
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

/// The properties of one block id.
fn build(block: Block) -> BlockProperties {
    use BlockProperties as P;
    match block {
        Block::Air => P::fluid(0.0),
        Block::Stone => P::tool_only(1.5),
        Block::Grass | Block::Gravel | Block::Sponge | Block::Clay => P::solid(0.6),
        Block::Farmland => P {
            opaque_cube: false,
            selection_bounds: ([0.0; 3], [1.0, 15.0 / 16.0, 1.0]),
            ..P::solid(0.6)
        },
        Block::Dirt | Block::Sand => P::solid(0.5),
        Block::Cobblestone | Block::DoubleStoneSlab | Block::Bricks | Block::MossyCobblestone => {
            P::tool_only(2.0)
        }
        Block::WoodenPlanks | Block::Wood | Block::Jukebox => P::solid(2.0),
        Block::Bedrock => P::tool_only(-1.0),
        Block::Sandstone => P::tool_only(0.8),
        Block::NoteBlock | Block::Wool => P::solid(0.8),
        Block::GoldBlock | Block::GoldOre | Block::IronOre | Block::CoalOre => P::tool_only(3.0),
        Block::LapisOre | Block::LapisBlock | Block::DiamondOre | Block::RedstoneOre => {
            P::tool_only(3.0)
        }
        Block::LitRedstoneOre => P {
            light_emission: 9,
            ..P::tool_only(3.0)
        },
        Block::IronBlock | Block::DiamondBlock => P::tool_only(5.0),
        Block::Bookshelf => P::solid(1.5),
        Block::Obsidian => P::tool_only(10.0),
        Block::MobSpawner => P {
            opaque_cube: false,
            ..P::solid(0.0)
        },
        Block::StoneSlab
        | Block::CobblestoneStairs
        | Block::IronDoor
        | Block::Cobweb
        | Block::StonePressurePlate => P::tool_only(0.0),
        Block::CraftingTable => P::solid(2.5),
        Block::SnowLayer => P {
            opaque_cube: false,
            collision_bounds: None,
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            light_opacity: 0,
            ..P::tool_only(0.1)
        },
        Block::Ice => P {
            opaque_cube: false,
            slipperiness: 0.98,
            light_opacity: 3,
            ..P::tool_only(0.5)
        },
        Block::Snow => P::tool_only(0.2),
        Block::Netherrack => P::tool_only(0.4),
        Block::Dispenser => P::tool_only(3.5),
        // Water and lava.
        Block::FlowingWater | Block::Water => P {
            light_opacity: 3,
            ..P::fluid(100.0)
        },
        Block::FlowingLava | Block::Lava => P {
            light_opacity: 15,
            light_emission: 15,
            ..P::fluid(0.0)
        },
        // Plants.
        Block::Leaves => P {
            opaque_cube: false,
            light_opacity: 1,
            ..P::solid(0.2)
        },
        Block::TallGrass | Block::DeadBush => crossed_plant(([0.1, 0.0, 0.1], [0.9, 0.8, 0.9])),
        Block::Dandelion | Block::Rose => crossed_plant(([0.3, 0.0, 0.3], [0.7, 0.6, 0.7])),
        Block::BrownMushroom | Block::RedMushroom => {
            crossed_plant(([0.3, 0.0, 0.3], [0.7, 0.4, 0.7]))
        }
        Block::SugarCane => crossed_plant(([0.125, 0.0, 0.125], [0.875, 1.0, 0.875])),
        // `BlockCrops`: a quarter-block selection box and no collision.
        Block::Crops => P {
            selection_bounds: ([0.0; 3], [1.0, 0.25, 1.0]),
            ..P::non_colliding(0.0)
        },
        Block::Cactus => P {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0625, 0.0, 0.0625], [0.9375, 0.9375, 0.9375])),
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 1.0, 0.9375]),
            ..P::solid(0.4)
        },
        // Light sources.
        Block::Torch => P {
            light_emission: 15,
            selection_bounds: oriented_bounds(Block::Torch, 0),
            ..P::non_colliding(0.0)
        },
        Block::Fire => P {
            light_emission: 15,
            targetable: false,
            replaceable: true,
            crossed_plant: true,
            ..P::non_colliding(0.0)
        },
        Block::Glowstone => P {
            light_emission: 15,
            ..P::tool_only(0.3)
        },
        Block::JackOLantern => P {
            light_emission: 15,
            ..P::solid(1.0)
        },
        // Oriented blocks.
        Block::Chest => P {
            opaque_cube: false,
            ..P::solid(2.5)
        },
        Block::Ladder => P {
            opaque_cube: false,
            light_opacity: 0,
            ..P::solid(0.4)
        },
        Block::Furnace => P::tool_only(3.5),
        Block::LitFurnace => P {
            light_emission: 13,
            ..P::tool_only(3.5)
        },
        Block::Pumpkin => P::solid(1.0),
        _ => P::unknown(),
    }
}

fn crossed_plant(selection_bounds: BlockBounds) -> BlockProperties {
    BlockProperties {
        crossed_plant: true,
        selection_bounds,
        ..BlockProperties::non_colliding(0.0)
    }
}

/// Bounds of a torch or ladder, rotated onto the side named by `metadata`.
/// A floor torch and an unattached ladder use the block's own box.
pub(crate) fn oriented_bounds(block: Block, metadata: u8) -> BlockBounds {
    let facing = block.facing(metadata);
    match block {
        Block::Ladder => match facing {
            Some(Direction::North) => ([0.0, 0.0, 0.0], [1.0, 1.0, 0.125]),
            Some(Direction::South) => ([0.0, 0.0, 0.875], [1.0, 1.0, 1.0]),
            Some(Direction::East) => ([0.875, 0.0, 0.0], [1.0, 1.0, 1.0]),
            Some(Direction::West) => ([0.0, 0.0, 0.0], [0.125, 1.0, 1.0]),
            None => BlockProperties::FULL_BOUNDS,
        },
        _ => torch_selection_bounds(facing),
    }
}

/// Cached block properties for hot voxel queries, built once from [`build`].
static BLOCK_PROPERTIES: LazyLock<[BlockProperties; 256]> = LazyLock::new(|| {
    std::array::from_fn(|raw| {
        let raw = raw as u8;
        build(Block::from_u8(raw).unwrap_or(Block::Unknown(raw)))
    })
});

pub fn properties_table() -> &'static [BlockProperties; 256] {
    &BLOCK_PROPERTIES
}

#[inline]
pub fn properties(block: Block) -> BlockProperties {
    match block {
        Block::Unknown(_) => BlockProperties::unknown(),
        _ => properties_table()[block.as_u8() as usize],
    }
}

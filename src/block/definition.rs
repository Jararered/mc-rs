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

/// The properties of one block id, from Beta's `Block` registry. Every Beta
/// id has an arm; [`Block::Unknown`] is the only fallback.
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
        // `BlockSoulSand`: the collision box is an eighth short.
        Block::SoulSand => P {
            collision_bounds: Some(([0.0; 3], [1.0, 0.875, 1.0])),
            ..P::solid(0.5)
        },
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
        Block::Tnt => P::solid(0.0),
        Block::MobSpawner => P {
            opaque_cube: false,
            ..P::tool_only(5.0)
        },
        Block::Glass => P {
            opaque_cube: false,
            light_opacity: 0,
            ..P::solid(0.3)
        },
        Block::Dispenser => P::tool_only(3.5),
        Block::Bed => P {
            opaque_cube: false,
            light_opacity: 0,
            selection_bounds: ([0.0; 3], [1.0, 9.0 / 16.0, 1.0]),
            collision_bounds: Some(([0.0; 3], [1.0, 9.0 / 16.0, 1.0])),
            ..P::solid(0.2)
        },
        Block::Rail | Block::PoweredRail | Block::DetectorRail => P {
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            ..P::non_colliding(0.7)
        },
        Block::Piston | Block::StickyPiston | Block::PistonHead => P::solid(0.5),
        Block::MovingPiston => P {
            targetable: false,
            ..P::solid(-1.0)
        },
        Block::Cobweb => P {
            harvestable_by_hand: false,
            light_opacity: 1,
            ..P::non_colliding(4.0)
        },
        Block::StoneSlab => P {
            opaque_cube: false,
            collision_bounds: Some(([0.0; 3], [1.0, 0.5, 1.0])),
            selection_bounds: ([0.0; 3], [1.0, 0.5, 1.0]),
            ..P::tool_only(2.0)
        },
        Block::CobblestoneStairs => P {
            opaque_cube: false,
            ..P::tool_only(2.0)
        },
        Block::WoodenStairs => P {
            opaque_cube: false,
            ..P::solid(2.0)
        },
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
        Block::Sapling | Block::TallGrass | Block::DeadBush => {
            crossed_plant(([0.1, 0.0, 0.1], [0.9, 0.8, 0.9]))
        }
        Block::Dandelion | Block::Rose => crossed_plant(([0.3, 0.0, 0.3], [0.7, 0.6, 0.7])),
        Block::RedMushroom => crossed_plant(([0.3, 0.0, 0.3], [0.7, 0.4, 0.7])),
        Block::BrownMushroom => BlockProperties {
            light_emission: 1,
            ..crossed_plant(([0.3, 0.0, 0.3], [0.7, 0.4, 0.7]))
        },
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
        // Light sources. Beta stores `(int)(15 * brightness)`.
        Block::Torch => P {
            light_emission: 14,
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
        Block::RedstoneTorch => P {
            light_emission: 7,
            ..P::non_colliding(0.0)
        },
        Block::UnlitRedstoneTorch | Block::RedstoneWire => P::non_colliding(0.0),
        Block::NetherPortal => P {
            light_emission: 11,
            targetable: false,
            ..P::non_colliding(-1.0)
        },
        Block::LockedChest => P {
            light_emission: 15,
            ..P::solid(0.0)
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
        Block::CraftingTable => P::solid(2.5),
        // Wooden and iron fixtures.
        // `BlockFence.getCollisionBoundingBoxFromPool`: half a block taller
        // than the cell, so it cannot be jumped.
        Block::Fence => P {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0; 3], [1.0, 1.5, 1.0])),
            ..P::solid(2.0)
        },
        Block::StandingSign | Block::WallSign => P::non_colliding(1.0),
        Block::WoodenDoor | Block::Trapdoor => P {
            opaque_cube: false,
            light_opacity: 0,
            ..P::solid(3.0)
        },
        Block::IronDoor => P {
            opaque_cube: false,
            light_opacity: 0,
            ..P::tool_only(5.0)
        },
        Block::Lever | Block::StoneButton => P::non_colliding(0.5),
        Block::StonePressurePlate => P {
            harvestable_by_hand: false,
            ..P::non_colliding(0.5)
        },
        Block::WoodenPressurePlate => P::non_colliding(0.5),
        Block::Cake => P {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0625, 0.0, 0.0625], [0.9375, 0.5, 0.9375])),
            selection_bounds: ([0.0625, 0.0, 0.0625], [0.9375, 0.5, 0.9375]),
            ..P::solid(0.5)
        },
        Block::Repeater => P {
            opaque_cube: false,
            light_opacity: 0,
            collision_bounds: Some(([0.0; 3], [1.0, 0.125, 1.0])),
            selection_bounds: ([0.0; 3], [1.0, 0.125, 1.0]),
            ..P::solid(0.0)
        },
        Block::PoweredRepeater => P {
            light_emission: 9,
            ..build(Block::Repeater)
        },
        Block::Unknown(_) => P::unknown(),
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

/// Thickness of a door or trapdoor panel.
const PANEL: f32 = 0.1875;

/// `BlockDoor.setDoorRotation` on `BlockDoor.getState`: the panel hugs one
/// side of its cell, and an open door swings a quarter turn.
pub(crate) fn door_bounds(metadata: u8) -> BlockBounds {
    let state = if metadata & 4 == 0 {
        metadata.wrapping_sub(1) & 3
    } else {
        metadata & 3
    };
    match state {
        0 => ([0.0, 0.0, 0.0], [1.0, 1.0, PANEL]),
        1 => ([1.0 - PANEL, 0.0, 0.0], [1.0, 1.0, 1.0]),
        2 => ([0.0, 0.0, 1.0 - PANEL], [1.0, 1.0, 1.0]),
        _ => ([0.0, 0.0, 0.0], [PANEL, 1.0, 1.0]),
    }
}

/// `BlockSign.setBlockBoundsBasedOnState`: a post's half-wide column, or a
/// wall sign's board against the block behind it (`metadata` 2 to 5, the
/// face it hangs on).
pub(crate) fn sign_bounds(block: Block, metadata: u8) -> BlockBounds {
    if block == Block::StandingSign {
        return ([0.25, 0.0, 0.25], [0.75, 1.0, 0.75]);
    }
    let (low, high, thick) = (0.281_25, 0.781_25, 0.125);
    match metadata {
        2 => ([0.0, low, 1.0 - thick], [1.0, high, 1.0]),
        3 => ([0.0, low, 0.0], [1.0, high, thick]),
        4 => ([1.0 - thick, low, 0.0], [1.0, high, 1.0]),
        _ => ([0.0, low, 0.0], [thick, high, 1.0]),
    }
}

/// `BlockTrapDoor.setBlockBoundsForBlockRender`: flat on the floor of its
/// cell when closed, upright against its supporting wall when open.
pub(crate) fn trapdoor_bounds(metadata: u8) -> BlockBounds {
    if metadata & 4 == 0 {
        return ([0.0; 3], [1.0, PANEL, 1.0]);
    }
    match metadata & 3 {
        0 => ([0.0, 0.0, 1.0 - PANEL], [1.0, 1.0, 1.0]),
        1 => ([0.0, 0.0, 0.0], [1.0, 1.0, PANEL]),
        2 => ([1.0 - PANEL, 0.0, 0.0], [1.0, 1.0, 1.0]),
        _ => ([0.0, 0.0, 0.0], [PANEL, 1.0, 1.0]),
    }
}

/// A slab of `thickness` against the low or high end of `axis`.
fn slab(axis: usize, high: bool, thickness: f32) -> BlockBounds {
    let mut min = [0.0; 3];
    let mut max = [1.0; 3];
    if high {
        min[axis] = 1.0 - thickness;
    } else {
        max[axis] = thickness;
    }
    (min, max)
}

/// Beta's metadata-dependent boxes for the redstone family, shared by
/// picking, the outline, and collision where the block collides.
pub(crate) fn redstone_bounds(block: Block, metadata: u8) -> Option<BlockBounds> {
    Some(match block {
        // `BlockPistonBase`: extended, the base is three quarters thick on the
        // side away from its head.
        Block::Piston | Block::StickyPiston if metadata & 8 != 0 => match metadata & 7 {
            0 => slab(1, true, 0.75),
            1 => slab(1, false, 0.75),
            2 => slab(2, true, 0.75),
            3 => slab(2, false, 0.75),
            4 => slab(0, true, 0.75),
            _ => slab(0, false, 0.75),
        },
        // `BlockPistonExtension`: the head is a quarter-thick plate.
        Block::PistonHead => match metadata & 7 {
            0 => slab(1, false, 0.25),
            1 => slab(1, true, 0.25),
            2 => slab(2, false, 0.25),
            3 => slab(2, true, 0.25),
            4 => slab(0, false, 0.25),
            _ => slab(0, true, 0.25),
        },
        // `BlockButton.setBlockBoundsBasedOnState`: a small plate that sinks
        // from 2/16 to 1/16 thick while pressed.
        Block::StoneButton => {
            let t = if metadata & 8 != 0 { 0.0625 } else { 0.125 };
            match metadata & 7 {
                1 => ([0.0, 0.375, 0.3125], [t, 0.625, 0.6875]),
                2 => ([1.0 - t, 0.375, 0.3125], [1.0, 0.625, 0.6875]),
                3 => ([0.3125, 0.375, 0.0], [0.6875, 0.625, t]),
                _ => ([0.3125, 0.375, 1.0 - t], [0.6875, 0.625, 1.0]),
            }
        }
        Block::Lever => match metadata & 7 {
            1 => ([0.0, 0.2, 0.25], [0.5, 0.8, 0.75]),
            2 => ([0.5, 0.2, 0.25], [1.0, 0.8, 0.75]),
            3 => ([0.25, 0.2, 0.0], [0.75, 0.8, 0.5]),
            4 => ([0.25, 0.2, 0.5], [0.75, 0.8, 1.0]),
            _ => ([0.25, 0.0, 0.25], [0.75, 0.6, 0.75]),
        },
        Block::RedstoneWire => slab(1, false, 1.0 / 16.0),
        Block::RedstoneTorch | Block::UnlitRedstoneTorch => {
            torch_selection_bounds(Block::Torch.facing(metadata & 7))
        }
        Block::StonePressurePlate | Block::WoodenPressurePlate => (
            [1.0 / 16.0, 0.0, 1.0 / 16.0],
            [
                15.0 / 16.0,
                if metadata == 0 {
                    1.0 / 16.0
                } else {
                    0.5 / 16.0
                },
                15.0 / 16.0,
            ],
        ),
        _ => return None,
    })
}

/// `BlockStairs.getCollidingBoundingBoxes`: a half-height step and a
/// full-height riser, turned by `metadata`.
pub(crate) fn stairs_boxes(metadata: u8) -> [BlockBounds; 2] {
    match metadata & 3 {
        0 => [
            ([0.0, 0.0, 0.0], [0.5, 0.5, 1.0]),
            ([0.5, 0.0, 0.0], [1.0, 1.0, 1.0]),
        ],
        1 => [
            ([0.0, 0.0, 0.0], [0.5, 1.0, 1.0]),
            ([0.5, 0.0, 0.0], [1.0, 0.5, 1.0]),
        ],
        2 => [
            ([0.0, 0.0, 0.0], [1.0, 0.5, 0.5]),
            ([0.0, 0.0, 0.5], [1.0, 1.0, 1.0]),
        ],
        _ => [
            ([0.0, 0.0, 0.0], [1.0, 1.0, 0.5]),
            ([0.0, 0.0, 0.5], [1.0, 0.5, 1.0]),
        ],
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

static UNKNOWN_PROPERTIES: BlockProperties = BlockProperties::unknown();

#[inline]
pub fn properties(block: Block) -> &'static BlockProperties {
    match block {
        Block::Unknown(_) => &UNKNOWN_PROPERTIES,
        _ => &properties_table()[block.as_u8() as usize],
    }
}

//! Static block properties: one record per block id, consumed by physics,
//! mining, picking, meshing, and lighting. Everything here depends on the id
//! alone; orientation and species live in chunk metadata (see
//! [`super::direction`]).

use super::blocks::Block;
use super::direction::Direction;
use super::properties::torch_selection_bounds;
use serde::Deserialize;
use std::collections::HashMap;
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
    /// `BlockFire.setBurnRate`: `chanceToEncourageFire` (how readily fire
    /// appears beside the block) and `abilityToCatchFire` (how readily it
    /// burns away).
    pub burn: (u8, u8),
    /// `setResistance`'s argument, for a block that has one of its own.
    pub resistance: Option<f32>,
    /// Beta's `Material.isSolid` classification.
    pub solid_material: bool,
    /// The tool whose `blocksEffectiveAgainst` lists the block.
    pub tool: Option<Digger>,
    /// The lowest pickaxe harvest level `ItemPickaxe.canHarvestBlock`
    /// accepts, for a block only a pickaxe harvests.
    pub pick_level: Option<u8>,
}

/// A tool with a list of blocks it digs quickly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Digger {
    Pick,
    Axe,
    Shovel,
}

impl BlockProperties {
    pub const FULL_BOUNDS: BlockBounds = ([0.0; 3], [1.0; 3]);

    /// Default opaque, colliding cube behavior. A row of `data/blocks.ron`
    /// overrides only the fields which make a block special.
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
            burn: (0, 0),
            resistance: None,
            solid_material: true,
            tool: None,
            pick_level: None,
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

#[derive(Deserialize)]
enum Base {
    Solid,
    ToolOnly,
    NonColliding,
    Fluid,
    CrossedPlant,
}

/// One row of `data/blocks.ron`: a preset and what differs from it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    base: Base,
    #[serde(default)]
    hardness: f32,
    harvestable_by_hand: Option<bool>,
    targetable: Option<bool>,
    replaceable: Option<bool>,
    opaque_cube: Option<bool>,
    blocks_movement: Option<bool>,
    slipperiness: Option<f32>,
    collision: Option<BlockBounds>,
    selection: Option<BlockBounds>,
    crossed_plant: Option<bool>,
    light_opacity: Option<u8>,
    light_emission: Option<u8>,
    #[serde(default)]
    burn: (u8, u8),
    resistance: Option<f32>,
    solid_material: Option<bool>,
    tool: Option<Digger>,
    pick_level: Option<u8>,
}

impl Row {
    fn build(self) -> BlockProperties {
        use BlockProperties as P;
        let base = match self.base {
            Base::Solid => P::solid(self.hardness),
            Base::ToolOnly => P::tool_only(self.hardness),
            Base::NonColliding => P::non_colliding(self.hardness),
            Base::Fluid => P::fluid(self.hardness),
            Base::CrossedPlant => P {
                crossed_plant: true,
                ..P::non_colliding(self.hardness)
            },
        };
        P {
            harvestable_by_hand: self.harvestable_by_hand.unwrap_or(base.harvestable_by_hand),
            targetable: self.targetable.unwrap_or(base.targetable),
            replaceable: self.replaceable.unwrap_or(base.replaceable),
            opaque_cube: self.opaque_cube.unwrap_or(base.opaque_cube),
            blocks_movement: self.blocks_movement.unwrap_or(base.blocks_movement),
            slipperiness: self.slipperiness.unwrap_or(base.slipperiness),
            collision_bounds: self.collision.or(base.collision_bounds),
            selection_bounds: self.selection.unwrap_or(base.selection_bounds),
            crossed_plant: self.crossed_plant.unwrap_or(base.crossed_plant),
            light_opacity: self.light_opacity.unwrap_or(base.light_opacity),
            light_emission: self.light_emission.unwrap_or(base.light_emission),
            burn: self.burn,
            resistance: self.resistance,
            solid_material: self
                .solid_material
                .unwrap_or(!matches!(self.base, Base::Fluid | Base::CrossedPlant)),
            tool: self.tool,
            pick_level: self.pick_level,
            ..base
        }
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
/// Beta's `Block` registry, read once from `data/blocks.ron`. Ids that are
/// not blocks keep [`BlockProperties::unknown`].
static BLOCK_PROPERTIES: LazyLock<[BlockProperties; 256]> = LazyLock::new(|| {
    let rows: HashMap<Block, Row> = ron::from_str(include_str!("../../data/blocks.ron"))
        .unwrap_or_else(|error| panic!("data/blocks.ron: {error}"));
    assert_eq!(
        rows.len(),
        usize::from(Block::MAX_ITEM_ID) + 1,
        "data/blocks.ron has one row for every block id"
    );
    let mut table = [BlockProperties::unknown(); 256];
    for (block, row) in rows {
        table[usize::from(block.as_u8())] = row.build();
    }
    table[usize::from(Block::Torch.as_u8())].selection_bounds = oriented_bounds(Block::Torch, 0);
    table
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

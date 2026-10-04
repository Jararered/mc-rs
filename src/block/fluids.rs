//! Water and lava: the level math Beta's
//! `BlockFluid` and `RenderBlocks` share. Metadata is a fluid's decay: `0`
//! for a source, `1..=7` for how far it has spread, and `8` and up for fluid
//! falling from above.

use crate::block::blocks::Block;

/// A fluid material, Beta's `Material.water` or `Material.lava`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FluidType {
    Water,
    Lava,
}

impl FluidType {
    /// The material of `block`, if it is a fluid.
    pub const fn of(block: Block) -> Option<Self> {
        match block {
            Block::Water | Block::FlowingWater => Some(Self::Water),
            Block::Lava | Block::FlowingLava => Some(Self::Lava),
            _ => None,
        }
    }

    pub const fn flowing(self) -> Block {
        match self {
            Self::Water => Block::FlowingWater,
            Self::Lava => Block::FlowingLava,
        }
    }

    pub const fn still(self) -> Block {
        match self {
            Self::Water => Block::Water,
            Self::Lava => Block::Lava,
        }
    }

    /// `BlockFluid.tickRate`.
    pub const fn tick_rate(self) -> u32 {
        match self {
            Self::Water => 5,
            Self::Lava => 30,
        }
    }

    /// Decay added per block of spread. Lava spreads half as far outside the
    /// Nether.
    pub const fn decay_step(self) -> i32 {
        match self {
            Self::Water => 1,
            Self::Lava => 2,
        }
    }
}

/// Beta `Material.getIsLiquid`.
pub const fn is_liquid(block: Block) -> bool {
    FluidType::of(block).is_some()
}

pub const fn is_water(block: Block) -> bool {
    matches!(FluidType::of(block), Some(FluidType::Water))
}

pub const fn is_lava(block: Block) -> bool {
    matches!(FluidType::of(block), Some(FluidType::Lava))
}

/// `BlockFluid.getPercentAir`: how far below the top of its cell a fluid's
/// surface sits, as a fraction of the block. Falling fluid counts as a
/// source.
pub fn percent_air(metadata: u8) -> f32 {
    let decay = if metadata >= 8 { 0 } else { metadata };
    f32::from(decay + 1) / 9.0
}

/// `BlockFluid.getEffectiveFlowDecay`: `-1` unless the cell holds `fluid`;
/// falling fluid reads as a source.
pub fn effective_decay(fluid: FluidType, block: Block, metadata: u8) -> i32 {
    if FluidType::of(block) != Some(fluid) {
        return -1;
    }
    if metadata >= 8 {
        0
    } else {
        i32::from(metadata)
    }
}

/// `RenderBlocks` fluid corner height (`func_1224_a`): the surface height, in
/// blocks above `y`, at the corner shared by the four columns `x - 1..=x`
/// and `z - 1..=z`. `cell` reads a block and its metadata. Sources and
/// falling fluid weigh ten times more than spread fluid, and open non-solid
/// neighbors pull the corner down.
pub fn corner_height(
    fluid: FluidType,
    x: i32,
    y: i32,
    z: i32,
    cell: impl Fn(i32, i32, i32) -> (Block, u8),
) -> f32 {
    let mut weight = 0;
    let mut air = 0.0;
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let (cx, cz) = (x - dx, z - dz);
        if FluidType::of(cell(cx, y + 1, cz).0) == Some(fluid) {
            return 1.0;
        }
        let (block, metadata) = cell(cx, y, cz);
        if FluidType::of(block) == Some(fluid) {
            if metadata >= 8 || metadata == 0 {
                air += percent_air(metadata) * 10.0;
                weight += 10;
            }
            air += percent_air(metadata);
            weight += 1;
        } else if !block.is_solid_material() {
            air += 1.0;
            weight += 1;
        }
    }
    1.0 - air / weight as f32
}

/// The horizontal part of `BlockFluid.getFlowVector`, before normalizing:
/// the direction fluid at `(x, y, z)` runs toward lower levels or drops.
pub fn flow_vector(
    fluid: FluidType,
    x: i32,
    y: i32,
    z: i32,
    cell: impl Fn(i32, i32, i32) -> (Block, u8),
) -> [f32; 2] {
    let (block, metadata) = cell(x, y, z);
    let decay = effective_decay(fluid, block, metadata);
    let mut flow = [0.0f32; 2];
    for (dx, dz) in [(-1, 0), (0, -1), (1, 0), (0, 1)] {
        let (nx, nz) = (x + dx, z + dz);
        let (neighbor, neighbor_metadata) = cell(nx, y, nz);
        let neighbor_decay = effective_decay(fluid, neighbor, neighbor_metadata);
        let weight = if neighbor_decay < 0 {
            if neighbor.is_solid_material() {
                continue;
            }
            let (below, below_metadata) = cell(nx, y - 1, nz);
            let below_decay = effective_decay(fluid, below, below_metadata);
            if below_decay < 0 {
                continue;
            }
            below_decay - (decay - 8)
        } else {
            neighbor_decay - decay
        };
        flow[0] += (dx * weight) as f32;
        flow[1] += (dz * weight) as f32;
    }
    flow
}

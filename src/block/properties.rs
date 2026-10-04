//! Calculations involving neighboring blocks, geometry, or mining context.

use super::blocks::Block;

/// `BlockCactus.canBlockStay`: cactus may grow on sand or another cactus,
/// provided each horizontal neighbour has a non-solid material.
pub fn cactus_can_stay(below: Block, neighbors: [Block; 4]) -> bool {
    matches!(below, Block::Sand | Block::Cactus)
        && neighbors
            .into_iter()
            .all(|block| !block.is_solid_material())
}

/// Beta reed placement/growth rule. A cane segment stacks on another segment;
/// the bottom segment needs soil beside water on the same supporting level.
pub fn sugar_cane_can_stay(below: Block, adjacent_water: [bool; 4]) -> bool {
    below == Block::SugarCane
        || (matches!(below, Block::Grass | Block::Dirt | Block::Sand)
            && adjacent_water.into_iter().any(|is_water| is_water))
}

/// Rotate the floor torch's local geometry into its wall pose. This remains a
/// compatibility helper for the mesher; picking bounds are supplied by the
/// torch definition in the block family module.
pub fn torch_point(block: Block, point: [f32; 3]) -> [f32; 3] {
    if block == Block::Torch {
        return point;
    }
    let [x, y, z] = point;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        Block::TorchWest => [
            -0.04 + (x - 0.5) * cos + y * sin,
            0.34 - (x - 0.5) * sin + y * cos,
            z,
        ],
        Block::TorchEast => [
            1.04 + (x - 0.5) * cos - y * sin,
            0.34 + (x - 0.5) * sin + y * cos,
            z,
        ],
        Block::TorchNorth => [
            x,
            0.34 - (z - 0.5) * sin + y * cos,
            -0.04 + (z - 0.5) * cos + y * sin,
        ],
        Block::TorchSouth => [
            x,
            0.34 + (z - 0.5) * sin + y * cos,
            1.04 + (z - 0.5) * cos - y * sin,
        ],
        _ => point,
    }
}

pub fn torch_normal(block: Block, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        Block::TorchWest => [x * cos + y * sin, -x * sin + y * cos, z],
        Block::TorchEast => [x * cos - y * sin, x * sin + y * cos, z],
        Block::TorchNorth => [x, -z * sin + y * cos, z * cos + y * sin],
        Block::TorchSouth => [x, z * sin + y * cos, z * cos - y * sin],
        _ => normal,
    }
}

pub(crate) fn torch_selection_bounds(block: Block) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for x in [0.4, 0.6] {
        for y in [0.0, 0.625] {
            for z in [0.4, 0.6] {
                let point = torch_point(block, [x, y, z]);
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
    }
    for axis in 0..3 {
        min[axis] = min[axis].max(0.0);
        max[axis] = max[axis].min(1.0);
    }
    (min, max)
}

/// Damage added each game tick, matching `Block.blockStrength`.
///
/// `strength` is the held item's `getStrVsBlock` before the water and airborne
/// penalties. Those penalties and the `/ 30` divisor apply only when the block
/// can be harvested. Otherwise the tool is ignored and the step is `/ 100`.
/// Hardness `0` is an instant break.
pub fn mine_progress_per_tick(
    block: Block,
    strength: f32,
    can_harvest: bool,
    on_ground: bool,
    in_water: bool,
) -> f32 {
    let hardness = block.hardness();
    if hardness < 0.0 {
        return 0.0;
    }
    if hardness == 0.0 {
        return f32::INFINITY;
    }
    if can_harvest {
        let mut strength = strength;
        if in_water {
            strength /= 5.0;
        }
        if !on_ground {
            strength /= 5.0;
        }
        strength / hardness / 30.0
    } else {
        1.0 / hardness / 100.0
    }
}

/// Empty-hand `Block.blockStrength`.
pub fn hand_mine_progress_per_tick(block: Block, on_ground: bool, in_water: bool) -> f32 {
    mine_progress_per_tick(block, 1.0, block.harvestable_by_hand(), on_ground, in_water)
}

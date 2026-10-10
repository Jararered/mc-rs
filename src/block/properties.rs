//! Calculations involving neighboring blocks, geometry, or mining context.

use super::blocks::Block;
use super::definition::properties;
use super::direction::Direction;

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

/// `canThisPlantGrowOnThisBlockID` for each `BlockFlower` subclass: dead bushes
/// only take on sand, mushrooms on any opaque cube, crops on tilled soil, and
/// everything else on soil.
pub fn plant_ground_can_hold(plant: Block, ground: Block) -> bool {
    match plant {
        Block::DeadBush => ground == Block::Sand,
        Block::Crops => ground == Block::Farmland,
        Block::BrownMushroom | Block::RedMushroom => ground.is_opaque_cube(),
        _ => ground.supports_plants(),
    }
}

/// `BlockFire.setBurnRate`: `chanceToEncourageFire` (how readily fire appears
/// beside the block) and `abilityToCatchFire` (how readily it burns away).
pub fn burn_rates(block: Block) -> (u32, u32) {
    let (encourage, catch) = properties(block).burn;
    (u32::from(encourage), u32::from(catch))
}

/// `BlockFire.canBlockCatchFire`.
pub fn can_catch_fire(block: Block) -> bool {
    burn_rates(block).0 > 0
}

const TORCH_TILT: f32 = 0.55;

/// Rotate the floor torch's local geometry into its wall pose. `facing` is
/// the side the torch hangs on; `None` is a torch standing on the floor.
pub fn torch_point(facing: Option<Direction>, point: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = point;
    let sin = TORCH_TILT;
    let cos = (1.0 - sin * sin).sqrt();
    match facing {
        Some(Direction::West) => [
            -0.04 + (x - 0.5) * cos + y * sin,
            0.34 - (x - 0.5) * sin + y * cos,
            z,
        ],
        Some(Direction::East) => [
            1.04 + (x - 0.5) * cos - y * sin,
            0.34 + (x - 0.5) * sin + y * cos,
            z,
        ],
        Some(Direction::North) => [
            x,
            0.34 - (z - 0.5) * sin + y * cos,
            -0.04 + (z - 0.5) * cos + y * sin,
        ],
        Some(Direction::South) => [
            x,
            0.34 + (z - 0.5) * sin + y * cos,
            1.04 + (z - 0.5) * cos - y * sin,
        ],
        None => point,
    }
}

pub fn torch_normal(facing: Option<Direction>, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal;
    let sin = TORCH_TILT;
    let cos = (1.0 - sin * sin).sqrt();
    match facing {
        Some(Direction::West) => [x * cos + y * sin, -x * sin + y * cos, z],
        Some(Direction::East) => [x * cos - y * sin, x * sin + y * cos, z],
        Some(Direction::North) => [x, -z * sin + y * cos, z * cos + y * sin],
        Some(Direction::South) => [x, z * sin + y * cos, z * cos - y * sin],
        None => normal,
    }
}

pub(crate) fn torch_selection_bounds(facing: Option<Direction>) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for x in [0.4, 0.6] {
        for y in [0.0, 0.625] {
            for z in [0.4, 0.6] {
                let point = torch_point(facing, [x, y, z]);
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

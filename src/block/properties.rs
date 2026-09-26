//! Compatibility helpers for block gameplay properties.
//!
//! Block-specific values live in the block family definitions; this module keeps the
//! established query API and the shared calculations used by callers.

use super::definition;
use super::id::Id;

/// Whether a block fully occludes its neighbours, mirroring the reference's
/// `Block.opaqueCubeLookup`.
pub fn is_opaque_cube(block: Id) -> bool {
    definition::properties(block).opaque_cube
}

/// Whether an entity AABB should collide with this block.
pub fn blocks_movement(block: Id) -> bool {
    definition::properties(block).blocks_movement
}

/// Surface slipperiness used by Beta's living-entity ground acceleration and drag.
pub fn slipperiness(block: Id) -> f32 {
    definition::properties(block).slipperiness
}

/// Local collision bounds for a block, if it collides with entities.
pub fn collision_bounds(block: Id) -> Option<([f32; 3], [f32; 3])> {
    definition::properties(block).collision_bounds
}

/// Crossed sprites with no collision: flowers, mushrooms, plants, and reeds.
pub fn is_crossed_plant(block: Id) -> bool {
    definition::properties(block).crossed_plant
}

/// `BlockFlower.canThisPlantGrowOnThisBlockID`.
pub fn plant_grows_on(block: Id) -> bool {
    matches!(block, Id::Grass | Id::Dirt | Id::Farmland)
}

/// `BlockCactus.canBlockStay`: cactus may grow on sand or another cactus,
/// provided each horizontal neighbour has a non-solid material.
pub fn cactus_can_stay(below: Id, neighbors: [Id; 4]) -> bool {
    matches!(below, Id::Sand | Id::Cactus)
        && neighbors
            .into_iter()
            .all(|block| !has_solid_material(block))
}

/// Beta reed placement/growth rule. A cane segment stacks on another segment;
/// the bottom segment needs soil beside water on the same supporting level.
pub fn sugar_cane_can_stay(below: Id, adjacent_water: [bool; 4]) -> bool {
    below == Id::SugarCane
        || (matches!(below, Id::Grass | Id::Dirt | Id::Sand)
            && adjacent_water.into_iter().any(|is_water| is_water))
}

/// Beta's `Material.isSolid`, used for cactus clearance. Transparent glass and
/// ice still count as solid materials; fluids and logic/plant blocks do not.
fn has_solid_material(block: Id) -> bool {
    if block.is_ladder() {
        return false;
    }
    !matches!(
        block,
        Id::Air
            | Id::Water
            | Id::FlowingWater
            | Id::Lava
            | Id::FlowingLava
            | Id::Torch
            | Id::TorchWest
            | Id::TorchEast
            | Id::TorchNorth
            | Id::TorchSouth
            | Id::DeadBush
            | Id::TallGrass
            | Id::Dandelion
            | Id::Rose
            | Id::Fern
            | Id::BrownMushroom
            | Id::RedMushroom
            | Id::Fire
            | Id::RedstoneWire
            | Id::Crops
            | Id::Sapling
            | Id::Rail
            | Id::PoweredRail
            | Id::DetectorRail
            | Id::Ladder
            | Id::Lever
            | Id::StonePressurePlate
            | Id::WoodenPressurePlate
            | Id::UnlitRedstoneTorch
            | Id::RedstoneTorch
            | Id::StoneButton
            | Id::SugarCane
            | Id::SnowLayer
            | Id::Repeater
            | Id::PoweredRepeater
            | Id::NetherPortal
    )
}

pub fn is_torch(block: Id) -> bool {
    definition::properties(block).torch
}

/// Rotate the floor torch's local geometry into its wall pose. This remains a
/// compatibility helper for the mesher; picking bounds are supplied by the
/// torch definition in the block family module.
pub fn torch_point(block: Id, point: [f32; 3]) -> [f32; 3] {
    if block == Id::Torch {
        return point;
    }
    let [x, y, z] = point;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        Id::TorchWest => [
            -0.04 + (x - 0.5) * cos + y * sin,
            0.34 - (x - 0.5) * sin + y * cos,
            z,
        ],
        Id::TorchEast => [
            1.04 + (x - 0.5) * cos - y * sin,
            0.34 + (x - 0.5) * sin + y * cos,
            z,
        ],
        Id::TorchNorth => [
            x,
            0.34 - (z - 0.5) * sin + y * cos,
            -0.04 + (z - 0.5) * cos + y * sin,
        ],
        Id::TorchSouth => [
            x,
            0.34 + (z - 0.5) * sin + y * cos,
            1.04 + (z - 0.5) * cos - y * sin,
        ],
        _ => point,
    }
}

pub fn torch_normal(block: Id, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        Id::TorchWest => [x * cos + y * sin, -x * sin + y * cos, z],
        Id::TorchEast => [x * cos - y * sin, x * sin + y * cos, z],
        Id::TorchNorth => [x, -z * sin + y * cos, z * cos + y * sin],
        Id::TorchSouth => [x, z * sin + y * cos, z * cos - y * sin],
        _ => normal,
    }
}

pub(crate) fn torch_selection_bounds(block: Id) -> ([f32; 3], [f32; 3]) {
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

/// Local bounds used for picking and the hover outline.
pub fn selection_bounds(block: Id) -> ([f32; 3], [f32; 3]) {
    definition::properties(block).selection_bounds
}

/// Whether the player's block selection ray should stop on this block.
pub fn is_targetable(block: Id) -> bool {
    definition::properties(block).targetable
}

/// Whether a placed block may replace this cell.
pub fn is_replaceable(block: Id) -> bool {
    definition::properties(block).replaceable
}

/// Whether the player may mine this block. Bedrock is unbreakable.
pub fn is_breakable(block: Id) -> bool {
    let properties = definition::properties(block);
    properties.targetable && properties.hardness >= 0.0
}

/// Beta `Block.blockHardness`. Negative means unbreakable (`setBlockUnbreakable`).
pub fn hardness(block: Id) -> f32 {
    definition::properties(block).hardness
}

/// Empty-hand `InventoryPlayer.canHarvestBlock`.
pub fn harvestable_by_hand(block: Id) -> bool {
    definition::properties(block).harvestable_by_hand
}

/// Damage added each game tick, matching `Block.blockStrength`.
///
/// `strength` is the held item's `getStrVsBlock` before the water and airborne
/// penalties. Those penalties and the `/ 30` divisor apply only when the block
/// can be harvested. Otherwise the tool is ignored and the step is `/ 100`.
/// Hardness `0` is an instant break.
pub fn mine_progress_per_tick(
    block: Id,
    strength: f32,
    can_harvest: bool,
    on_ground: bool,
    in_water: bool,
) -> f32 {
    let hardness = hardness(block);
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
pub fn hand_mine_progress_per_tick(block: Id, on_ground: bool, in_water: bool) -> f32 {
    mine_progress_per_tick(block, 1.0, harvestable_by_hand(block), on_ground, in_water)
}

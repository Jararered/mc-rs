//! Compatibility helpers for block gameplay properties.
//!
//! Block-specific values live in the block family definitions; this module keeps the
//! established query API and the shared calculations used by callers.

use super::blocks::Block;
use super::definition;

/// Whether a block fully occludes its neighbours, mirroring the reference's
/// `Block.opaqueCubeLookup`.
pub fn is_opaque_cube(block: Block) -> bool {
    definition::properties(block).opaque_cube
}

/// Whether an entity AABB should collide with this block.
pub fn blocks_movement(block: Block) -> bool {
    definition::properties(block).blocks_movement
}

/// Surface slipperiness used by Beta's living-entity ground acceleration and drag.
pub fn slipperiness(block: Block) -> f32 {
    definition::properties(block).slipperiness
}

/// Local collision bounds for a block, if it collides with entities.
pub fn collision_bounds(block: Block) -> Option<([f32; 3], [f32; 3])> {
    definition::properties(block).collision_bounds
}

/// Crossed sprites with no collision: flowers, mushrooms, plants, and reeds.
pub fn is_crossed_plant(block: Block) -> bool {
    definition::properties(block).crossed_plant
}

/// `BlockFlower.canThisPlantGrowOnThisBlockID`.
pub fn plant_grows_on(block: Block) -> bool {
    matches!(block, Block::Grass | Block::Dirt | Block::Farmland)
}

/// `BlockCactus.canBlockStay`: cactus may grow on sand or another cactus,
/// provided each horizontal neighbour has a non-solid material.
pub fn cactus_can_stay(below: Block, neighbors: [Block; 4]) -> bool {
    matches!(below, Block::Sand | Block::Cactus)
        && neighbors.into_iter().all(|block| !is_solid_material(block))
}

/// Beta reed placement/growth rule. A cane segment stacks on another segment;
/// the bottom segment needs soil beside water on the same supporting level.
pub fn sugar_cane_can_stay(below: Block, adjacent_water: [bool; 4]) -> bool {
    below == Block::SugarCane
        || (matches!(below, Block::Grass | Block::Dirt | Block::Sand)
            && adjacent_water.into_iter().any(|is_water| is_water))
}

/// Beta's `Material.isSolid`. Transparent glass, ice, and leaves still count as
/// solid materials; air, fluids, fire, and logic/plant blocks do not.
pub fn is_solid_material(block: Block) -> bool {
    if block.is_ladder() {
        return false;
    }
    !matches!(
        block,
        Block::Air
            | Block::Water
            | Block::FlowingWater
            | Block::Lava
            | Block::FlowingLava
            | Block::Torch
            | Block::TorchWest
            | Block::TorchEast
            | Block::TorchNorth
            | Block::TorchSouth
            | Block::DeadBush
            | Block::TallGrass
            | Block::Dandelion
            | Block::Rose
            | Block::Fern
            | Block::BrownMushroom
            | Block::RedMushroom
            | Block::Fire
            | Block::RedstoneWire
            | Block::Crops
            | Block::Sapling
            | Block::Rail
            | Block::PoweredRail
            | Block::DetectorRail
            | Block::Ladder
            | Block::Lever
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::UnlitRedstoneTorch
            | Block::RedstoneTorch
            | Block::StoneButton
            | Block::SugarCane
            | Block::SnowLayer
            | Block::Repeater
            | Block::PoweredRepeater
            | Block::NetherPortal
    )
}

pub fn is_torch(block: Block) -> bool {
    definition::properties(block).torch
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

/// Local bounds used for picking and the hover outline.
pub fn selection_bounds(block: Block) -> ([f32; 3], [f32; 3]) {
    definition::properties(block).selection_bounds
}

/// Whether the player's block selection ray should stop on this block.
pub fn is_targetable(block: Block) -> bool {
    definition::properties(block).targetable
}

/// Whether a placed block may replace this cell.
pub fn is_replaceable(block: Block) -> bool {
    definition::properties(block).replaceable
}

/// Whether the player may mine this block. Bedrock is unbreakable.
pub fn is_breakable(block: Block) -> bool {
    let properties = definition::properties(block);
    properties.targetable && properties.hardness >= 0.0
}

/// Beta `Block.blockHardness`. Negative means unbreakable (`setBlockUnbreakable`).
pub fn hardness(block: Block) -> f32 {
    definition::properties(block).hardness
}

/// `Block.getExplosionResistance`: `blockResistance / 5`. `setHardness` raises
/// the resistance to five times the hardness, and an explicit `setResistance`
/// afterwards replaces it with three times its argument. Stairs copy their
/// material's resistance. Still lava keeps Beta's hardness of 100.
pub fn explosion_resistance(block: Block) -> f32 {
    let explicit = match block.item_form().0 {
        Block::Bedrock => Some(6_000_000.0),
        Block::Obsidian => Some(2000.0),
        Block::Stone
        | Block::Cobblestone
        | Block::GoldBlock
        | Block::IronBlock
        | Block::DoubleStoneSlab
        | Block::StoneSlab
        | Block::Bricks
        | Block::MossyCobblestone
        | Block::DiamondBlock
        | Block::Jukebox
        | Block::CobblestoneStairs => Some(10.0),
        Block::WoodenPlanks
        | Block::GoldOre
        | Block::IronOre
        | Block::CoalOre
        | Block::LapisOre
        | Block::LapisBlock
        | Block::DiamondOre
        | Block::RedstoneOre
        | Block::LitRedstoneOre
        | Block::Fence
        | Block::WoodenStairs => Some(5.0),
        Block::Lava => return 100.0,
        _ => None,
    };
    explicit.map_or_else(|| hardness(block).max(0.0), |resistance| resistance * 0.6)
}

/// Empty-hand `InventoryPlayer.canHarvestBlock`.
pub fn harvestable_by_hand(block: Block) -> bool {
    definition::properties(block).harvestable_by_hand
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
pub fn hand_mine_progress_per_tick(block: Block, on_ground: bool, in_water: bool) -> f32 {
    mine_progress_per_tick(block, 1.0, harvestable_by_hand(block), on_ground, in_water)
}

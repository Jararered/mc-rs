use super::block::BlockId;

/// Whether a block fully occludes its neighbours, mirroring the reference's
/// `Block.opaqueCubeLookup`.
///
/// Tree generation uses this to decide whether a leaf may replace a block.
/// Leaves are treated as non-opaque (the reference's fancy-graphics behaviour),
/// so canopies stay dense where they overlap.
pub fn is_opaque_cube(block: BlockId) -> bool {
    !matches!(
        block,
        BlockId::Air
            | BlockId::Water
            | BlockId::Ice
            | BlockId::Leaves
            | BlockId::SpruceLeaves
            | BlockId::BirchLeaves
            | BlockId::Torch
            | BlockId::TorchWest
            | BlockId::TorchEast
            | BlockId::TorchNorth
            | BlockId::TorchSouth
    )
}

/// Whether an entity AABB should collide with this block.
///
/// Fluids have no collision box in Beta (`getCollisionBoundingBoxFromPool`
/// returns null). Everything else currently in the registry is a full cube.
pub fn blocks_movement(block: BlockId) -> bool {
    !matches!(block, BlockId::Air | BlockId::Water) && !is_torch(block)
}

pub fn is_torch(block: BlockId) -> bool {
    matches!(
        block,
        BlockId::Torch
            | BlockId::TorchWest
            | BlockId::TorchEast
            | BlockId::TorchNorth
            | BlockId::TorchSouth
    )
}

/// Rotate the floor torch's local geometry into its wall pose. The base sits
/// partly inside the supporting face and the whole post, including its cap,
/// tilts toward the center of the cell.
pub fn torch_point(block: BlockId, point: [f32; 3]) -> [f32; 3] {
    if block == BlockId::Torch {
        return point;
    }
    let [x, y, z] = point;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        BlockId::TorchWest => [
            -0.04 + (x - 0.5) * cos + y * sin,
            0.34 - (x - 0.5) * sin + y * cos,
            z,
        ],
        BlockId::TorchEast => [
            1.04 + (x - 0.5) * cos - y * sin,
            0.34 + (x - 0.5) * sin + y * cos,
            z,
        ],
        BlockId::TorchNorth => [
            x,
            0.34 - (z - 0.5) * sin + y * cos,
            -0.04 + (z - 0.5) * cos + y * sin,
        ],
        BlockId::TorchSouth => [
            x,
            0.34 + (z - 0.5) * sin + y * cos,
            1.04 + (z - 0.5) * cos - y * sin,
        ],
        _ => point,
    }
}

pub fn torch_normal(block: BlockId, normal: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = normal;
    let sin = 0.55_f32;
    let cos = (1.0 - sin * sin).sqrt();
    match block {
        BlockId::TorchWest => [x * cos + y * sin, -x * sin + y * cos, z],
        BlockId::TorchEast => [x * cos - y * sin, x * sin + y * cos, z],
        BlockId::TorchNorth => [x, -z * sin + y * cos, z * cos + y * sin],
        BlockId::TorchSouth => [x, z * sin + y * cos, z * cos - y * sin],
        _ => normal,
    }
}

/// Local bounds used for picking and the hover outline.
pub fn selection_bounds(block: BlockId) -> ([f32; 3], [f32; 3]) {
    if !is_torch(block) {
        return ([0.0; 3], [1.0; 3]);
    }
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

/// Whether a pick ray should stop on this block.
///
/// Water is skipped unless the ray is a bucket trace (`canCollideCheck` is
/// false for fluids when `stopOnLiquid` is false).
pub fn is_targetable(block: BlockId) -> bool {
    !matches!(block, BlockId::Air | BlockId::Water)
}

/// Whether a placed block may replace this cell.
pub fn is_replaceable(block: BlockId) -> bool {
    matches!(block, BlockId::Air | BlockId::Water)
}

/// Whether the player may mine this block. Bedrock is unbreakable.
pub fn is_breakable(block: BlockId) -> bool {
    is_targetable(block) && hardness(block) >= 0.0
}

/// Beta `Block.blockHardness`. Negative means unbreakable (`setBlockUnbreakable`).
pub fn hardness(block: BlockId) -> f32 {
    match block {
        BlockId::Air => 0.0,
        BlockId::Torch
        | BlockId::TorchWest
        | BlockId::TorchEast
        | BlockId::TorchNorth
        | BlockId::TorchSouth => 0.0,
        BlockId::Stone => 1.5,
        BlockId::Grass => 0.6,
        BlockId::Dirt => 0.5,
        BlockId::Cobblestone => 2.0,
        BlockId::WoodenPlanks => 2.0,
        BlockId::Bedrock => -1.0,
        BlockId::Water => 100.0,
        BlockId::Sand => 0.5,
        BlockId::Gravel => 0.6,
        BlockId::GoldOre => 3.0,
        BlockId::IronOre => 3.0,
        BlockId::CoalOre => 3.0,
        BlockId::Wood | BlockId::SpruceWood | BlockId::BirchWood => 2.0,
        BlockId::Leaves | BlockId::SpruceLeaves | BlockId::BirchLeaves => 0.2,
        BlockId::Sponge => 0.6,
        BlockId::LapisOre => 3.0,
        BlockId::LapisBlock => 3.0,
        BlockId::Dispenser => 3.5,
        BlockId::Sandstone => 0.8,
        BlockId::NoteBlock => 0.8,
        BlockId::Wool => 0.8,
        BlockId::GoldBlock => 3.0,
        BlockId::IronBlock => 5.0,
        BlockId::DoubleStoneSlab => 2.0,
        BlockId::Bricks => 2.0,
        BlockId::Tnt => 0.0,
        BlockId::Bookshelf => 1.5,
        BlockId::MossyCobblestone => 2.0,
        BlockId::Obsidian => 10.0,
        BlockId::DiamondOre => 3.0,
        BlockId::DiamondBlock => 5.0,
        BlockId::CraftingTable => 2.5,
        BlockId::Furnace | BlockId::LitFurnace => 3.5,
        BlockId::RedstoneOre | BlockId::LitRedstoneOre => 3.0,
        BlockId::Ice => 0.5,
        BlockId::Snow => 0.2,
        BlockId::Clay => 0.6,
        BlockId::Jukebox => 2.0,
        BlockId::Pumpkin | BlockId::JackOLantern => 1.0,
        BlockId::Netherrack => 0.4,
        BlockId::Glowstone => 0.3,
        // Catalog blocks are not placed, so this value is never sampled.
        BlockId::Sapling
        | BlockId::FlowingWater
        | BlockId::FlowingLava
        | BlockId::Lava
        | BlockId::Glass
        | BlockId::Bed
        | BlockId::PoweredRail
        | BlockId::DetectorRail
        | BlockId::StickyPiston
        | BlockId::Cobweb
        | BlockId::TallGrass
        | BlockId::DeadBush
        | BlockId::Piston
        | BlockId::PistonHead
        | BlockId::MovingPiston
        | BlockId::Dandelion
        | BlockId::Rose
        | BlockId::BrownMushroom
        | BlockId::RedMushroom
        | BlockId::StoneSlab
        | BlockId::Fire
        | BlockId::MobSpawner
        | BlockId::WoodenStairs
        | BlockId::Chest
        | BlockId::RedstoneWire
        | BlockId::Crops
        | BlockId::Farmland
        | BlockId::StandingSign
        | BlockId::WoodenDoor
        | BlockId::Ladder
        | BlockId::Rail
        | BlockId::CobblestoneStairs
        | BlockId::WallSign
        | BlockId::Lever
        | BlockId::StonePressurePlate
        | BlockId::IronDoor
        | BlockId::WoodenPressurePlate
        | BlockId::UnlitRedstoneTorch
        | BlockId::RedstoneTorch
        | BlockId::StoneButton
        | BlockId::SnowLayer
        | BlockId::Cactus
        | BlockId::SugarCane
        | BlockId::Fence
        | BlockId::SoulSand
        | BlockId::NetherPortal
        | BlockId::Cake
        | BlockId::Repeater
        | BlockId::PoweredRepeater
        | BlockId::LockedChest
        | BlockId::Trapdoor => 0.0,
    }
}

/// Empty-hand `InventoryPlayer.canHarvestBlock`: true unless the material used
/// `setNoHarvest` (rock, iron, snow block).
pub fn harvestable_by_hand(block: BlockId) -> bool {
    !matches!(
        block,
        BlockId::Stone
            | BlockId::Cobblestone
            | BlockId::Bedrock
            | BlockId::GoldOre
            | BlockId::IronOre
            | BlockId::CoalOre
            | BlockId::LapisOre
            | BlockId::LapisBlock
            | BlockId::Dispenser
            | BlockId::Sandstone
            | BlockId::GoldBlock
            | BlockId::IronBlock
            | BlockId::DoubleStoneSlab
            | BlockId::Bricks
            | BlockId::MossyCobblestone
            | BlockId::Obsidian
            | BlockId::DiamondOre
            | BlockId::DiamondBlock
            | BlockId::Furnace
            | BlockId::LitFurnace
            | BlockId::RedstoneOre
            | BlockId::LitRedstoneOre
            | BlockId::Snow
            | BlockId::Netherrack
            | BlockId::Glowstone
    )
}

/// Damage added each game tick, matching `Block.blockStrength`.
///
/// `strength` is the held item's `getStrVsBlock` before the water and airborne
/// penalties. Those penalties and the `/ 30` divisor apply only when the block
/// can be harvested. Otherwise the tool is ignored and the step is `/ 100`.
/// Hardness `0` is an instant break.
pub fn mine_progress_per_tick(
    block: BlockId,
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
pub fn hand_mine_progress_per_tick(block: BlockId, on_ground: bool, in_water: bool) -> f32 {
    mine_progress_per_tick(block, 1.0, harvestable_by_hand(block), on_ground, in_water)
}

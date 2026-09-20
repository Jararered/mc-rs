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
    )
}

/// Whether an entity AABB should collide with this block.
///
/// Fluids have no collision box in Beta (`getCollisionBoundingBoxFromPool`
/// returns null). Everything else currently in the registry is a full cube.
pub fn blocks_movement(block: BlockId) -> bool {
    !matches!(block, BlockId::Air | BlockId::Water)
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

/// Damage added each game tick while punching, matching `Block.blockStrength`
/// with no tool. Harvestable blocks use `/ 30` and the water / airborne
/// penalties from `getCurrentPlayerStrVsBlock`; everything else uses `/ 100`.
pub fn hand_mine_progress_per_tick(block: BlockId, on_ground: bool, in_water: bool) -> f32 {
    let hardness = hardness(block);
    if hardness < 0.0 {
        return 0.0;
    }
    if hardness == 0.0 {
        return f32::INFINITY;
    }
    if harvestable_by_hand(block) {
        let mut strength = 1.0;
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

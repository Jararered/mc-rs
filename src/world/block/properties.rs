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
    is_targetable(block) && block != BlockId::Bedrock
}

//! Beta `EntityFallingSand`: sand or gravel that lost its support, falling as
//! an entity until it lands and becomes a block again.
//!
//! [`crate::world::block_ticks`] spawns these from `BlockSand.tryToFall`. The
//! entity steps once per world tick, writes the world directly, and reports
//! each write as a [`BlockEvent`](crate::world::block_ticks::BlockEvent) so the tick pass runs Beta's neighbor
//! updates for it on the next frame.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::PreviousTick;
use crate::entity::drops::items::spawn_block_drop;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::WATER_CURRENT_PER_TICK;
use crate::physics::move_entity;
use crate::physics::water_current;
use crate::random::ItemRng;
use crate::rendering::meshing::dropped_block_meshes;
use crate::rendering::textures::TerrainMaterial;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::behaviors::falling::can_fall_below;
use crate::world::block_ticks::behaviors::falling::can_land_in;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

/// `setSize(0.98F, 0.98F)`, centered on the entity position.
const HALF_EXTENT: f32 = 0.49;
const GRAVITY_PER_TICK: f32 = 0.04;
const DRAG: f32 = 0.98;
/// `fallTime > 100` without landing drops the block as an item.
const MAX_FALL_TICKS: u32 = 100;

/// A falling block. `Transform.translation` is the box center, like Beta's
/// `posY` with `yOffset = height / 2`.
#[derive(Component, Clone, Copy, Debug)]
pub struct FallingBlock {
    pub block: Block,
    /// Beta `fallTime`.
    pub fall_ticks: u32,
    /// Blocks per tick.
    pub motion: Vec3,
    pub on_ground: bool,
}

#[derive(Component)]
pub(crate) struct FallingBlockVisual;

/// Spawn a falling block centered in the cell it leaves.
pub fn spawn_falling_block(commands: &mut Commands, position: IVec3, block: Block) {
    let center = position.as_vec3() + Vec3::splat(0.5);
    commands.spawn((
        Name::new("Falling block"),
        FallingBlock {
            block,
            fall_ticks: 0,
            motion: Vec3::ZERO,
            on_ground: false,
        },
        PreviousTick(center),
        Transform::from_translation(center),
        Visibility::default(),
    ));
}

/// What one `EntityFallingSand.onUpdate` did to the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallingStep {
    Falling,
    /// Landed and placed its block at the cell.
    Placed(IVec3),
    /// Landed where it could not be placed, or fell too long; drop the item.
    Dropped(IVec3),
}

/// One `EntityFallingSand.onUpdate`. Removes the source block on the first
/// tick, as Beta does, and writes the landed block. Writes are reported to
/// `ticks` for their neighbor updates, and each written cell is pushed to
/// `edits`.
pub fn step_falling_block(
    falling: &mut FallingBlock,
    center: &mut Vec3,
    chunks: &mut WorldChunks,
    ticks: &mut BlockTicks,
    edits: &mut Vec<IVec3>,
) -> FallingStep {
    falling.fall_ticks += 1;
    let aabb = Aabb::new(
        *center - Vec3::splat(HALF_EXTENT),
        *center + Vec3::splat(HALF_EXTENT),
    );
    falling.motion += water_current(aabb, chunks).1 * WATER_CURRENT_PER_TICK;
    falling.motion.y -= GRAVITY_PER_TICK;
    let movement = move_entity(aabb, falling.motion, 0.0, falling.on_ground, chunks);
    *center = (movement.aabb.min + movement.aabb.max) * 0.5;
    falling.on_ground = movement.collision.on_ground;
    if movement.collision.collided_x {
        falling.motion.x = 0.0;
    }
    if movement.collision.collided_y {
        falling.motion.y = 0.0;
    }
    if movement.collision.collided_z {
        falling.motion.z = 0.0;
    }
    falling.motion *= DRAG;

    let cell = center.floor().as_ivec3();
    if chunks.block_at(cell.x, cell.y, cell.z) == Some(falling.block) {
        write_block(chunks, ticks, edits, cell, Block::Air);
    }
    if falling.on_ground {
        falling.motion.x *= 0.7;
        falling.motion.z *= 0.7;
        falling.motion.y *= -0.5;
        let placeable = chunks
            .block_at(cell.x, cell.y, cell.z)
            .is_some_and(can_land_in);
        let below = chunks
            .block_at(cell.x, cell.y - 1, cell.z)
            .unwrap_or(Block::Air);
        if placeable
            && !can_fall_below(below)
            && write_block(chunks, ticks, edits, cell, falling.block)
        {
            return FallingStep::Placed(cell);
        }
        return FallingStep::Dropped(cell);
    }
    if falling.fall_ticks > MAX_FALL_TICKS {
        return FallingStep::Dropped(cell);
    }
    FallingStep::Falling
}

/// `World.setBlockWithNotify` from outside the tick pass.
fn write_block(
    chunks: &mut WorldChunks,
    ticks: &mut BlockTicks,
    edits: &mut Vec<IVec3>,
    cell: IVec3,
    block: Block,
) -> bool {
    let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
    let Some(previous) = chunks.set_block(cell.x, cell.y, cell.z, block) else {
        return false;
    };
    if previous == block {
        return false;
    }
    ticks.block_changed(cell, previous, metadata);
    edits.push(cell);
    true
}

pub(crate) fn tick_falling_blocks(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: ResMut<BlockTicks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut falling: Query<(Entity, &mut FallingBlock, &mut Transform, &mut PreviousTick)>,
    mut rng: Local<ItemRng>,
    mut edits: Local<Vec<IVec3>>,
) {
    let count = tick.ticks_this_frame();
    if count == 0 {
        return;
    }
    edits.clear();
    for (entity, mut block, mut transform, mut previous) in &mut falling {
        let position = transform.translation;
        if !chunks.contains(ChunkPosition::from_world(position.x, position.z)) {
            continue;
        }
        let mut center = position;
        let mut outcome = FallingStep::Falling;
        for _ in 0..count {
            previous.0 = center;
            outcome =
                step_falling_block(&mut block, &mut center, &mut chunks, &mut ticks, &mut edits);
            if outcome != FallingStep::Falling {
                break;
            }
        }
        transform.translation = center;
        match outcome {
            FallingStep::Falling => {}
            FallingStep::Placed(_) => commands.entity(entity).despawn(),
            FallingStep::Dropped(cell) => {
                // `dropItem(blockID, 1)`: the block itself, not its harvest drop.
                if let Ok(stack) = ItemStack::from_block(block.block, 1) {
                    spawn_block_drop(&mut commands, &mut rng, cell, stack);
                }
                commands.entity(entity).despawn();
            }
        }
    }
    for cell in edits.drain(..) {
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
        }
        if let Some(streaming) = streaming.as_deref_mut() {
            streaming.request_block_update(cell.x, cell.y, cell.z);
        }
    }
}

/// Give each new falling block the world cube mesh, and slide it between
/// ticks like other entities.
pub(crate) fn sync_falling_block_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    terrain: Option<Res<TerrainMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    new_blocks: Query<(Entity, &FallingBlock), Without<FallingBlockVisual>>,
    mut visuals: Query<
        (&Transform, &PreviousTick, &Children),
        (With<FallingBlock>, With<FallingBlockVisual>),
    >,
    mut pieces: Query<&mut Transform, Without<FallingBlock>>,
) {
    let Some(terrain) = terrain else {
        return;
    };
    let fancy = settings.is_some_and(|settings| settings.graphics.fancy_leaves());
    for (entity, falling) in &new_blocks {
        let built =
            dropped_block_meshes(falling.block, fancy, [0.55, 0.8, 0.4], [0.28, 0.71, 0.09]);
        let child = commands
            .spawn((
                Mesh3d(meshes.add(built.body.into_mesh())),
                MeshMaterial3d(terrain.0.clone()),
                Transform::default(),
                NoFrustumCulling,
            ))
            .id();
        commands
            .entity(entity)
            .add_child(child)
            .insert(FallingBlockVisual);
    }
    for (transform, previous, children) in &mut visuals {
        let slide = previous.0.lerp(transform.translation, tick.partial()) - transform.translation;
        for child in children.iter() {
            if let Ok(mut piece) = pieces.get_mut(child) {
                piece.translation = slide;
            }
        }
    }
}

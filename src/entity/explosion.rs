//! Beta 1.7.3 `Explosion`: creepers, TNT, and ghast fireballs.
//!
//! `doExplosionA` casts 1352 rays from the center. Each starts with
//! `0.7..1.3` times the strength and loses `(resistance + 0.3) * 0.3` for
//! each solid block it passes and `0.225` per step, marking every cell it
//! reaches with power to spare. Bodies within twice the strength take damage
//! and are flung by how exposed they are: the share of sample points on their
//! box with a clear line to the center. A flaming blast lights a third of the
//! marked air cells that sit on an opaque block. `doExplosionB` then removes
//! every marked block, dropping each item with a 30% chance, and lights any
//! TNT it reaches on a short fuse.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Gravity;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::Source;
use crate::entity::combat::drop_loot;
use crate::entity::combat::hurt_creature;
use crate::entity::combat::hurt_player;
use crate::entity::creature::Living;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::mobs::Mob;
use crate::entity::projectiles::victim_of;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::raycast_blocks;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::rendering::particles::effects::EffectParticles;
use crate::rendering::particles::effects::FxKind;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::difficulty::Difficulty;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;

/// `World.newExplosion`.
#[derive(Message, Clone, Copy, Debug)]
pub struct Explosion {
    pub center: Vec3,
    pub strength: f32,
    /// Ghast fireballs set fires.
    pub flaming: bool,
    /// The exploder: a creeper is a monster, so difficulty scales its blast
    /// against the player. TNT and fireballs have none.
    pub source: Source,
}

/// `EntityTNTPrimed`. Its `Transform` is the bottom center of its box.
#[derive(Component)]
pub struct PrimedTnt {
    pub fuse: u16,
}

/// `EntityTNTPrimed.setSize(0.98, 0.98)`.
const TNT_SIZE: EntitySize = EntitySize {
    width: 0.98,
    height: 0.98,
    y_offset: 0.0,
};

pub fn prime_tnt(commands: &mut Commands, position: Vec3, fuse: u16) -> Entity {
    commands
        .spawn((
            PrimedTnt { fuse },
            Transform::from_translation(position),
            TNT_SIZE,
            Velocity::default(),
            Gravity::DEFAULT,
            CollisionState::default(),
            StepHeight(0.0),
        ))
        .id()
}

pub(crate) fn tick_tnt(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut tnt: Query<(Entity, &mut PrimedTnt, &Transform)>,
    mut explosions: MessageWriter<Explosion>,
    mut particles: Option<ResMut<EffectParticles>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    for (entity, mut tnt, transform) in &mut tnt {
        if u32::from(tnt.fuse) > ticks {
            tnt.fuse -= ticks as u16;
            if let Some(particles) = particles.as_deref_mut() {
                // `EntityTNTPrimed.onUpdate`: a puff above it every tick.
                for _ in 0..ticks {
                    particles.spawn(
                        FxKind::Smoke,
                        transform.translation + Vec3::Y * 0.5,
                        Vec3::ZERO,
                    );
                }
            }
        } else {
            explosions.write(Explosion {
                center: transform.translation,
                strength: 4.0,
                flaming: false,
                source: Source::Environment,
            });
            commands.entity(entity).despawn();
        }
    }
}

/// `World.rand` and `Explosion.ExplosionRNG`.
#[derive(Resource)]
pub(crate) struct ExplosionRandom(JavaRandom);

impl Default for ExplosionRandom {
    fn default() -> Self {
        Self(JavaRandom::new(0x4254_4e54))
    }
}

/// The cells a blast destroys, in the order its rays first reach them.
pub fn blast_cells(
    chunks: &WorldChunks,
    center: Vec3,
    strength: f32,
    rng: &mut JavaRandom,
) -> Vec<IVec3> {
    const RAYS: i32 = 16;
    let mut seen = HashSet::new();
    let mut cells = Vec::new();
    for i in 0..RAYS {
        for j in 0..RAYS {
            for k in 0..RAYS {
                let edge = |n: i32| n == 0 || n == RAYS - 1;
                if !(edge(i) || edge(j) || edge(k)) {
                    continue;
                }
                let along = |n: i32| n as f32 / (RAYS as f32 - 1.0) * 2.0 - 1.0;
                let direction = Vec3::new(along(i), along(j), along(k)).normalize();
                let mut power = strength * (0.7 + rng.next_float() * 0.6);
                let mut at = center;
                const STEP: f32 = 0.3;
                while power > 0.0 {
                    let cell = at.floor().as_ivec3();
                    let block = chunks
                        .block_at(cell.x, cell.y, cell.z)
                        .unwrap_or(Block::Air);
                    if block != Block::Air {
                        power -= (block.explosion_resistance() + 0.3) * STEP;
                    }
                    if power > 0.0 && seen.insert(cell) {
                        cells.push(cell);
                    }
                    at += direction * STEP;
                    power -= STEP * 0.75;
                }
            }
        }
    }
    cells
}

/// `World.getBlockDensity`: the share of sample points across `aabb` with a
/// clear line to `center`.
pub fn exposure(chunks: &WorldChunks, center: Vec3, aabb: Aabb) -> f32 {
    let size = aabb.max - aabb.min;
    let step = Vec3::ONE / (size * 2.0 + Vec3::ONE);
    let mut clear = 0;
    let mut total = 0;
    let mut x = 0.0_f32;
    while x <= 1.0 {
        let mut y = 0.0_f32;
        while y <= 1.0 {
            let mut z = 0.0_f32;
            while z <= 1.0 {
                let sample = aabb.min + size * Vec3::new(x, y, z);
                let toward = center - sample;
                if raycast_blocks(chunks, sample, toward, toward.length()).is_none() {
                    clear += 1;
                }
                total += 1;
                z += step.z;
            }
            y += step.y;
        }
        x += step.x;
    }
    clear as f32 / total as f32
}

/// The damage and fling a body `distance` from the center takes, where
/// `reach` is twice the strength.
fn impact(
    chunks: &WorldChunks,
    center: Vec3,
    position: Vec3,
    aabb: Aabb,
    reach: f32,
) -> Option<(i16, Vec3)> {
    let distance = position.distance(center) / reach;
    if distance > 1.0 {
        return None;
    }
    let direction = (position - center).normalize_or_zero();
    let impact = (1.0 - distance) * exposure(chunks, center, aabb);
    let damage = ((impact * impact + impact) / 2.0 * 8.0 * reach + 1.0) as i16;
    Some((damage, direction * impact))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_explosions(
    mut commands: Commands,
    mut blasts: MessageReader<Explosion>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    settings: Option<Res<GameSettings>>,
    mut mobs: Query<
        (
            &mut Mob,
            &mut Living,
            &mut Velocity,
            &Transform,
            &EntitySize,
        ),
        Without<Player>,
    >,
    mut player: Query<
        (
            &Transform,
            Option<&mut PlayerHealth>,
            Option<&mut PlayerCombat>,
            &mut Velocity,
            Option<&mut Inventory>,
        ),
        With<Player>,
    >,
    mut tnt: Query<
        (&Transform, &mut Velocity),
        (With<PrimedTnt>, Without<Living>, Without<Player>),
    >,
    mut rng: ResMut<ExplosionRandom>,
    mut particles: Option<ResMut<EffectParticles>>,
    mut loot: Local<ItemRng>,
    mut spare_armor: Local<[Option<ItemStack>; 4]>,
) {
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |settings| settings.difficulty);
    for blast in blasts.read().copied().collect::<Vec<_>>() {
        let center = blast.center;
        let cells = blast_cells(&chunks, center, blast.strength, &mut rng.0);
        let reach = blast.strength * 2.0;
        let hit = |damage: i16| Hit {
            amount: damage,
            from: (blast.source != Source::Environment).then_some(center),
            source: blast.source,
        };
        let within = Aabb::new(
            (center - Vec3::splat(reach + 1.0)).floor(),
            (center + Vec3::splat(reach + 1.0)).floor(),
        );

        for (mut mob, mut living, mut velocity, transform, size) in &mut mobs {
            let feet = transform.translation;
            let aabb = size.aabb(feet);
            if !within.intersects(aabb) {
                continue;
            }
            if let Some((damage, fling)) = impact(&chunks, center, feet, aabb, reach) {
                let wound = hurt_creature(&mut mob, &mut living, &mut velocity, feet, hit(damage));
                if wound.died {
                    drop_loot(&mut commands, &mut loot, &mut mob, feet);
                }
                velocity.0 += fling / TICK_SECONDS;
            }
        }
        for mut parts in &mut player {
            let eye = parts.0.translation;
            let aabb = EntitySize::PLAYER.aabb(eye);
            if within.intersects(aabb)
                && let Some((damage, fling)) = impact(&chunks, center, eye, aabb, reach)
            {
                if let Some(mut victim) = victim_of(&mut parts, &mut spare_armor) {
                    hurt_player(&mut victim, hit(damage), difficulty, &mut loot);
                }
                parts.3.0 += fling / TICK_SECONDS;
            }
        }
        for (transform, mut velocity) in &mut tnt {
            let aabb = TNT_SIZE.aabb(transform.translation);
            if within.intersects(aabb)
                && let Some((_, fling)) =
                    impact(&chunks, center, transform.translation, aabb, reach)
            {
                velocity.0 += fling / TICK_SECONDS;
            }
        }

        let mut changed = Vec::new();
        if blast.flaming {
            for &cell in cells.iter().rev() {
                if chunks.block_at(cell.x, cell.y, cell.z) == Some(Block::Air)
                    && chunks
                        .block_at(cell.x, cell.y - 1, cell.z)
                        .is_some_and(Block::is_opaque_cube)
                    && rng.0.next_int(3) == 0
                    && let Some(previous) = chunks.set_block(cell.x, cell.y, cell.z, Block::Fire)
                {
                    changed.push((cell, previous, 0));
                }
            }
        }
        for &cell in cells.iter().rev() {
            if let Some(particles) = particles.as_deref_mut() {
                particles.blast_cell(cell, center, blast.strength);
            }
            let Some(block) = chunks.block_at(cell.x, cell.y, cell.z) else {
                continue;
            };
            if block == Block::Air {
                continue;
            }
            let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
            // `onBlockRemoval`: a container spills everything it held. Writing
            // air discards the slots, so read them first.
            if let Some(furnace) = chunks.furnace_at(cell.x, cell.y, cell.z) {
                for stack in furnace.slots.into_iter().flatten() {
                    spawn_block_drop(&mut commands, &mut loot, cell, stack);
                }
            }
            if let Some(chest) = chunks.chest_at(cell.x, cell.y, cell.z) {
                let stacks: Vec<_> = chest.slots.into_iter().flatten().collect();
                spawn_chest_drops(&mut commands, &mut loot, cell, stacks);
            }
            if let Some(dispenser) = chunks.dispenser_at(cell.x, cell.y, cell.z) {
                let stacks: Vec<_> = dispenser.slots.into_iter().flatten().collect();
                spawn_chest_drops(&mut commands, &mut loot, cell, stacks);
            }
            for stack in natural_drops_with_metadata(block, metadata, &mut *loot) {
                if rng.0.next_float() <= 0.3 {
                    spawn_block_drop(&mut commands, &mut loot, cell, stack);
                }
            }
            if let Some(previous) = chunks.set_block(cell.x, cell.y, cell.z, Block::Air) {
                changed.push((cell, previous, metadata));
            }
            // `BlockTNT.onBlockDestroyedByExplosion`: a short, random fuse.
            if block == Block::Tnt {
                let feet = Vec3::new(cell.x as f32 + 0.5, cell.y as f32, cell.z as f32 + 0.5);
                prime_tnt(&mut commands, feet, rng.0.next_int(20) as u16 + 10);
            }
        }
        for (cell, previous, metadata) in changed {
            if let Some(ticks) = ticks.as_deref_mut() {
                ticks.block_changed(cell, previous, metadata);
            }
            if let Some(streaming) = streaming.as_deref_mut() {
                streaming.request_block_update(cell.x, cell.y, cell.z);
            }
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
            }
        }
    }
}

//! Beta 1.7.3 `EntityFish`: the fishing rod's bobber.
//!
//! Cast from the hand like a snowball, it floats where it lands in water and
//! waits for a bite: one chance in 500 each tick, 300 where rain reaches it.
//! A bite pulls it under for 10 to 39 ticks, and reeling in during that time
//! flings a raw fish at the angler. It also hooks the first mob it touches,
//! which reeling in then drags toward the angler, and stays put in a block
//! it strikes. Putting the rod away or walking 32 blocks off drops the line.
//!
//! Beta never records which block a bobber struck, so one that has landed in
//! a block stays there for its full minute even if the block is dug out. The
//! bite's splash sound is not played.

use bevy::prelude::*;

use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::Source;
use crate::entity::combat::hurt_creature;
use crate::entity::creature::Living;
use crate::entity::drops::items::spawn_flung_item;
use crate::entity::mobs::Mob;
use crate::entity::projectiles::Projectile;
use crate::entity::projectiles::Struck;
use crate::entity::projectiles::first_struck;
use crate::entity::projectiles::hand_origin;
use crate::entity::projectiles::scattered_heading;
use crate::inventory::Hotbar;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::move_entity;
use crate::physics::raycast_blocks;
use crate::physics::water_within;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::rendering::particles::effects::EffectParticles;
use crate::world::biome::Biome;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::dimension::Environment;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;

/// `setSize(0.25, 0.25)`.
pub const BOBBER_SIZE: EntitySize = EntitySize {
    width: 0.25,
    height: 0.25,
    y_offset: 0.0,
};

/// Farther than this from its angler, the line snaps (`1024` squared).
const LINE_LENGTH: f32 = 32.0;

/// `EntityPlayer.fishEntity`: the bobber this player has cast.
#[derive(Component, Clone, Copy, Debug)]
pub struct Fishing(pub Entity);

/// `EntityFish`. Its `Transform` is the bottom center of its box.
#[derive(Component, Clone, Debug)]
pub struct Bobber {
    pub angler: Entity,
    /// `motionX/Y/Z`, in blocks per tick.
    pub motion: Vec3,
    /// `bobber`: the mob it has hooked and rides on.
    pub hooked: Option<Entity>,
    /// `inGround`.
    pub in_ground: bool,
    /// Ticks left in which reeling in lands a fish.
    pub ticks_catchable: u32,
    ticks_in_ground: u32,
    collision: CollisionState,
    rng: JavaRandom,
}

/// What `EntityFish.catchFish` does when the line is reeled in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reel {
    /// What the cast costs the rod.
    pub damage: u16,
    /// A raw fish is flung at the angler.
    pub fish: bool,
    /// The hooked mob is dragged toward the angler.
    pub pull: Option<Entity>,
}

impl Bobber {
    /// `EntityFish.catchFish`, without its effects.
    pub fn reel(&self) -> Reel {
        let mut reel = Reel {
            damage: 0,
            fish: false,
            pull: None,
        };
        if let Some(mob) = self.hooked {
            reel.pull = Some(mob);
            reel.damage = 3;
        } else if self.ticks_catchable > 0 {
            reel.fish = true;
            reel.damage = 1;
        }
        if self.in_ground {
            reel.damage = 2;
        }
        reel
    }
}

/// The motion `catchFish` gives what it sends from `from` to the angler at
/// `to`, in blocks per tick.
pub fn reel_motion(from: Vec3, to: Vec3) -> Vec3 {
    let delta = to - from;
    delta * 0.1 + Vec3::Y * delta.length().sqrt() * 0.08
}

/// `new EntityFish(world, player)`: from the hand, along `look`, at 1.5 blocks
/// per tick. The caller records it on the player as [`Fishing`].
pub fn cast_bobber(
    commands: &mut Commands,
    angler: Entity,
    eye: Vec3,
    look: Vec3,
    rng: &mut JavaRandom,
) -> Entity {
    let position = hand_origin(eye, look);
    let bobber = commands
        .spawn((
            Name::new("Fishing bobber"),
            Bobber {
                angler,
                motion: scattered_heading(look, 1.5, 1.0, rng),
                hooked: None,
                in_ground: false,
                ticks_catchable: 0,
                ticks_in_ground: 0,
                collision: CollisionState::default(),
                rng: JavaRandom::new(rng.next_long() as u64),
            },
            Projectile,
            BOBBER_SIZE,
            Transform::from_translation(position),
            PreviousTick(position),
            Visibility::Inherited,
        ))
        .id();
    commands.entity(angler).insert(Fishing(bobber));
    bobber
}

/// Carry out `catchFish` for `bobber` at `position`: fling the fish or drag
/// the hooked mob toward `angler_eye`, and remove the bobber. Returns the
/// damage for the rod.
pub fn reel_in(
    commands: &mut Commands,
    rng: &mut ItemRng,
    entity: Entity,
    bobber: &Bobber,
    position: Vec3,
    angler_eye: Vec3,
    mobs: &mut Query<&mut Velocity, With<Mob>>,
) -> u16 {
    let reel = bobber.reel();
    let motion = reel_motion(position, angler_eye);
    if let Some(mob) = reel.pull
        && let Ok(mut velocity) = mobs.get_mut(mob)
    {
        velocity.0 += motion / TICK_SECONDS;
    }
    if reel.fish
        && let Ok(fish) = ItemStack::new(Item::RawFish, 1)
    {
        spawn_flung_item(commands, rng, position, motion, fish);
    }
    commands.entity(entity).despawn();
    commands.entity(bobber.angler).remove::<Fishing>();
    reel.damage
}

/// How much of the bobber's box is under water, in fifths, as
/// `EntityFish.onUpdate` slices it.
pub fn submerged_fraction(aabb: Aabb, chunks: &WorldChunks) -> f32 {
    let height = aabb.max.y - aabb.min.y;
    (0..5)
        .filter(|&slice| {
            let band = Aabb::new(
                Vec3::new(
                    aabb.min.x,
                    aabb.min.y + height * slice as f32 / 5.0,
                    aabb.min.z,
                ),
                Vec3::new(
                    aabb.max.x,
                    aabb.min.y + height * (slice + 1) as f32 / 5.0,
                    aabb.max.z,
                ),
            );
            water_within(band, chunks)
        })
        .count() as f32
        / 5.0
}

/// `World.canBlockBeRainedOn` while it rains: open sky above the cell, no
/// ground above it, and a biome where rain falls as rain.
fn rain_reaches(chunks: &WorldChunks, cell: IVec3) -> bool {
    chunks.top_solid_block(cell.x, cell.z) <= cell.y
        && (cell.y.max(0)..CHUNK_HEIGHT as i32).all(|y| {
            chunks
                .block_at(cell.x, y, cell.z)
                .is_none_or(|block| block.light_opacity() == 0)
        })
        && chunks.climate_at(cell.x, cell.z).is_some_and(|climate| {
            !matches!(
                climate.biome,
                Biome::Taiga | Biome::Tundra | Biome::IceDesert | Biome::Desert | Biome::Hell
            )
        })
}

/// Advance every bobber once per world tick.
pub(crate) fn tick_bobbers(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    environment: Environment,
    anglers: Query<(&Transform, &Hotbar, Option<&PlayerHealth>, Option<&Fishing>), With<Player>>,
    mut bobbers: Query<
        (Entity, &mut Bobber, &mut Transform, &mut PreviousTick),
        (Without<Living>, Without<Player>),
    >,
    mut mobs: Query<
        (
            Entity,
            &mut Mob,
            &mut Living,
            &mut Velocity,
            &Transform,
            &EntitySize,
        ),
        Without<Player>,
    >,
    mut particles: Option<ResMut<EffectParticles>>,
) {
    let ticks = tick.ticks_this_frame();
    let raining = environment.is_raining();
    for (entity, mut bobber, mut transform, mut previous) in &mut bobbers {
        let mut position = transform.translation;
        // The angler put the rod away, died, left, or cast another line.
        let angler = anglers
            .get(bobber.angler)
            .ok()
            .filter(|(at, hotbar, health, fishing)| {
                fishing.is_some_and(|fishing| fishing.0 == entity)
                    && health.is_none_or(|health| health.current > 0)
                    && hotbar
                        .selected_stack()
                        .is_some_and(|stack| stack.item() == Item::FishingRod)
                    && at.translation.distance_squared(position) <= LINE_LENGTH * LINE_LENGTH
            });
        let mut gone =
            angler.is_none() || !chunks.contains(ChunkPosition::from_world(position.x, position.z));
        if !gone {
            for _ in 0..ticks {
                previous.0 = position;
                if let Some(target) = bobber.hooked {
                    if let Ok((_, _, _, _, at, size)) = mobs.get(target) {
                        let aabb = size.aabb(at.translation);
                        position = Vec3::new(
                            at.translation.x,
                            aabb.min.y + size.height * 0.8,
                            at.translation.z,
                        );
                        continue;
                    }
                    bobber.hooked = None;
                }
                if bobber.in_ground {
                    bobber.ticks_in_ground += 1;
                    if bobber.ticks_in_ground == 1200 {
                        gone = true;
                        break;
                    }
                    continue;
                }

                let reach = bobber.motion.length();
                let block_hit = raycast_blocks(&chunks, position, bobber.motion, reach).is_some();
                let to = if block_hit {
                    position
                } else {
                    position + bobber.motion
                };
                // The angler is a player, whom a blow of 0 never lands on.
                let struck = first_struck(
                    position,
                    to,
                    None,
                    mobs.iter().filter(|(_, mob, ..)| mob.health > 0).map(
                        |(entity, _, _, _, transform, size)| {
                            (entity, size.aabb(transform.translation))
                        },
                    ),
                    None,
                );
                if let Some(Struck::Mob(target)) = struck {
                    // `attackEntityFrom(angler, 0)`: hooked if it lands.
                    if let Ok((_, mut mob, mut living, mut velocity, at, _)) = mobs.get_mut(target)
                    {
                        let hit = Hit {
                            amount: 0,
                            from: angler.map(|(at, ..)| at.translation),
                            source: Source::Player,
                        };
                        let feet = at.translation;
                        if hurt_creature(&mut mob, &mut living, &mut velocity, feet, hit).landed {
                            bobber.hooked = Some(target);
                        }
                    }
                } else if block_hit {
                    bobber.in_ground = true;
                    continue;
                }

                let movement = move_entity(
                    BOBBER_SIZE.aabb(position),
                    bobber.motion,
                    0.0,
                    bobber.collision.on_ground,
                    &chunks,
                );
                position = BOBBER_SIZE.position_from_aabb(movement.aabb);
                bobber.collision = movement.collision;
                let collision = bobber.collision;
                if collision.collided_x {
                    bobber.motion.x = 0.0;
                }
                if collision.collided_y {
                    bobber.motion.y = 0.0;
                }
                if collision.collided_z {
                    bobber.motion.z = 0.0;
                }
                let mut drag =
                    if collision.on_ground || collision.collided_x || collision.collided_z {
                        0.5
                    } else {
                        0.92
                    };
                let submerged = submerged_fraction(movement.aabb, &chunks);
                if submerged > 0.0 {
                    if bobber.ticks_catchable > 0 {
                        bobber.ticks_catchable -= 1;
                    } else {
                        let cell = position.floor().as_ivec3() + IVec3::Y;
                        let odds = if raining && rain_reaches(&chunks, cell) {
                            300
                        } else {
                            500
                        };
                        if bobber.rng.next_int(odds) == 0 {
                            bobber.ticks_catchable = bobber.rng.next_int(30) + 10;
                            bobber.motion.y -= 0.2;
                            // The draws Beta spends on the bite's bubble and
                            // splash particles: six of each.
                            for _ in 0..30 {
                                bobber.rng.next_float();
                            }
                            if let Some(particles) = particles.as_deref_mut() {
                                particles.water_entry(
                                    position,
                                    movement.aabb.min.y.floor(),
                                    BOBBER_SIZE.width,
                                    bobber.motion,
                                );
                            }
                        }
                    }
                }
                if bobber.ticks_catchable > 0 {
                    let tug =
                        bobber.rng.next_float() * bobber.rng.next_float() * bobber.rng.next_float();
                    bobber.motion.y -= tug * 0.2;
                }
                bobber.motion.y += 0.04 * (submerged * 2.0 - 1.0);
                if submerged > 0.0 {
                    drag *= 0.9;
                    bobber.motion.y *= 0.8;
                }
                bobber.motion *= drag;
            }
        }
        transform.translation = position;
        if gone {
            commands.entity(entity).despawn();
            if anglers
                .get(bobber.angler)
                .is_ok_and(|(.., fishing)| fishing.is_some_and(|fishing| fishing.0 == entity))
            {
                commands.entity(bobber.angler).remove::<Fishing>();
            }
        }
    }
}

//! Beta 1.7.3 projectiles: `EntityArrow`, which skeletons, dispensers and the
//! player's bow shoot, and `EntityFireball`, which ghasts lob. Snowballs and
//! eggs are in [`thrown`](crate::entity::thrown) and the fishing bobber in
//! [`fishing`](crate::entity::fishing); they share the helpers here.
//!
//! An arrow falls 0.03 blocks per tick per tick and loses 1% of its speed,
//! passes through plants and torches, and sticks in the first block with a
//! collision box, where it lies for a minute. It hurts what it hits by 4 and
//! glances off a target still invulnerable from an earlier hit. An arrow the
//! player or a dispenser shot can be picked up again once it has stuck. A fireball
//! accelerates toward where it was aimed and explodes against anything,
//! setting fires. The player can bat a fireball back by hitting it.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::Source;
use crate::entity::combat::Victim;
use crate::entity::combat::drop_loot;
use crate::entity::combat::hurt_creature;
use crate::entity::combat::hurt_player;
use crate::entity::creature::Living;
use crate::entity::creature::PLAYER_EYE_HEIGHT;
use crate::entity::explosion::Explosion;
use crate::entity::mobs::Mob;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::raycast_blocks;
use crate::physics::raycast_collision;
use crate::physics::segment_entry;
use crate::physics::water_movement;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::rendering::particles::effects::EffectParticles;
use crate::rendering::particles::effects::FxKind;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::difficulty::Difficulty;
use crate::world::tick::WorldTick;

/// `EntityArrow.setSize(0.5, 0.5)`.
pub const ARROW_SIZE: EntitySize = EntitySize {
    width: 0.5,
    height: 0.5,
    y_offset: 0.0,
};
/// `EntityFireball.setSize(1, 1)`.
pub const FIREBALL_SIZE: EntitySize = EntitySize {
    width: 1.0,
    height: 1.0,
    y_offset: 0.0,
};

/// `EntityPickupFX` lasts three ticks.
pub const PICKUP_FLIGHT_TICKS: u32 = 3;

/// Marks a body that is neither living nor an item but still presses a
/// wooden pressure plate: arrows, thrown snowballs and eggs, and bobbers.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Projectile;

/// How hard an arrow strikes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArrowDamage {
    /// Beta 1.7.3: every arrow does this much.
    Flat(i16),
    /// The Bow Charging feature, after Beta 1.8: twice the arrow's speed,
    /// rounded up, and a critical arrow adds up to half of that again.
    Speed { critical: bool },
}

/// Who loosed an arrow. Beta's arrow keeps a reference to its shooter even
/// after the shooter dies, so what the hit needs is copied here.
#[derive(Clone, Copy, Debug)]
pub struct Shooter {
    pub entity: Entity,
    /// Where it stood when it shot, used once it is gone.
    pub position: Vec3,
    /// What kind of attacker it is: a skeleton is [`Source::Monster`].
    pub source: Source,
}

impl Shooter {
    pub fn player(entity: Entity, position: Vec3) -> Self {
        Self {
            entity,
            position,
            source: Source::Player,
        }
    }
}

/// `EntityArrow`. Its `Transform` is the bottom center of its box.
#[derive(Component, Clone, Debug)]
pub struct Arrow {
    /// Who shot it, whom it cannot hit for its first 5 ticks.
    pub shooter: Option<Shooter>,
    /// `doesArrowBelongToPlayer`: the player can pick it up once it sticks.
    pub pickup: bool,
    /// Ticks since a player picked it up. `EntityPickupFX` flies it to them
    /// from where it stuck for [`PICKUP_FLIGHT_TICKS`], and then it is gone.
    pub taken: Option<u32>,
    pub damage: ArrowDamage,
    /// `motionX/Y/Z`, in blocks per tick.
    pub motion: Vec3,
    /// Degrees, for the renderer. Yaw 0 points along +Z.
    pub yaw: f32,
    pub prev_yaw: f32,
    pub pitch: f32,
    pub prev_pitch: f32,
    /// `arrowShake`: the quiver after striking a block.
    pub shake: i16,
    /// `inGround`, with the block it stuck in and that block's metadata.
    stuck: Option<(IVec3, Block, u8)>,
    ticks_in_ground: u32,
    ticks_in_air: u32,
}

/// `EntityFireball`. Its `Transform` is the bottom center of its box.
#[derive(Component, Clone, Debug)]
pub struct Fireball {
    /// The ghast that lobbed it, which it cannot hit for its first 25 ticks.
    pub owner: Option<Entity>,
    /// `motionX/Y/Z`, in blocks per tick.
    pub motion: Vec3,
    /// `accelerationX/Y/Z`, added every tick.
    pub acceleration: Vec3,
    ticks_in_air: u32,
}

impl Arrow {
    /// `inGround`.
    pub fn is_stuck(&self) -> bool {
        self.stuck.is_some()
    }
}

impl Fireball {
    /// `EntityFireball.attackEntityFrom`: struck by the player, it flies off
    /// the way they look.
    pub fn deflect(&mut self, look: Vec3) {
        self.motion = look;
        self.acceleration = look * 0.1;
    }
}

/// Java's `nextGaussian` by the polar method, without its cached second value.
pub(crate) fn gaussian(rng: &mut JavaRandom) -> f32 {
    loop {
        let a = 2.0 * rng.next_double() - 1.0;
        let b = 2.0 * rng.next_double() - 1.0;
        let s = a * a + b * b;
        if s < 1.0 && s != 0.0 {
            return (a * (-2.0 * s.ln() / s).sqrt()) as f32;
        }
    }
}

pub(crate) fn gaussian_vec(rng: &mut JavaRandom) -> Vec3 {
    Vec3::new(gaussian(rng), gaussian(rng), gaussian(rng))
}

/// `(yaw, pitch)` in degrees for a direction of travel.
pub(crate) fn heading_angles(motion: Vec3) -> (f32, f32) {
    let horizontal = (motion.x * motion.x + motion.z * motion.z).sqrt();
    (
        motion.x.atan2(motion.z).to_degrees(),
        motion.y.atan2(horizontal).to_degrees(),
    )
}

/// `setArrowHeading` and its snowball, egg and bobber twins: `heading` at
/// `speed` blocks per tick, scattered by `spread`.
pub(crate) fn scattered_heading(
    heading: Vec3,
    speed: f32,
    spread: f32,
    rng: &mut JavaRandom,
) -> Vec3 {
    (heading.normalize_or_zero() + gaussian_vec(rng) * 0.0075 * spread) * speed
}

/// Where `new EntityArrow(world, shooter)` and the thrown entities start:
/// `eye`, a little to the right of the way `look` faces and 0.1 down.
pub fn hand_origin(eye: Vec3, look: Vec3) -> Vec3 {
    let flat = Vec3::new(look.x, 0.0, look.z).normalize_or_zero();
    eye - Vec3::new(flat.z * 0.16, 0.1, -flat.x * 0.16)
}

/// `EntityArrow.setArrowHeading`: aim along `heading` at `speed` blocks per
/// tick, scattered by `spread`. The arrow does Beta's 4 damage and stays
/// where it lands.
pub fn spawn_arrow(
    commands: &mut Commands,
    position: Vec3,
    heading: Vec3,
    speed: f32,
    spread: f32,
    shooter: Option<Shooter>,
    rng: &mut JavaRandom,
) -> Entity {
    spawn_arrow_with(
        commands,
        position,
        heading,
        speed,
        spread,
        shooter,
        false,
        ArrowDamage::Flat(4),
        rng,
    )
}

/// [`spawn_arrow`] with `doesArrowBelongToPlayer` and the damage chosen.
#[allow(clippy::too_many_arguments)]
pub fn spawn_arrow_with(
    commands: &mut Commands,
    position: Vec3,
    heading: Vec3,
    speed: f32,
    spread: f32,
    shooter: Option<Shooter>,
    pickup: bool,
    damage: ArrowDamage,
    rng: &mut JavaRandom,
) -> Entity {
    let motion = scattered_heading(heading, speed, spread, rng);
    let (yaw, pitch) = heading_angles(motion);
    commands
        .spawn((
            Name::new("Arrow"),
            Arrow {
                shooter,
                pickup,
                taken: None,
                damage,
                motion,
                yaw,
                prev_yaw: yaw,
                pitch,
                prev_pitch: pitch,
                shake: 0,
                stuck: None,
                ticks_in_ground: 0,
                ticks_in_air: 0,
            },
            Projectile,
            ARROW_SIZE,
            Transform::from_translation(position),
            PreviousTick(position),
            Visibility::Inherited,
        ))
        .id()
}

/// `new EntityArrow(world, player)` from `ItemBow`: from the hand, along
/// `look`. Beta 1.7.3 always shoots at 1.5 blocks per tick.
pub fn spawn_player_arrow(
    commands: &mut Commands,
    player: Entity,
    eye: Vec3,
    look: Vec3,
    speed: f32,
    damage: ArrowDamage,
    rng: &mut JavaRandom,
) -> Entity {
    let origin = hand_origin(eye + Vec3::Y * PLAYER_EYE_HEIGHT, look);
    spawn_arrow_with(
        commands,
        origin,
        look,
        speed,
        1.0,
        Some(Shooter::player(player, eye)),
        true,
        damage,
        rng,
    )
}

/// `new EntityFireball(world, shooter, dx, dy, dz)`: aimed along `toward`,
/// scattered a little, and starting at rest.
pub fn spawn_fireball(
    commands: &mut Commands,
    position: Vec3,
    toward: Vec3,
    owner: Option<Entity>,
    rng: &mut JavaRandom,
) -> Entity {
    let toward = toward + gaussian_vec(rng) * 0.4;
    commands
        .spawn((
            Name::new("Fireball"),
            Fireball {
                owner,
                motion: Vec3::ZERO,
                acceleration: toward.normalize_or_zero() * 0.1,
                ticks_in_air: 0,
            },
            Transform::from_translation(position),
            PreviousTick(position),
            Visibility::Inherited,
        ))
        .id()
}

/// Something a projectile can strike.
#[derive(Clone, Copy)]
pub(crate) enum Struck {
    Player(Entity),
    Mob(Entity),
}

/// The nearest body whose box, grown by 0.3, the segment enters.
pub(crate) fn first_struck(
    from: Vec3,
    to: Vec3,
    players: &[(Entity, Aabb)],
    mobs: impl Iterator<Item = (Entity, Aabb)>,
    ignore: Option<Entity>,
) -> Option<Struck> {
    let delta = to - from;
    let length = delta.length();
    let direction = delta.normalize_or_zero();
    if direction == Vec3::ZERO {
        return None;
    }
    let entry = |aabb: Aabb| {
        let border = Vec3::splat(0.3);
        segment_entry(
            from,
            direction,
            aabb.min - border,
            aabb.max + border,
            length,
        )
    };
    let mut best: Option<(f32, Struck)> = None;
    let mut consider = |distance: Option<f32>, struck: Struck| {
        if let Some(distance) = distance
            && best.is_none_or(|(nearest, _)| distance < nearest)
        {
            best = Some((distance, struck));
        }
    };
    // The shooter's own box does not count while it is ignored either.
    for &(entity, aabb) in players {
        if Some(entity) != ignore {
            consider(entry(aabb), Struck::Player(entity));
        }
    }
    for (entity, aabb) in mobs {
        if Some(entity) != ignore {
            consider(entry(aabb), Struck::Mob(entity));
        }
    }
    best.map(|(_, struck)| struck)
}

/// Fold `next` toward `previous` the way Beta smooths a projectile's facing.
pub(crate) fn smooth_angle(previous: &mut f32, next: f32) -> f32 {
    let mut prev = *previous;
    while next - prev < -180.0 {
        prev -= 360.0;
    }
    while next - prev >= 180.0 {
        prev += 360.0;
    }
    *previous = prev;
    prev + (next - prev) * 0.2
}

pub(crate) type PlayerParts<'a> = (
    &'a Transform,
    Option<Mut<'a, PlayerHealth>>,
    Option<Mut<'a, PlayerCombat>>,
    Mut<'a, Velocity>,
    Option<Mut<'a, Inventory>>,
);

/// A player as a target, if they can take damage. A player without an
/// inventory wears `spare` armor, which is empty.
pub(crate) fn victim_of<'a>(
    player: &'a mut PlayerParts<'_>,
    spare: &'a mut [Option<ItemStack>; 4],
) -> Option<Victim<'a>> {
    let (transform, health, combat, velocity, inventory) = player;
    let (bevy_yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
    Some(Victim {
        health: health.as_mut()?.as_mut(),
        combat: combat.as_mut()?.as_mut(),
        velocity: velocity.as_mut(),
        armor: match inventory {
            Some(inventory) => &mut inventory.as_mut().armor,
            None => spare,
        },
        eye: transform.translation,
        yaw: (std::f32::consts::PI - bevy_yaw).to_degrees(),
    })
}

/// Advance every arrow and fireball once per world tick.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick_projectiles(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    settings: Option<Res<GameSettings>>,
    mut arrows: Query<
        (Entity, &mut Arrow, &mut Transform, &mut PreviousTick),
        (Without<Fireball>, Without<Living>, Without<Player>),
    >,
    mut fireballs: Query<
        (Entity, &mut Fireball, &mut Transform, &mut PreviousTick),
        (Without<Arrow>, Without<Living>, Without<Player>),
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
    mut players: Query<
        (
            Entity,
            (
                &Transform,
                Option<&mut PlayerHealth>,
                Option<&mut PlayerCombat>,
                &mut Velocity,
                Option<&mut Inventory>,
            ),
        ),
        With<Player>,
    >,
    mut explosions: MessageWriter<Explosion>,
    mut particles: Option<ResMut<EffectParticles>>,
    mut loot: Local<ItemRng>,
    mut rng: Local<ProjectileRandom>,
    mut spare_armor: Local<[Option<ItemStack>; 4]>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |settings| settings.difficulty);
    let mut players: Vec<_> = players.iter_mut().collect();
    let player_boxes: Vec<(Entity, Aabb)> = players
        .iter()
        .map(|(entity, (transform, ..))| (*entity, EntitySize::PLAYER.aabb(transform.translation)))
        .collect();

    for (entity, mut arrow, mut transform, mut previous) in &mut arrows {
        let mut position = transform.translation;
        if !chunks.contains(ChunkPosition::from_world(position.x, position.z)) {
            commands.entity(entity).despawn();
            continue;
        }
        let mut gone = false;
        for _ in 0..ticks {
            previous.0 = position;
            arrow.prev_yaw = arrow.yaw;
            arrow.prev_pitch = arrow.pitch;
            if let Some(age) = arrow.taken.as_mut() {
                // Flying to the player who picked it up.
                *age += 1;
                if *age >= PICKUP_FLIGHT_TICKS {
                    gone = true;
                    break;
                }
                continue;
            }
            arrow.shake = (arrow.shake - 1).max(0);
            if let Some((cell, block, metadata)) = arrow.stuck {
                if chunks.block_at(cell.x, cell.y, cell.z) == Some(block)
                    && chunks.metadata_at(cell.x, cell.y, cell.z) == metadata
                {
                    arrow.ticks_in_ground += 1;
                    if arrow.ticks_in_ground == 1200 {
                        gone = true;
                        break;
                    }
                    continue;
                }
                // The block it was stuck in is gone: drop out of it.
                arrow.stuck = None;
                arrow.motion *= Vec3::new(
                    rng.0.next_float() * 0.2,
                    rng.0.next_float() * 0.2,
                    rng.0.next_float() * 0.2,
                );
                arrow.ticks_in_ground = 0;
                arrow.ticks_in_air = 0;
            }

            arrow.ticks_in_air += 1;
            let mut to = position + arrow.motion;
            let block_hit = raycast_collision(&chunks, position, to);
            if let Some((_, point)) = block_hit {
                to = point;
            }
            let ignore = arrow
                .shooter
                .filter(|_| arrow.ticks_in_air < 5)
                .map(|shooter| shooter.entity);
            let struck = first_struck(
                position,
                to,
                &player_boxes,
                mobs.iter().filter(|(_, mob, ..)| mob.health > 0).map(
                    |(entity, _, _, _, transform, size)| (entity, size.aabb(transform.translation)),
                ),
                ignore,
            );
            let amount = match arrow.damage {
                ArrowDamage::Flat(amount) => amount,
                ArrowDamage::Speed { critical } => {
                    let amount = (arrow.motion.length() * 2.0).ceil() as i16;
                    if critical {
                        amount + rng.0.next_int((amount / 2 + 2) as u32) as i16
                    } else {
                        amount
                    }
                }
            };
            let hit = Hit {
                amount,
                from: arrow.shooter.map(|shooter| {
                    if let Some((_, (transform, ..))) =
                        players.iter().find(|(entity, _)| *entity == shooter.entity)
                    {
                        return transform.translation;
                    }
                    mobs.get(shooter.entity)
                        .map_or(shooter.position, |(.., transform, _)| transform.translation)
                }),
                source: arrow
                    .shooter
                    .map_or(Source::Environment, |shooter| shooter.source),
            };
            if let Some(struck) = struck {
                let landed = match struck {
                    Struck::Player(target) => players
                        .iter_mut()
                        .find(|(entity, _)| *entity == target)
                        .and_then(|(_, parts)| victim_of(parts, &mut spare_armor))
                        .is_some_and(|mut victim| {
                            hurt_player(&mut victim, hit, difficulty, &mut loot)
                        }),
                    Struck::Mob(target) => mobs.get_mut(target).is_ok_and(
                        |(_, mut mob, mut living, mut velocity, transform, _)| {
                            let feet = transform.translation;
                            let wound =
                                hurt_creature(&mut mob, &mut living, &mut velocity, feet, hit);
                            if wound.died {
                                drop_loot(&mut commands, &mut loot, &mut mob, feet);
                            }
                            wound.landed
                        },
                    ),
                };
                if landed {
                    gone = true;
                    break;
                }
                arrow.motion *= -0.1;
                arrow.yaw += 180.0;
                arrow.prev_yaw += 180.0;
                arrow.ticks_in_air = 0;
            } else if let Some((cell, point)) = block_hit {
                let block = chunks
                    .block_at(cell.x, cell.y, cell.z)
                    .unwrap_or(Block::Air);
                arrow.stuck = Some((cell, block, chunks.metadata_at(cell.x, cell.y, cell.z)));
                arrow.motion = point - position;
                position -= arrow.motion.normalize_or_zero() * 0.05;
                arrow.shake = 7;
            }
            position += arrow.motion;
            let (yaw, pitch) = heading_angles(arrow.motion);
            arrow.pitch = smooth_angle(&mut arrow.prev_pitch, pitch);
            arrow.yaw = smooth_angle(&mut arrow.prev_yaw, yaw);
            let drag = if water_movement(ARROW_SIZE.aabb(position), &chunks).0 {
                if let Some(particles) = particles.as_deref_mut() {
                    for _ in 0..4 {
                        particles.spawn(
                            FxKind::Bubble,
                            position - arrow.motion * 0.25,
                            arrow.motion,
                        );
                    }
                }
                0.8
            } else {
                0.99
            };
            arrow.motion *= drag;
            arrow.motion.y -= 0.03;
        }
        transform.translation = position;
        if gone {
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut fireball, mut transform, mut previous) in &mut fireballs {
        let mut position = transform.translation;
        if !chunks.contains(ChunkPosition::from_world(position.x, position.z)) {
            commands.entity(entity).despawn();
            continue;
        }
        let mut exploded = false;
        for _ in 0..ticks {
            previous.0 = position;
            fireball.ticks_in_air += 1;
            let mut to = position + fireball.motion;
            let reach = fireball.motion.length();
            let block_hit = raycast_blocks(&chunks, position, fireball.motion, reach).is_some();
            if block_hit {
                to = position;
            }
            let ignore = fireball.owner.filter(|_| fireball.ticks_in_air < 25);
            let struck = first_struck(
                position,
                to,
                &player_boxes,
                mobs.iter().filter(|(_, mob, ..)| mob.health > 0).map(
                    |(entity, _, _, _, transform, size)| (entity, size.aabb(transform.translation)),
                ),
                ignore,
            );
            if struck.is_some() || block_hit {
                // `attackEntityFrom(shootingEntity, 0)`: no damage, but a mob
                // still flinches. The blast does the harm.
                if let Some(Struck::Mob(target)) = struck {
                    let from = fireball
                        .owner
                        .and_then(|owner| mobs.get(owner).ok())
                        .map(|(.., transform, _)| transform.translation);
                    if let Ok((_, mut mob, mut living, mut velocity, transform, _)) =
                        mobs.get_mut(target)
                    {
                        let feet = transform.translation;
                        let hit = Hit {
                            amount: 0,
                            from,
                            source: Source::Creature,
                        };
                        hurt_creature(&mut mob, &mut living, &mut velocity, feet, hit);
                    }
                }
                explosions.write(Explosion {
                    center: position,
                    strength: 1.0,
                    flaming: true,
                    source: Source::Environment,
                });
                exploded = true;
                break;
            }
            position += fireball.motion;
            let drag = if water_movement(FIREBALL_SIZE.aabb(position), &chunks).0 {
                if let Some(particles) = particles.as_deref_mut() {
                    for _ in 0..4 {
                        particles.spawn(
                            FxKind::Bubble,
                            position - fireball.motion * 0.25,
                            fireball.motion,
                        );
                    }
                }
                0.8
            } else {
                0.95
            };
            let acceleration = fireball.acceleration;
            fireball.motion = (fireball.motion + acceleration) * drag;
            if let Some(particles) = particles.as_deref_mut() {
                particles.spawn(FxKind::Smoke, position + Vec3::Y * 0.5, Vec3::ZERO);
            }
        }
        transform.translation = position;
        if exploded {
            commands.entity(entity).despawn();
        }
    }
}

/// `EntityArrow.onCollideWithPlayer`: an arrow that has stuck and stopped
/// quivering, shot by the player or a dispenser, goes back into the inventory
/// of a player who touches it, flying to them as a picked-up item does
/// (`EntityPickupFX`). It stays where it is if there is no room.
pub(crate) fn pickup_arrows(
    mut commands: Commands,
    mut players: Query<(&Transform, &EntitySize, &mut Hotbar, &mut Inventory), With<Player>>,
    mut arrows: Query<(Entity, &mut Arrow, &Transform), Without<Player>>,
) {
    for (entity, mut arrow, transform) in &mut arrows {
        if !arrow.pickup || arrow.taken.is_some() || !arrow.is_stuck() || arrow.shake > 0 {
            continue;
        }
        for (at, size, mut hotbar, mut inventory) in &mut players {
            // `boundingBox.expand(1, 0, 1)`, as for a dropped item.
            let mut reach = size.aabb(at.translation);
            reach.min -= Vec3::new(1.0, 0.0, 1.0);
            reach.max += Vec3::new(1.0, 0.0, 1.0);
            if !reach.intersects(ARROW_SIZE.aabb(transform.translation)) {
                continue;
            }
            let Ok(stack) = ItemStack::new(Item::Arrow, 1) else {
                return;
            };
            // A full inventory changes nothing, so it must not be flagged as
            // changed every frame the player stands on the arrow.
            if inventory
                .bypass_change_detection()
                .insert(hotbar.bypass_change_detection(), stack)
                .is_none()
            {
                inventory.set_changed();
                hotbar.set_changed();
                // The flight's last tick removes the arrow.
                arrow.pickup = false;
                arrow.taken = Some(0);
                commands.entity(entity).remove::<Projectile>();
                break;
            }
        }
    }
}

/// `Entity.rand` for projectiles.
pub(crate) struct ProjectileRandom(pub(crate) JavaRandom);

impl Default for ProjectileRandom {
    fn default() -> Self {
        Self(JavaRandom::new(0x4152_524f))
    }
}

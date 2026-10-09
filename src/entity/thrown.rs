//! Beta 1.7.3 `EntitySnowball` and `EntityEgg`: thrown by the player or
//! launched by a dispenser, they fly like an arrow and burst on the first
//! thing they touch.
//!
//! A hit is `attackEntityFrom(thrower, 0)`: no damage, only the flinch and
//! knockback. One egg in eight hatches a chicken, and one of those in
//! thirty-two hatches four. The `snowballpoof` and bubble particles are not
//! drawn.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::PlayerCombat;
use crate::entity::combat::Source;
use crate::entity::combat::hurt_creature;
use crate::entity::combat::hurt_player;
use crate::entity::creature::Living;
use crate::entity::creature::PLAYER_EYE_HEIGHT;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mobs::spawn_facing;
use crate::entity::projectiles::Projectile;
use crate::entity::projectiles::ProjectileRandom;
use crate::entity::projectiles::Shooter;
use crate::entity::projectiles::Struck;
use crate::entity::projectiles::first_struck;
use crate::entity::projectiles::hand_origin;
use crate::entity::projectiles::heading_angles;
use crate::entity::projectiles::scattered_heading;
use crate::entity::projectiles::victim;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::raycast_blocks;
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

/// `setSize(0.25, 0.25)`.
pub const THROWN_SIZE: EntitySize = EntitySize {
    width: 0.25,
    height: 0.25,
    y_offset: 0.0,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrownKind {
    Snowball,
    Egg,
}

impl ThrownKind {
    /// The kind `item` is thrown as, if it is thrown at all.
    pub fn from_item(item: Item) -> Option<Self> {
        match item {
            Item::Snowball => Some(Self::Snowball),
            Item::Egg => Some(Self::Egg),
            _ => None,
        }
    }

    pub fn item(self) -> Item {
        match self {
            Self::Snowball => Item::Snowball,
            Self::Egg => Item::Egg,
        }
    }
}

/// `EntitySnowball` or `EntityEgg`. Its `Transform` is the bottom center of
/// its box.
#[derive(Component, Clone, Debug)]
pub struct Thrown {
    pub kind: ThrownKind,
    /// Who threw it, whom it cannot hit for its first 5 ticks.
    pub thrower: Option<Shooter>,
    /// `motionX/Y/Z`, in blocks per tick.
    pub motion: Vec3,
    /// `rotationYaw`, in degrees, which a hatched chicken faces.
    pub yaw: f32,
    ticks_in_air: u32,
}

/// `setSnowballHeading` on a new entity at `position`.
pub fn spawn_thrown(
    commands: &mut Commands,
    kind: ThrownKind,
    position: Vec3,
    heading: Vec3,
    speed: f32,
    spread: f32,
    thrower: Option<Shooter>,
    rng: &mut JavaRandom,
) -> Entity {
    let motion = scattered_heading(heading, speed, spread, rng);
    commands
        .spawn((
            Name::new(match kind {
                ThrownKind::Snowball => "Snowball",
                ThrownKind::Egg => "Egg",
            }),
            Thrown {
                kind,
                thrower,
                motion,
                yaw: heading_angles(motion).0,
                ticks_in_air: 0,
            },
            Projectile,
            THROWN_SIZE,
            Transform::from_translation(position),
            PreviousTick(position),
            Visibility::Inherited,
        ))
        .id()
}

/// `new EntitySnowball(world, player)`: from the hand, along `look`, at 1.5
/// blocks per tick.
pub fn throw_from_player(
    commands: &mut Commands,
    kind: ThrownKind,
    player: Entity,
    eye: Vec3,
    look: Vec3,
    rng: &mut JavaRandom,
) -> Entity {
    spawn_thrown(
        commands,
        kind,
        hand_origin(eye + Vec3::Y * PLAYER_EYE_HEIGHT, look),
        look,
        1.5,
        1.0,
        Some(Shooter::player(player, eye)),
        rng,
    )
}

/// `EntityEgg.onUpdate`'s hatch roll: how many chickens come out.
pub fn egg_chickens(rng: &mut JavaRandom) -> u32 {
    if rng.next_int(8) != 0 {
        0
    } else if rng.next_int(32) == 0 {
        4
    } else {
        1
    }
}

/// Advance every snowball and egg once per world tick.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tick_thrown(
    mut commands: Commands,
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    settings: Option<Res<GameSettings>>,
    mut thrown: Query<
        (Entity, &mut Thrown, &mut Transform, &mut PreviousTick),
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
    player_entity: Query<Entity, With<Player>>,
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
    let mut player = player.single_mut().ok();
    let player_entity = player_entity.single().ok();

    for (entity, mut ball, mut transform, mut previous) in &mut thrown {
        let mut position = transform.translation;
        if !chunks.contains(ChunkPosition::from_world(position.x, position.z)) {
            commands.entity(entity).despawn();
            continue;
        }
        let mut burst = false;
        for _ in 0..ticks {
            previous.0 = position;
            ball.ticks_in_air += 1;
            let mut to = position + ball.motion;
            let reach = ball.motion.length();
            let block_hit = raycast_blocks(&chunks, position, ball.motion, reach).is_some();
            if block_hit {
                to = position;
            }
            let ignore = ball
                .thrower
                .filter(|_| ball.ticks_in_air < 5)
                .map(|thrower| thrower.entity);
            let player_box = player
                .as_ref()
                .filter(|_| ignore.is_none() || ignore != player_entity)
                .map(|(transform, ..)| EntitySize::PLAYER.aabb(transform.translation));
            let struck = first_struck(
                position,
                to,
                player_box,
                mobs.iter().filter(|(_, mob, ..)| mob.health > 0).map(
                    |(entity, _, _, _, transform, size)| (entity, size.aabb(transform.translation)),
                ),
                ignore,
            );
            if struck.is_some() || block_hit {
                // `attackEntityFrom(thrower, 0)`.
                let hit = Hit {
                    amount: 0,
                    from: ball.thrower.map(|thrower| {
                        if Some(thrower.entity) == player_entity
                            && let Some((transform, ..)) = player.as_ref()
                        {
                            return transform.translation;
                        }
                        mobs.get(thrower.entity)
                            .map_or(thrower.position, |(.., transform, _)| transform.translation)
                    }),
                    source: ball
                        .thrower
                        .map_or(Source::Environment, |thrower| thrower.source),
                };
                match struck {
                    Some(Struck::Player) => {
                        if let Some(mut victim) = victim(&mut player, &mut spare_armor) {
                            hurt_player(&mut victim, hit, difficulty, &mut loot);
                        }
                    }
                    Some(Struck::Mob(target)) => {
                        if let Ok((_, mut mob, mut living, mut velocity, transform, _)) =
                            mobs.get_mut(target)
                        {
                            let feet = transform.translation;
                            hurt_creature(&mut mob, &mut living, &mut velocity, feet, hit);
                        }
                    }
                    None => {}
                }
                if ball.kind == ThrownKind::Egg {
                    for _ in 0..egg_chickens(&mut rng.0) {
                        let chicken = Mob::new(MobType::Chicken, rng.0.next_long() as u64);
                        spawn_facing(&mut commands, chicken, position, ball.yaw);
                    }
                }
                burst = true;
                break;
            }
            position += ball.motion;
            ball.yaw = heading_angles(ball.motion).0;
            let drag = if water_movement(THROWN_SIZE.aabb(position), &chunks).0 {
                if let Some(particles) = particles.as_deref_mut() {
                    for _ in 0..4 {
                        particles.spawn(FxKind::Bubble, position - ball.motion * 0.25, ball.motion);
                    }
                }
                0.8
            } else {
                0.99
            };
            ball.motion *= drag;
            ball.motion.y -= 0.03;
        }
        transform.translation = position;
        if burst {
            commands.entity(entity).despawn();
        }
    }
}

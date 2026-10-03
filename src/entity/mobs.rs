//! Independently simulated Beta 1.7.3 creatures. Blocks remain compact chunk data.
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::app::settings::Difficulty;
use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::block::id::Id;
use crate::block::properties::is_opaque_cube;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Gravity;
use crate::entity::PreviousTick;
use crate::entity::StepDistance;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::creature::Swim;
use crate::entity::creature::Wings;
use crate::entity::creature::tick_creatures;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::physics::PhysicsSet;
use crate::physics::raycast_blocks;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::random::JavaRandom;
use crate::world::biome::Biome;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::environment::celestial_angle;
use crate::world::environment::skylight_subtracted;
use crate::world::lighting::LightCache;
use crate::world::lighting::combined_light;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MobKind {
    Spider,
    Zombie,
    Skeleton,
    Creeper,
    Slime,
    Sheep,
    Pig,
    Chicken,
    Cow,
    Wolf,
    Squid,
    Ghast,
    PigZombie,
}

impl MobKind {
    pub const ALL: [Self; 13] = [
        Self::Spider,
        Self::Zombie,
        Self::Skeleton,
        Self::Creeper,
        Self::Slime,
        Self::Sheep,
        Self::Pig,
        Self::Chicken,
        Self::Cow,
        Self::Wolf,
        Self::Squid,
        Self::Ghast,
        Self::PigZombie,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Spider => "spider",
            Self::Zombie => "zombie",
            Self::Skeleton => "skeleton",
            Self::Creeper => "creeper",
            Self::Slime => "slime",
            Self::Sheep => "sheep",
            Self::Pig => "pig",
            Self::Chicken => "chicken",
            Self::Cow => "cow",
            Self::Wolf => "wolf",
            Self::Squid => "squid",
            Self::Ghast => "ghast",
            Self::PigZombie => "pig_zombie",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| {
            kind.name() == name
                || (*kind == Self::PigZombie && matches!(name, "pigzombie" | "zombie_pigman"))
        })
    }

    pub fn size(self, variant: u8) -> EntitySize {
        let (width, height) = match self {
            Self::Spider => (1.4, 0.9),
            Self::Cow | Self::Sheep => (0.9, 1.3),
            Self::Pig => (0.9, 0.9),
            Self::Chicken => (0.3, 0.4),
            Self::Wolf => (0.8, 0.8),
            Self::Squid => (0.95, 0.95),
            Self::Ghast => (4.0, 4.0),
            Self::Slime => {
                let side = f32::from(variant.max(1)) * 0.6;
                (side, side)
            }
            _ => (0.6, 1.8),
        };
        EntitySize {
            width,
            height,
            y_offset: 0.0,
        }
    }

    pub fn health(self, variant: u8) -> i16 {
        match self {
            Self::Chicken => 4,
            Self::Wolf => 8,
            Self::Slime => i16::from(variant.max(1)).pow(2),
            Self::Sheep | Self::Pig | Self::Cow | Self::Squid | Self::Ghast => 10,
            _ => 20,
        }
    }

    pub fn hostile(self) -> bool {
        matches!(
            self,
            Self::Spider
                | Self::Zombie
                | Self::Skeleton
                | Self::Creeper
                | Self::Slime
                | Self::Ghast
                | Self::PigZombie
        )
    }

    pub fn texture(self, state: &Mob) -> &'static str {
        match self {
            Self::Wolf if state.tamed => "mob/wolf_tame.png",
            Self::Wolf if state.angry => "mob/wolf_angry.png",
            Self::Ghast if state.fuse > 10 => "mob/ghast_fire.png",
            Self::Spider => "mob/spider.png",
            Self::Zombie => "mob/zombie.png",
            Self::Skeleton => "mob/skeleton.png",
            Self::Creeper => "mob/creeper.png",
            Self::Slime => "mob/slime.png",
            Self::Sheep => "mob/sheep.png",
            Self::Pig => "mob/pig.png",
            Self::Chicken => "mob/chicken.png",
            Self::Cow => "mob/cow.png",
            Self::Wolf => "mob/wolf.png",
            Self::Squid => "mob/squid.png",
            Self::Ghast => "mob/ghast.png",
            Self::PigZombie => "mob/pigzombie.png",
        }
    }
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Mob {
    pub kind: MobKind,
    pub health: i16,
    pub age: u32,
    pub variant: u8, // slime size, or sheep's wool color
    pub sheared: bool,
    pub saddled: bool,
    pub tamed: bool,
    pub sitting: bool,
    pub angry: bool,
    pub charged: bool,
    pub owner: Option<String>,
    pub fuse: i16,
    pub cooldown: u16,
    pub egg_timer: u16,
    pub fire_ticks: u16,
    pub wander_yaw: f32,
    pub wander_ticks: u16,
    pub rng: JavaRandom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MobRecord {
    pub mob: Mob,
    pub feet: [f32; 3],
    pub velocity: [f32; 3],
    /// Beta's saved `Rotation` yaw, in degrees. Older saves face south.
    #[serde(default)]
    pub yaw: f32,
}

impl MobRecord {
    pub fn capture(mob: &Mob, feet: Vec3, velocity: Vec3, living: Option<&Living>) -> Self {
        Self {
            mob: mob.clone(),
            feet: feet.to_array(),
            velocity: velocity.to_array(),
            yaw: living.map_or(0.0, |living| living.yaw),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MobSpawner {
    pub kind: MobKind,
    pub delay: u16,
    pub rng_state: u64,
}

impl Default for MobSpawner {
    fn default() -> Self {
        Self {
            kind: MobKind::Pig,
            delay: 20,
            rng_state: JavaRandom::new(0x5350_4157).state(),
        }
    }
}

pub fn spawn_saved(commands: &mut Commands, record: MobRecord) {
    let entity = spawn_facing(
        commands,
        record.mob,
        Vec3::from_array(record.feet),
        record.yaw,
    );
    commands
        .entity(entity)
        .insert(Velocity(Vec3::from_array(record.velocity)));
}

impl Mob {
    pub fn new(kind: MobKind, seed: u64) -> Self {
        let mut rng = JavaRandom::new(seed);
        let variant = match kind {
            MobKind::Slime => 1 << rng.next_int(3),
            MobKind::Sheep => {
                let roll = rng.next_int(100);
                match roll {
                    0..=4 => 15,
                    5..=9 => 7,
                    10..=14 => 8,
                    15..=17 => 12,
                    _ => {
                        if rng.next_int(500) == 0 {
                            6
                        } else {
                            0
                        }
                    }
                }
            }
            _ => 0,
        } as u8;
        let egg_timer = if kind == MobKind::Chicken {
            6000 + rng.next_int(6000) as u16
        } else {
            0
        };
        Self {
            kind,
            health: kind.health(variant),
            age: 0,
            variant,
            sheared: false,
            saddled: false,
            tamed: false,
            sitting: false,
            angry: false,
            charged: false,
            owner: None,
            fuse: 0,
            cooldown: 0,
            egg_timer,
            fire_ticks: 0,
            wander_yaw: 0.0,
            wander_ticks: 0,
            rng,
        }
    }
}

#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnMob {
    pub kind: MobKind,
    /// World coordinates of the mob's feet.
    pub feet: Vec3,
    /// Explicit summons bypass natural biome/light tests, not unloaded chunks.
    pub explicit: bool,
    pub variant: u8,
}

#[derive(Component)]
pub struct MobProjectile {
    pub velocity: Vec3,
    pub fireball: bool,
    pub age: u16,
}

#[derive(Component)]
pub struct PrimedTnt {
    pub fuse: u16,
}

#[derive(Message, Clone, Copy)]
pub struct Explosion {
    pub center: Vec3,
    pub strength: f32,
}

pub fn prime_tnt(commands: &mut Commands, position: Vec3, fuse: u16) {
    commands.spawn((
        PrimedTnt { fuse },
        Transform::from_translation(position),
        EntitySize {
            width: 0.98,
            height: 0.98,
            y_offset: 0.0,
        },
        Velocity::default(),
        Gravity::DEFAULT,
        CollisionState::default(),
        StepHeight(0.0),
    ));
}

/// Distance along a ray to a creature's collision box, if it is within reach.
pub fn ray_hit(
    origin: Vec3,
    direction: Vec3,
    feet: Vec3,
    size: EntitySize,
    reach: f32,
) -> Option<f32> {
    let aabb = size.aabb(feet);
    let mut near: f32 = 0.0;
    let mut far = reach;
    for axis in 0..3 {
        let o = origin[axis];
        let d = direction[axis];
        if d.abs() < 1e-6 {
            if o < aabb.min[axis] || o > aabb.max[axis] {
                return None;
            }
        } else {
            let a = (aabb.min[axis] - o) / d;
            let b = (aabb.max[axis] - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

#[derive(Resource)]
struct MobRandom(JavaRandom);
impl Default for MobRandom {
    fn default() -> Self {
        Self(JavaRandom::new(0x4d4f_4253))
    }
}

pub struct MobPlugin;
impl Plugin for MobPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MobRandom>()
            .add_message::<SpawnMob>()
            .add_message::<Explosion>()
            .add_systems(
                Update,
                (
                    natural_spawning,
                    materialize_spawns,
                    tick_creatures,
                    tick_mobs,
                    tick_projectiles,
                    tick_spawners,
                    tick_tnt,
                    apply_explosions,
                )
                    .chain()
                    .after(PhysicsSet::Integrate)
                    .run_if(mobs_active),
            );
    }
}

fn tick_spawners(
    tick: Res<WorldTick>,
    mut chunks: ResMut<WorldChunks>,
    player: Query<&Transform, With<Player>>,
    mobs: Query<(&Mob, &Transform)>,
    light: Res<LightCache>,
    mut requests: MessageWriter<SpawnMob>,
    settings: Option<Res<GameSettings>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
) {
    if tick.ticks_this_frame() == 0
        || settings
            .as_ref()
            .is_some_and(|s| s.difficulty == Difficulty::Peaceful)
    {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPosition::from_world(player.translation.x, player.translation.z);
    let candidates: Vec<_> = chunks
        .positions()
        .filter(|p| (p.x - center.x).abs() <= 2 && (p.z - center.z).abs() <= 2)
        .flat_map(|p| {
            chunks.get(p).into_iter().flat_map(move |chunk| {
                chunk
                    .chunk
                    .spawners()
                    .map(move |(index, spawner)| (p, index, *spawner))
            })
        })
        .collect();
    for (position, index, mut spawner) in candidates {
        let x = position.x * 16 + (index % 16) as i32;
        let y = (index / 256) as i32;
        let z = position.z * 16 + (index / 16 % 16) as i32;
        let center = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5);
        if center.distance_squared(player.translation) > 256.0 {
            continue;
        }
        for _ in 0..tick.ticks_this_frame() {
            if spawner.delay > 0 {
                spawner.delay -= 1;
                continue;
            }
            let mut rng = JavaRandom::from_state(spawner.rng_state);
            if mobs
                .iter()
                .filter(|(mob, transform)| {
                    mob.kind == spawner.kind
                        && (transform.translation - center)
                            .abs()
                            .cmple(Vec3::new(8.0, 4.0, 8.0))
                            .all()
                })
                .count()
                >= 6
            {
                spawner.delay = 200 + rng.next_int(600) as u16;
            } else {
                for _ in 0..4 {
                    let feet = Vec3::new(
                        x as f32 + (rng.next_double() - rng.next_double()) as f32 * 4.0,
                        (y + rng.next_int(3) as i32 - 1) as f32,
                        z as f32 + (rng.next_double() - rng.next_double()) as f32 * 4.0,
                    );
                    if can_spawn_at(
                        spawner.kind,
                        1,
                        feet,
                        &chunks,
                        &light,
                        tick.world_time(),
                        weather.as_ref().map_or(0, |w| w.skylight_penalty()),
                        persistence.as_ref().map_or(0, |p| p.seed()),
                        &mut rng,
                    ) {
                        requests.write(SpawnMob {
                            kind: spawner.kind,
                            feet,
                            explicit: true,
                            variant: 0,
                        });
                        spawner.delay = 200 + rng.next_int(600) as u16;
                    }
                }
            }
            spawner.rng_state = rng.state();
        }
        if let Some(chunk) = chunks.get_mut(position)
            && let Some(stored) = chunk.chunk.spawner_mut(index)
        {
            *stored = spawner;
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(position);
            }
        }
    }
}

fn mobs_active(screen: Option<Res<State<AppScreen>>>) -> bool {
    screen.is_none_or(|s| *s.get() == AppScreen::Playing)
}

/// Spawn a new mob. Creatures face a random direction, as
/// `SpawnerAnimals` places them.
pub fn spawn(commands: &mut Commands, mut mob: Mob, feet: Vec3) -> Entity {
    let yaw = if mob.kind.hostile() {
        0.0
    } else {
        mob.rng.next_float() * 360.0
    };
    spawn_facing(commands, mob, feet, yaw)
}

fn spawn_facing(commands: &mut Commands, mut mob: Mob, feet: Vec3, yaw: f32) -> Entity {
    let size = mob.kind.size(mob.variant);
    let kind = mob.kind;
    let swim = (kind == MobKind::Squid).then(|| Swim::new(&mut mob.rng));
    let entity = commands
        .spawn((
            Name::new(format!("Mob: {}", kind.name())),
            PreviousTick(feet),
            size,
            StepHeight(0.5),
            CollisionState::default(),
            Velocity::default(),
            Transform::from_translation(feet),
            Visibility::Inherited,
            mob,
        ))
        .id();
    let mut entity_commands = commands.entity(entity);
    if kind.hostile() {
        if kind == MobKind::Ghast {
            entity_commands.insert(crate::entity::Flying);
        } else {
            entity_commands.insert(Gravity::DEFAULT);
        }
        return entity;
    }
    entity_commands.insert(Living::facing(yaw));
    // `EntityWolf.canTriggerWalking` is false: wolves never trample.
    if kind != MobKind::Wolf {
        entity_commands.insert(StepDistance::default());
    }
    if kind == MobKind::Chicken {
        entity_commands.insert(Wings::default());
    }
    if let Some(swim) = swim {
        entity_commands.insert(swim);
    }
    entity
}

fn materialize_spawns(
    mut commands: Commands,
    mut requests: MessageReader<SpawnMob>,
    chunks: Res<WorldChunks>,
    settings: Option<Res<GameSettings>>,
    mut rng: ResMut<MobRandom>,
) {
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |s| s.difficulty);
    for request in requests.read() {
        if !request.feet.is_finite()
            || request.feet.y < 0.0
            || request.feet.y >= CHUNK_HEIGHT as f32
            || (difficulty == Difficulty::Peaceful && request.kind.hostile())
            || !chunks.contains(ChunkPosition::from_world(request.feet.x, request.feet.z))
        {
            continue;
        }
        let mut mob = Mob::new(request.kind, rng.0.next_long() as u64);
        if request.kind == MobKind::Slime && request.variant != 0 {
            mob.variant = request.variant;
            mob.health = request.kind.health(mob.variant);
        }
        spawn(&mut commands, mob, request.feet);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpawnCategory {
    Monster,
    Creature,
    Water,
}

pub fn spawn_category(kind: MobKind) -> SpawnCategory {
    if kind == MobKind::Squid {
        SpawnCategory::Water
    } else if kind.hostile() {
        SpawnCategory::Monster
    } else {
        SpawnCategory::Creature
    }
}

/// Beta's weighted biome lists, including wolves only in forests and taiga.
pub fn spawn_table(biome: Biome, category: SpawnCategory) -> &'static [(MobKind, u32)] {
    use MobKind as M;
    const MONSTERS: &[(M, u32)] = &[
        (M::Spider, 10),
        (M::Zombie, 10),
        (M::Skeleton, 10),
        (M::Creeper, 10),
        (M::Slime, 10),
    ];
    const ANIMALS: &[(M, u32)] = &[(M::Sheep, 12), (M::Pig, 10), (M::Chicken, 10), (M::Cow, 8)];
    const WOLVES: &[(M, u32)] = &[
        (M::Sheep, 12),
        (M::Pig, 10),
        (M::Chicken, 10),
        (M::Cow, 8),
        (M::Wolf, 2),
    ];
    match category {
        SpawnCategory::Monster => MONSTERS,
        SpawnCategory::Water => &[(M::Squid, 10)],
        SpawnCategory::Creature if matches!(biome, Biome::Forest | Biome::Taiga) => WOLVES,
        SpawnCategory::Creature => ANIMALS,
    }
}

pub fn can_spawn_at(
    kind: MobKind,
    variant: u8,
    feet: Vec3,
    chunks: &WorldChunks,
    light: &LightCache,
    tick: u64,
    weather_penalty: u8,
    seed: u64,
    rng: &mut JavaRandom,
) -> bool {
    let x = feet.x.floor() as i32;
    let y = feet.y.floor() as i32;
    let z = feet.z.floor() as i32;
    if !(1..CHUNK_HEIGHT as i32 - 2).contains(&y) {
        return false;
    }
    let size = kind.size(variant);
    let aabb = size.aabb(feet);
    let low = aabb.min.floor().as_ivec3();
    let high = aabb.max.ceil().as_ivec3();
    for bx in low.x..high.x {
        for by in low.y..high.y {
            for bz in low.z..high.z {
                let Some(block) = chunks.block_at(bx, by, bz) else {
                    return false;
                };
                if kind == MobKind::Squid {
                    if by == y && !matches!(block, Id::Water | Id::FlowingWater) {
                        return false;
                    }
                } else if crate::block::properties::collision_bounds(block).is_some()
                    || matches!(
                        block,
                        Id::Water | Id::FlowingWater | Id::Lava | Id::FlowingLava
                    )
                {
                    return false;
                }
            }
        }
    }
    if kind == MobKind::Squid {
        return true;
    }
    if !is_opaque_cube(chunks.block_at(x, y - 1, z).unwrap_or(Id::Air)) {
        return false;
    }
    if kind == MobKind::Slime {
        // Chunk.getRandomWithSeed(987234911L), independent of the spawn RNG.
        let pos = ChunkPosition::from_block(x, z);
        let coord = (i64::from(pos.x.wrapping_mul(pos.x).wrapping_mul(4_987_142))
            .wrapping_add(i64::from(pos.x.wrapping_mul(5_947_611)))
            .wrapping_add(i64::from(pos.z.wrapping_mul(pos.z)).wrapping_mul(4_392_871))
            .wrapping_add(i64::from(pos.z.wrapping_mul(389_711)))) as u64;
        return y < 16
            && rng.next_int(10) == 0
            && JavaRandom::new(seed.wrapping_add(coord) ^ 987_234_911).next_int(10) == 0;
    }
    let Some((sky, block)) = light.channels(x, y, z) else {
        return false;
    };
    let subtracted = skylight_subtracted(celestial_angle(tick, 0.0))
        .saturating_add(weather_penalty)
        .min(15);
    if !kind.hostile() {
        return chunks.block_at(x, y - 1, z) == Some(Id::Grass)
            && combined_light(sky, block, subtracted) > 8;
    }
    sky <= rng.next_int(32) as u8 && combined_light(sky, block, subtracted) <= rng.next_int(8) as u8
}

fn natural_spawning(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    light: Res<LightCache>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
    settings: Option<Res<GameSettings>>,
    player: Query<&Transform, With<Player>>,
    mobs: Query<(&Mob, &Transform)>,
    mut requests: MessageWriter<SpawnMob>,
    mut random: ResMut<MobRandom>,
    persistence: Option<Res<crate::world::persistence::WorldPersistence>>,
) {
    if tick.ticks_this_frame() == 0 {
        return;
    }
    let Ok(player) = player.single() else {
        return;
    };
    let center = ChunkPosition::from_world(player.translation.x, player.translation.z);
    let radius = settings.as_ref().map_or(4, |s| s.render_distance.min(8));
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |s| s.difficulty);
    let eligible: Vec<_> = chunks
        .positions()
        .filter(|pos| {
            (pos.x - center.x).abs() <= radius
                && (pos.z - center.z).abs() <= radius
                && light.contains(*pos)
                && (-1..=1).all(|dx| {
                    (-1..=1).all(|dz| {
                        chunks
                            .get(ChunkPosition {
                                x: pos.x + dx,
                                z: pos.z + dz,
                            })
                            .is_some_and(|c| c.populated)
                    })
                })
        })
        .collect();
    if eligible.is_empty() {
        return;
    }
    let seed = persistence.as_ref().map_or(0, |p| p.seed());
    for category in [
        SpawnCategory::Monster,
        SpawnCategory::Creature,
        SpawnCategory::Water,
    ] {
        if category == SpawnCategory::Monster && difficulty == Difficulty::Peaceful {
            continue;
        }
        let cap = match category {
            SpawnCategory::Monster => 70,
            SpawnCategory::Creature => 15,
            SpawnCategory::Water => 5,
        } * eligible.len()
            / 256;
        let mut count = mobs
            .iter()
            .filter(|(m, _)| spawn_category(m.kind) == category)
            .count();
        if count > cap {
            continue;
        }
        // Keep the 20 Hz Beta spawn lottery, but only inspect loaded chunks.
        for &pos in &eligible {
            if count > cap {
                break;
            }
            // Beta counts the outer ring toward the cap but never spawns on it.
            if (pos.x - center.x).abs() == radius || (pos.z - center.z).abs() == radius {
                continue;
            }
            let Some(chunk) = chunks.get(pos) else {
                continue;
            };
            let biome = chunk.biomes.get(8, 8).biome;
            let table = spawn_table(biome, category);
            let sum: u32 = table.iter().map(|(_, weight)| weight).sum();
            let mut roll = random.0.next_int(sum);
            let mut kind = table[0].0;
            for &(candidate, weight) in table {
                if roll < weight {
                    kind = candidate;
                    break;
                }
                roll -= weight;
            }
            let x = pos.x * CHUNK_SIZE as i32 + random.0.next_int(16) as i32;
            let y = random.0.next_int(CHUNK_HEIGHT as u32) as i32;
            let z = pos.z * CHUNK_SIZE as i32 + random.0.next_int(16) as i32;
            if chunks
                .block_at(x, y, z)
                .is_none_or(|b| b != Id::Air && category != SpawnCategory::Water)
            {
                continue;
            }
            let mut group_size = 0;
            for _ in 0..12 {
                let px = x + random.0.next_int(6) as i32 - random.0.next_int(6) as i32;
                let pz = z + random.0.next_int(6) as i32 - random.0.next_int(6) as i32;
                let feet = Vec3::new(px as f32 + 0.5, y as f32, pz as f32 + 0.5);
                let variant = if kind == MobKind::Slime {
                    1 << random.0.next_int(3)
                } else {
                    0
                };
                if feet.distance_squared(player.translation) < 576.0
                    || feet.length_squared() < 576.0
                    || !can_spawn_at(
                        kind,
                        variant as u8,
                        feet,
                        &chunks,
                        &light,
                        tick.world_time(),
                        weather.as_ref().map_or(0, |w| w.skylight_penalty()),
                        seed,
                        &mut random.0,
                    )
                {
                    continue;
                }
                requests.write(SpawnMob {
                    kind,
                    feet,
                    explicit: false,
                    variant: variant as u8,
                });
                count += 1;
                group_size += 1;
                if count > cap || group_size == 4 {
                    break;
                }
            }
        }
    }
}

fn tick_mobs(
    mut commands: Commands,
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    mut player: Query<(&Transform, &mut PlayerHealth), With<Player>>,
    mut mobs: Query<
        (
            Entity,
            &mut Mob,
            &mut Transform,
            &mut Velocity,
            &CollisionState,
            &EntitySize,
            &mut PreviousTick,
        ),
        (Without<Player>, Without<Living>),
    >,
    world: Res<WorldChunks>,
    mut explosions: MessageWriter<Explosion>,
    light: Res<LightCache>,
    weather: Option<Res<crate::world::weather::WorldWeather>>,
) {
    let raining = weather.as_ref().is_some_and(|w| w.is_raining());
    let difficulty = settings
        .as_ref()
        .map_or(Difficulty::Normal, |s| s.difficulty);
    let Ok((player, mut player_health)) = player.single_mut() else {
        return;
    };
    for (entity, mut mob, mut transform, mut velocity, collision, size, mut previous) in &mut mobs {
        if difficulty == Difficulty::Peaceful && mob.kind.hostile() {
            commands.entity(entity).despawn();
            continue;
        }
        let distance = transform.translation.distance_squared(player.translation);
        if !world.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            commands.entity(entity).despawn();
            continue;
        }
        for step in 0..tick.ticks_this_frame() {
            previous.0 = transform.translation;
            let now = tick
                .world_time()
                .saturating_sub(u64::from(tick.ticks_this_frame() - step - 1));
            mob.age = mob.age.saturating_add(1);
            mob.cooldown = mob.cooldown.saturating_sub(1);
            let foot = transform.translation.floor().as_ivec3();
            update_fire(&mut mob, &world, transform.translation, raining);
            if matches!(mob.kind, MobKind::Zombie | MobKind::Skeleton)
                && now % 20 == 0
                && exposed_to_sky(&world, foot)
                && light
                    .channels(foot.x, foot.y + 1, foot.z)
                    .is_some_and(|(sky, _)| sky >= 12)
                && skylight_subtracted(celestial_angle(now, 0.0))
                    .saturating_add(weather.as_ref().map_or(0, |w| w.skylight_penalty()))
                    < 4
            {
                mob.fire_ticks = 160;
            }
            if mob.age > 600 && distance > 32.0 * 32.0 && mob.rng.next_int(800) == 0 {
                commands.entity(entity).despawn();
                break;
            }
            burn(&mut mob, now);
            if mob.health <= 0 {
                kill_mob(&mut commands, entity, &mut mob, transform.translation);
                break;
            }
            if mob.kind == MobKind::Ghast {
                mob.fuse = (mob.fuse - 1).max(0);
                let from = transform.translation + Vec3::Y * 2.0;
                let toward = player.translation - from;
                if distance < 64.0 * 64.0
                    && mob.cooldown == 0
                    && raycast_blocks(&world, from, toward, toward.length()).is_none()
                {
                    launch(
                        &mut commands,
                        transform.translation + Vec3::Y * 2.0,
                        player.translation,
                        true,
                    );
                    mob.cooldown = 60;
                    mob.fuse = 20;
                }
                if mob.age % 50 == 0 {
                    mob.wander_yaw = mob.rng.next_float() * std::f32::consts::TAU;
                }
                velocity.0 = Vec3::new(
                    mob.wander_yaw.cos(),
                    (mob.rng.next_float() - 0.5) * 0.1,
                    mob.wander_yaw.sin(),
                ) * 1.8;
                transform.rotation =
                    Quat::from_rotation_y(-mob.wander_yaw - std::f32::consts::FRAC_PI_2);
                continue;
            }
            let chase = (mob.kind != MobKind::PigZombie
                && (mob.kind != MobKind::Spider || tick.world_time() % 24000 > 12000))
                || mob.angry;
            let eye = transform.translation + Vec3::Y * (size.height * 0.85);
            let toward = player.translation - eye;
            let visible = raycast_blocks(&world, eye, toward, toward.length()).is_none();
            let target = if chase && visible && distance < 256.0 {
                let diff = player.translation - transform.translation;
                Some(Vec3::new(diff.x, 0.0, diff.z).normalize_or_zero())
            } else {
                None
            };
            if mob.kind == MobKind::Skeleton
                && target.is_some()
                && mob.cooldown == 0
                && raycast_blocks(
                    &world,
                    transform.translation + Vec3::Y * 1.5,
                    (player.translation - transform.translation).normalize_or_zero(),
                    distance.sqrt(),
                )
                .is_none()
            {
                launch(
                    &mut commands,
                    transform.translation + Vec3::Y * 1.5,
                    player.translation,
                    false,
                );
                mob.cooldown = 30;
            }
            if mob.wander_ticks == 0 {
                mob.wander_ticks = 20 + mob.rng.next_int(60) as u16;
                mob.wander_yaw = mob.rng.next_float() * std::f32::consts::TAU;
            } else {
                mob.wander_ticks -= 1;
            }
            let dir = target.unwrap_or(Vec3::new(mob.wander_yaw.cos(), 0.0, mob.wander_yaw.sin()));
            let speed = if target.is_some() { 3.2 } else { 1.1 };
            velocity.0.x = dir.x * speed;
            velocity.0.z = dir.z * speed;
            if dir != Vec3::ZERO {
                transform.rotation = Quat::from_rotation_y(-dir.x.atan2(-dir.z));
            }
            if collision.on_ground
                && (collision.collided_x
                    || collision.collided_z
                    || mob.kind == MobKind::Slime && mob.age % 25 == 0)
            {
                velocity.0.y = 8.4;
            }
            if mob.kind == MobKind::Creeper {
                if target.is_some() && distance < if mob.fuse > 0 { 49.0 } else { 9.0 } {
                    mob.fuse += 1;
                } else {
                    mob.fuse = (mob.fuse - 1).max(0);
                }
            } else if visible
                && target.is_some()
                && mob.cooldown == 0
                && distance < (size.width + 1.3).powi(2)
            {
                if mob.kind != MobKind::Skeleton && mob.kind != MobKind::Ghast || mob.angry {
                    if mob.kind == MobKind::Slime && mob.variant == 1 {
                        continue;
                    }
                    let damage = match mob.kind {
                        MobKind::Zombie | MobKind::PigZombie => 5,
                        _ => 2,
                    };
                    player_health.current = player_health
                        .current
                        .saturating_sub(difficulty.mob_damage(damage));
                    mob.cooldown = 20;
                }
            }
            if mob.kind == MobKind::Creeper && mob.fuse >= 30 {
                explosions.write(Explosion {
                    center: transform.translation,
                    strength: if mob.charged { 6.0 } else { 3.0 },
                });
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}

/// Water and rain put a mob out; standing in fire or lava sets it alight.
pub(crate) fn update_fire(mob: &mut Mob, chunks: &WorldChunks, feet: Vec3, raining: bool) {
    let foot = feet.floor().as_ivec3();
    let occupied = chunks.block_at(foot.x, foot.y, foot.z);
    let in_water = matches!(occupied, Some(Id::Water | Id::FlowingWater));
    if in_water || raining && exposed_to_sky(chunks, foot) {
        mob.fire_ticks = 0;
    } else if matches!(occupied, Some(Id::Fire | Id::Lava | Id::FlowingLava)) {
        mob.fire_ticks = if occupied == Some(Id::Fire) { 160 } else { 300 };
    }
}

/// A burning mob loses a heart-half each second until the fire runs out.
pub(crate) fn burn(mob: &mut Mob, now: u64) {
    if mob.fire_ticks > 0 {
        mob.fire_ticks -= 1;
        if now % 20 == 0 && !matches!(mob.kind, MobKind::Ghast | MobKind::PigZombie) {
            mob.health -= 1;
        }
    }
}

fn exposed_to_sky(chunks: &WorldChunks, foot: IVec3) -> bool {
    ((foot.y + 1).max(0)..CHUNK_HEIGHT as i32).all(|y| {
        chunks
            .block_at(foot.x, y, foot.z)
            .is_some_and(|block| !is_opaque_cube(block))
    })
}

fn launch(commands: &mut Commands, origin: Vec3, target: Vec3, fireball: bool) {
    let direction = (target - origin).normalize_or_zero();
    commands.spawn((
        Transform::from_translation(origin),
        MobProjectile {
            velocity: direction * if fireball { 12.0 } else { 17.0 },
            fireball,
            age: 0,
        },
    ));
}

fn tick_projectiles(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut projectiles: Query<(Entity, &mut Transform, &mut MobProjectile), Without<Player>>,
    mut player: Query<(&Transform, &mut PlayerHealth), With<Player>>,
    world: Res<WorldChunks>,
    settings: Option<Res<GameSettings>>,
    mut explosions: MessageWriter<Explosion>,
) {
    for (entity, mut transform, mut projectile) in &mut projectiles {
        for _ in 0..tick.ticks_this_frame() {
            projectile.age += 1;
            let from = transform.translation;
            let motion = projectile.velocity * 0.05;
            transform.translation += motion;
            if !projectile.fireball {
                projectile.velocity.y -= 0.4;
            }
            transform.rotation =
                Quat::from_rotation_arc(Vec3::NEG_Z, projectile.velocity.normalize_or_zero());
            let player_hit = player.single_mut().is_ok_and(|(target, _)| {
                target.translation.distance_squared(transform.translation) < 0.9
            });
            let blocked = raycast_blocks(&world, from, motion, motion.length()).is_some();
            if player_hit || blocked || projectile.age > 120 {
                if projectile.fireball {
                    explosions.write(Explosion {
                        center: transform.translation,
                        strength: 1.0,
                    });
                }
                if player_hit && let Ok((_, mut health)) = player.single_mut() {
                    let damage = if projectile.fireball { 6 } else { 4 };
                    health.current = health.current.saturating_sub(
                        settings
                            .as_ref()
                            .map_or(Difficulty::Normal, |s| s.difficulty)
                            .mob_damage(damage),
                    );
                }
                commands.entity(entity).despawn();
                break;
            }
        }
    }
}

fn tick_tnt(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut tnt: Query<(Entity, &mut PrimedTnt, &Transform)>,
    mut explosions: MessageWriter<Explosion>,
) {
    for (entity, mut tnt, transform) in &mut tnt {
        if tick.ticks_this_frame() == 0 {
            continue;
        }
        if tnt.fuse > tick.ticks_this_frame() as u16 {
            tnt.fuse -= tick.ticks_this_frame() as u16;
        } else {
            explosions.write(Explosion {
                center: transform.translation,
                strength: 4.0,
            });
            commands.entity(entity).despawn();
        }
    }
}

fn apply_explosions(
    mut commands: Commands,
    mut explosions: MessageReader<Explosion>,
    mut chunks: ResMut<WorldChunks>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut mobs: Query<(&mut Mob, &Transform)>,
    mut player: Query<(&Transform, &mut PlayerHealth), With<Player>>,
) {
    for &explosion in explosions.read() {
        explode(
            &mut commands,
            &mut chunks,
            &mut ticks,
            &mut streaming,
            &mut persistence,
            explosion.center,
            explosion.strength,
        );
        let radius = explosion.strength * 2.0;
        for (mut mob, transform) in &mut mobs {
            let distance = transform.translation.distance(explosion.center);
            if distance < radius {
                mob.health -= ((1.0 - distance / radius) * explosion.strength * 7.0) as i16;
            }
        }
        if let Ok((transform, mut health)) = player.single_mut() {
            let distance = transform.translation.distance(explosion.center);
            if distance < radius {
                health.current = health
                    .current
                    .saturating_sub(((1.0 - distance / radius) * explosion.strength * 7.0) as u8);
            }
        }
    }
}

fn explode(
    commands: &mut Commands,
    chunks: &mut WorldChunks,
    ticks: &mut Option<ResMut<BlockTicks>>,
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    center: Vec3,
    strength: f32,
) {
    let c = center.floor().as_ivec3();
    let radius = strength.ceil() as i32;
    for x in c.x - radius..=c.x + radius {
        for y in (c.y - radius).max(0)..=(c.y + radius).min(CHUNK_HEIGHT as i32 - 1) {
            for z in c.z - radius..=c.z + radius {
                if Vec3::new(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5).distance(center)
                    > strength
                {
                    continue;
                }
                let Some(block) = chunks.block_at(x, y, z) else {
                    continue;
                };
                if matches!(
                    block,
                    Id::Air
                        | Id::Bedrock
                        | Id::Water
                        | Id::FlowingWater
                        | Id::Lava
                        | Id::FlowingLava
                ) {
                    continue;
                }
                if matches!(block, Id::Obsidian | Id::MobSpawner) {
                    continue;
                }
                let at = IVec3::new(x, y, z);
                let meta = chunks.metadata_at(x, y, z);
                if let Some(previous) = chunks.set_block(x, y, z, Id::Air) {
                    if previous == Id::Tnt {
                        prime_tnt(
                            commands,
                            Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5),
                            15,
                        );
                    }
                    if let Some(ticks) = ticks.as_deref_mut() {
                        ticks.block_changed(at, previous, meta);
                    }
                    if let Some(streaming) = streaming.as_deref_mut() {
                        streaming.request_block_update(x, y, z);
                    }
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(ChunkPosition::from_block(x, z));
                    }
                }
            }
        }
    }
}

pub(crate) fn kill_mob(commands: &mut Commands, entity: Entity, mob: &mut Mob, feet: Vec3) {
    if mob.kind == MobKind::Slime && mob.variant > 1 {
        for offset in [
            Vec3::new(-0.3, 0.0, -0.3),
            Vec3::new(0.3, 0.0, -0.3),
            Vec3::new(-0.3, 0.0, 0.3),
            Vec3::new(0.3, 0.0, 0.3),
        ] {
            let mut child = Mob::new(MobKind::Slime, mob.rng.next_long() as u64);
            child.variant = mob.variant / 2;
            child.health = MobKind::Slime.health(child.variant);
            spawn(commands, child, feet + offset);
        }
    } else if mob.kind == MobKind::Sheep && !mob.sheared {
        drop_item(
            commands,
            ItemId::from_u16(35).unwrap(),
            u16::from(mob.variant),
            feet,
        );
    } else {
        let item = match mob.kind {
            MobKind::Spider => Some(ItemId::String),
            MobKind::Zombie => Some(ItemId::Feather),
            MobKind::Skeleton => Some(ItemId::Arrow),
            MobKind::Creeper | MobKind::Ghast => Some(ItemId::Gunpowder),
            MobKind::Slime if mob.variant == 1 => Some(ItemId::Slimeball),
            MobKind::Pig => Some(if mob.fire_ticks > 0 {
                ItemId::CookedPorkchop
            } else {
                ItemId::RawPorkchop
            }),
            MobKind::Chicken => Some(ItemId::Feather),
            MobKind::Cow => Some(ItemId::Leather),
            MobKind::Squid => Some(ItemId::Dye),
            MobKind::PigZombie => Some(ItemId::CookedPorkchop),
            _ => None,
        };
        if let Some(item) = item {
            for _ in 0..mob.rng.next_int(3) {
                drop_item(commands, item, 0, feet);
            }
        }
        if mob.kind == MobKind::Skeleton {
            for _ in 0..mob.rng.next_int(3) {
                drop_item(commands, ItemId::Bone, 0, feet);
            }
        }
    }
    commands.entity(entity).despawn();
}

pub fn drop_item(commands: &mut Commands, item: ItemId, data: u16, feet: Vec3) {
    if let Ok(stack) = ItemStack::with_data(item, 1, data) {
        let cell = feet.floor().as_ivec3();
        let mut rng = crate::random::ItemRng::default();
        crate::entity::drops::items::spawn_block_drop(commands, &mut rng, cell, stack);
    }
}

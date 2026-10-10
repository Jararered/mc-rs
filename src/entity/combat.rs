//! Beta 1.7.3 damage.
//!
//! [`hurt_creature`] and [`hurt_player`] transcribe `EntityLiving.attackEntityFrom`:
//! a hit starts a 20-tick invulnerability window (`heartsLife`), and during
//! its first half only a harder hit lands, and then only for the difference.
//! A fresh hit flashes the target red (`hurtTime`) and knocks it away from the
//! attacker. The player's version adds `EntityPlayer`'s difficulty scaling for
//! monsters and arrows, and armor (`damageEntity`). [`drop_loot`] is each
//! creature's `dropFewItems`, which Beta runs the moment health runs out; the
//! body then tips over for 20 ticks before [`remove_dead`] takes it away.

use bevy::prelude::*;

use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::drops::items::spawn_entity_drop;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mobs::spawn;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::segment_entry;
use crate::player::PlayerHealth;
use crate::random::ItemRng;
use crate::world::difficulty::Difficulty;
use crate::world::tick::TICK_SECONDS;
use crate::world::tick::WorldTick;

/// `heartsHalvesLife`: how long a hit keeps its target invulnerable.
const INVULNERABLE_TICKS: i16 = 20;
/// `maxHurtTime`: how long a fresh hit flashes the target red.
pub const HURT_TICKS: i16 = 10;
/// `EntityLiving.deathTime` passes this before the body is removed.
pub const DEATH_TICKS: i16 = 20;

/// What dealt a hit. It decides difficulty scaling and how the target reacts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Fire, lava, drowning, suffocation, falling, or an explosion without an
    /// exploder.
    Environment,
    /// The player's own swing.
    Player,
    /// An `EntityMob` or an arrow one shot. Difficulty scales these against
    /// the player.
    Monster,
    /// Any other creature: a wolf's bite, a slime's touch, a ghast's fireball.
    Creature,
}

/// One `attackEntityFrom` call.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub amount: i16,
    /// Where the attacker stands. The target is knocked away from it.
    pub from: Option<Vec3>,
    pub source: Source,
}

impl Hit {
    pub fn environment(amount: i16) -> Self {
        Self {
            amount,
            from: None,
            source: Source::Environment,
        }
    }
}

/// What a hit did to a creature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Wound {
    /// `attackEntityFrom` returned true: the hit landed, even if partly.
    pub landed: bool,
    /// This hit took the last of its health. Drop its loot now.
    pub died: bool,
}

/// The player's side of `EntityLiving`'s damage state.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PlayerCombat {
    /// `heartsLife`.
    pub hearts_life: i16,
    /// `field_9346_af`: the hit that opened the invulnerability window.
    pub last_damage: i16,
    /// `hurtTime`: counts down from 10 after a hit and tilts the camera.
    pub hurt_time: i16,
    pub prev_hurt_time: i16,
    /// `attackedAtYaw`, in degrees relative to where the player faces.
    pub attacked_at_yaw: f32,
    /// No hit lands. `player::survival` sets this from the player's game mode.
    pub invulnerable: bool,
    /// `EntityPlayer.damageRemainder`: armor's carried fraction of a point.
    damage_remainder: i32,
}

/// The parts of the player a hit changes.
pub struct Victim<'a> {
    pub health: &'a mut PlayerHealth,
    pub combat: &'a mut PlayerCombat,
    pub velocity: &'a mut Velocity,
    pub armor: &'a mut [Option<ItemStack>; 4],
    /// The player's `pos`, at eye height.
    pub eye: Vec3,
    /// Beta `rotationYaw`, in degrees.
    pub yaw: f32,
}

/// `EntityPlayer.attackEntityFrom` and `damageEntity`.
pub fn hurt_player(
    victim: &mut Victim,
    mut hit: Hit,
    difficulty: Difficulty,
    rng: &mut ItemRng,
) -> bool {
    if victim.health.current == 0 || victim.combat.invulnerable {
        return false;
    }
    if hit.source == Source::Monster {
        hit.amount = i16::from(difficulty.mob_damage(hit.amount.clamp(0, 255) as u8));
    }
    if hit.amount <= 0 {
        return false;
    }
    let combat = &mut *victim.combat;
    let (amount, fresh) =
        match open_window(&mut combat.hearts_life, &mut combat.last_damage, hit.amount) {
            Some(window) => window,
            None => return false,
        };
    // `damageEntity`: armor absorbs part of each point, carrying the rest.
    let armor = armor_value(victim.armor);
    let scaled = i32::from(amount) * (25 - armor) + combat.damage_remainder;
    damage_armor(victim.armor, amount);
    combat.damage_remainder = scaled % 25;
    let taken = (scaled / 25).clamp(0, 255) as u8;
    victim.health.current = victim.health.current.saturating_sub(taken);

    combat.attacked_at_yaw = 0.0;
    if fresh {
        combat.hurt_time = HURT_TICKS;
        let mut motion = victim.velocity.0 * TICK_SECONDS;
        combat.attacked_at_yaw =
            knock_from(&mut motion, victim.eye, hit, victim.yaw, || rng.unit());
        victim.velocity.0 = motion / TICK_SECONDS;
    }
    true
}

/// `EntityLiving.attackEntityFrom` for a creature. Being struck by the
/// player turns a monster on them.
pub fn hurt_creature(
    mob: &mut Mob,
    living: &mut Living,
    velocity: &mut Velocity,
    feet: Vec3,
    mut hit: Hit,
) -> Wound {
    if mob.kind == MobType::Wolf {
        // `EntityWolf.attackEntityFrom` stands the wolf up and halves blows
        // from any attacker but a player. An arrow names its shooter as the
        // attacker, so it is halved too.
        mob.sitting = false;
        if matches!(hit.source, Source::Monster | Source::Creature) {
            hit.amount = (hit.amount + 1) / 2;
        }
    }
    living.entity_age = 0;
    if mob.health <= 0 {
        return Wound::default();
    }
    living.limb_amount = 1.5;
    let Some((amount, fresh)) =
        open_window(&mut living.hearts_life, &mut living.last_damage, hit.amount)
    else {
        return Wound::default();
    };
    mob.health -= amount;
    living.attacked_at_yaw = 0.0;
    if fresh {
        living.hurt_time = HURT_TICKS;
        let mut motion = velocity.0 * TICK_SECONDS;
        let rng = &mut mob.rng;
        living.attacked_at_yaw =
            knock_from(&mut motion, feet, hit, living.yaw, || rng.next_float());
        velocity.0 = motion / TICK_SECONDS;
    }
    // `EntityMob.attackEntityFrom` targets whoever hit it.
    if hit.source == Source::Player && mob.kind.hostile() && mob.kind != MobType::Slime {
        living.chasing = true;
    }
    Wound {
        landed: true,
        died: mob.health <= 0,
    }
}

/// The entity under the crosshair: the nearest box the ray from `eye` along
/// `look` enters within `reach`. An eye already inside a box picks it.
pub fn pick<T: Copy>(
    eye: Vec3,
    look: Vec3,
    reach: f32,
    candidates: impl IntoIterator<Item = (T, Aabb)>,
) -> Option<T> {
    let look = look.normalize_or_zero();
    let mut best: Option<(f32, T)> = None;
    for (candidate, aabb) in candidates {
        let Some(distance) = segment_entry(eye, look, aabb.min, aabb.max, reach) else {
            continue;
        };
        if best.is_none_or(|(nearest, _)| distance < nearest) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// `AxisAlignedBB.expand` by the same amount on every side.
pub fn bordered(aabb: Aabb, border: f32) -> Aabb {
    Aabb::new(
        aabb.min - Vec3::splat(border),
        aabb.max + Vec3::splat(border),
    )
}

/// The invulnerability rule. Returns the damage to apply and whether this
/// hit opened a fresh window (and so knocks back and flashes), or `None` when
/// it is absorbed.
fn open_window(hearts_life: &mut i16, last_damage: &mut i16, amount: i16) -> Option<(i16, bool)> {
    if *hearts_life > INVULNERABLE_TICKS / 2 {
        if amount <= *last_damage {
            return None;
        }
        let extra = amount - *last_damage;
        *last_damage = amount;
        Some((extra, false))
    } else {
        *last_damage = amount;
        *hearts_life = INVULNERABLE_TICKS;
        Some((amount, true))
    }
}

/// `EntityLiving.knockBack` away from the attacker, returning the new
/// `attackedAtYaw`. With no attacker, Beta picks 0° or 180° at random.
fn knock_from(
    motion: &mut Vec3,
    target: Vec3,
    hit: Hit,
    yaw: f32,
    mut random: impl FnMut() -> f32,
) -> f32 {
    let Some(from) = hit.from else {
        return if random() < 0.5 { 0.0 } else { 180.0 };
    };
    let mut dx = from.x - target.x;
    let mut dz = from.z - target.z;
    while dx * dx + dz * dz < 1.0e-4 {
        dx = (random() - random()) * 0.01;
        dz = (random() - random()) * 0.01;
    }
    let length = (dx * dx + dz * dz).sqrt();
    *motion /= 2.0;
    motion.x -= dx / length * 0.4;
    motion.y = (motion.y + 0.4).min(0.4);
    motion.z -= dz / length * 0.4;
    dz.atan2(dx).to_degrees() - yaw
}

/// `ItemArmor.damageReduceAmount` by slot: helmet, chestplate, leggings, boots.
fn armor_points(item: Item) -> Option<i32> {
    Some(i32::from(item.properties()?.armor?.points))
}

/// `InventoryPlayer.getTotalArmorValue`: each piece's points, scaled by how
/// worn the whole set is.
pub fn armor_value(armor: &[Option<ItemStack>; 4]) -> i32 {
    let mut points = 0;
    let mut left = 0;
    let mut total = 0;
    for stack in armor.iter().flatten() {
        let Some(piece) = armor_points(stack.item()) else {
            continue;
        };
        let crate::item::ItemData::Durability(max) = stack.definition().data else {
            continue;
        };
        let max = i32::from(max);
        left += max - i32::from(stack.data());
        total += max;
        points += piece;
    }
    if total == 0 {
        0
    } else {
        (points - 1) * left / total + 1
    }
}

/// `InventoryPlayer.damageArmor`: every worn piece takes the hit's damage.
fn damage_armor(armor: &mut [Option<ItemStack>; 4], amount: i16) {
    for slot in armor.iter_mut() {
        if let Some(stack) = *slot
            && armor_points(stack.item()).is_some()
        {
            *slot = stack.apply_damage(amount.max(0) as u16);
        }
    }
}

/// Count down the player's invulnerability and hurt flash once per tick.
pub(crate) fn tick_player_combat(tick: Res<WorldTick>, mut players: Query<&mut PlayerCombat>) {
    for mut combat in &mut players {
        for _ in 0..tick.ticks_this_frame() {
            combat.prev_hurt_time = combat.hurt_time;
            combat.hurt_time = (combat.hurt_time - 1).max(0);
            combat.hearts_life = (combat.hearts_life - 1).max(0);
        }
    }
}

/// `dropFewItems`, run when a creature's health runs out.
pub fn drop_loot(commands: &mut Commands, rng: &mut ItemRng, mob: &mut Mob, feet: Vec3) {
    let mut drop = |commands: &mut Commands, item: Item, data: u16| {
        if let Ok(stack) = ItemStack::with_data(item, 1, data) {
            spawn_entity_drop(commands, rng, feet, stack);
        }
    };
    let common = match mob.kind {
        MobType::Sheep => {
            if !mob.sheared {
                drop(
                    commands,
                    Item::from_u16(35).expect("wool"),
                    u16::from(mob.variant),
                );
            }
            return;
        }
        MobType::Squid => {
            for _ in 0..mob.rng.next_int(3) + 1 {
                drop(commands, Item::Dye, 0);
            }
            return;
        }
        MobType::Skeleton => {
            for _ in 0..mob.rng.next_int(3) {
                drop(commands, Item::Arrow, 0);
            }
            for _ in 0..mob.rng.next_int(3) {
                drop(commands, Item::Bone, 0);
            }
            return;
        }
        MobType::Pig if mob.fire_ticks > 0 => Item::CookedPorkchop,
        MobType::Pig => Item::RawPorkchop,
        MobType::Cow => Item::Leather,
        MobType::Chicken | MobType::Zombie => Item::Feather,
        MobType::Spider => Item::String,
        MobType::Creeper | MobType::Ghast => Item::Gunpowder,
        MobType::PigZombie => Item::CookedPorkchop,
        MobType::Slime if mob.variant <= 1 => Item::Slimeball,
        MobType::Slime | MobType::Wolf => return,
    };
    for _ in 0..mob.rng.next_int(3) {
        drop(commands, common, 0);
    }
}

/// `setEntityDead` once the death animation has played. A big slime whose
/// health ran out exactly splits into four of half its size; overkill leaves
/// none, as in Beta.
pub fn remove_dead(commands: &mut Commands, entity: Entity, mob: &mut Mob, feet: Vec3) {
    let size = mob.variant.max(1);
    if mob.kind == MobType::Slime && size > 1 && mob.health == 0 {
        for i in 0..4 {
            let offset = Vec3::new(
                ((i % 2) as f32 - 0.5) * f32::from(size) / 4.0,
                0.5,
                ((i / 2) as f32 - 0.5) * f32::from(size) / 4.0,
            );
            let mut child = Mob::new(MobType::Slime, mob.rng.next_long() as u64);
            child.variant = size / 2;
            child.health = MobType::Slime.health(child.variant);
            spawn(commands, child, feet + offset);
        }
    }
    commands.entity(entity).despawn();
}

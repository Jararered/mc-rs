//! The wolf packs Beta calls on from outside a wolf's own tick: the pack a
//! struck wild wolf rallies (`EntityWolf.attackEntityFrom`), and the tamed
//! wolves a player sends after whatever hurts them or whatever they hit
//! (`EntityPlayer.alertWolves`).

use bevy::prelude::*;

use super::Living;
use crate::entity::combat::Assailant;
use crate::entity::combat::Attacker;
use crate::entity::combat::Hit;
use crate::entity::combat::Source;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::physics::Aabb;
use crate::world::difficulty::Difficulty;

/// A wolf a pack call can reach.
pub(crate) struct PackWolf<'a> {
    pub mob: &'a mut Mob,
    pub living: &'a mut Living,
    pub aabb: Aabb,
}

/// `AxisAlignedBB.getBoundingBoxFromPool(posX, posY, posZ, posX + 1, ...)
/// .expand(16, 4, 16)`, the reach of both calls.
fn pack_reach(pos: Vec3) -> Aabb {
    let grow = Vec3::new(16.0, 4.0, 16.0);
    Aabb::new(pos - grow, pos + Vec3::ONE + grow)
}

/// `EntityWolf.attackEntityFrom`: every wild wolf within reach of the struck
/// one with no target of its own, the struck one included, turns on
/// `attacker`. A player angers them too.
pub(crate) fn rally_pack<'a>(
    wolves: impl IntoIterator<Item = PackWolf<'a>>,
    struck: Vec3,
    attacker: Assailant,
) {
    let reach = pack_reach(struck);
    for wolf in wolves {
        if wolf.mob.kind != MobType::Wolf
            || wolf.mob.tamed
            || wolf.living.chasing
            || !reach.intersects(wolf.aabb)
        {
            continue;
        }
        wolf.living.set_target(Some(attacker.entity()));
        if attacker.is_player() {
            wolf.mob.angry = true;
        }
    }
}

/// `EntityPlayer.alertWolves`: the tamed wolves near `owner` with no target
/// of their own stand up and go for `attacker`. When the owner was the one
/// who struck (`keep_sitting`), a sitting wolf stays put. Creepers, ghasts
/// and other players are left alone, as is a wolf the owner tamed.
pub(crate) fn alert_wolves<'a>(
    wolves: impl IntoIterator<Item = PackWolf<'a>>,
    owner: Vec3,
    attacker: Assailant,
    attacker_tamed: bool,
    keep_sitting: bool,
) {
    match attacker {
        Assailant::Player(_) | Assailant::Creature(_, MobType::Creeper | MobType::Ghast) => return,
        // `owner` is the same name for every player, so a tamed wolf is
        // always the player's own.
        Assailant::Creature(_, MobType::Wolf) if attacker_tamed => return,
        Assailant::Creature(..) => {}
    }
    let reach = pack_reach(owner);
    for wolf in wolves {
        if wolf.mob.kind != MobType::Wolf
            || !wolf.mob.tamed
            || wolf.living.chasing
            || keep_sitting && wolf.mob.sitting
            || !reach.intersects(wolf.aabb)
        {
            continue;
        }
        wolf.mob.sitting = false;
        wolf.living.set_target(Some(attacker.entity()));
    }
}

/// Whether `EntityPlayer.attackEntityFrom` reaches its `alertWolves` call
/// for this hit, and on whom: a creature, or an arrow's creature owner,
/// whose blow survives the difficulty scaling.
pub(crate) fn alerted_by(hit: &Hit, difficulty: Difficulty) -> Option<Assailant> {
    let assailant = hit.attacker.and_then(Attacker::assailant)?;
    let amount = if hit.source == Source::Monster {
        i16::from(difficulty.mob_damage(hit.amount.clamp(0, 255) as u8))
    } else {
        hit.amount
    };
    (amount > 0 && !assailant.is_player()).then_some(assailant)
}

/// The mobs a projectile can strike, as the arrow, thrown-item and bobber
/// systems query them.
pub(crate) type StruckMobs<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Mob,
        &'static mut Living,
        &'static mut crate::entity::Velocity,
        &'static Transform,
        &'static crate::entity::EntitySize,
    ),
    Without<crate::player::Player>,
>;

/// Every wolf among `mobs`, for a pack call.
pub(crate) fn pack_of<'a>(mobs: &'a mut StruckMobs) -> impl Iterator<Item = PackWolf<'a>> {
    mobs.iter_mut()
        .filter(|(_, mob, ..)| mob.kind == MobType::Wolf)
        .map(|(_, mob, living, _, transform, size)| PackWolf {
            mob: mob.into_inner(),
            living: living.into_inner(),
            aabb: size.aabb(transform.translation),
        })
}

/// After a projectile's blow on a creature: a struck wild wolf calls its
/// pack.
pub(crate) fn after_projectile_hit(mobs: &mut StruckMobs, struck: Vec3, rally: Option<Assailant>) {
    if let Some(attacker) = rally {
        rally_pack(pack_of(mobs), struck, attacker);
    }
}

/// After a projectile's blow on a player: their tamed wolves go for the
/// creature behind it.
pub(crate) fn alert_from_projectile(
    mobs: &mut StruckMobs,
    owner: Vec3,
    attacker: Option<Assailant>,
) {
    let Some(attacker) = attacker else {
        return;
    };
    let tamed = mobs
        .get(attacker.entity())
        .is_ok_and(|(_, mob, ..)| mob.tamed);
    alert_wolves(pack_of(mobs), owner, attacker, tamed, false);
}

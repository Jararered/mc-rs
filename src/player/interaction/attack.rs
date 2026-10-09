//! Hitting and using mobs.
//!
//! `EntityRenderer.getMouseOver` picks the entity under the crosshair: the
//! nearest whose box, grown by its collision border (0.1 for mobs, 1 for a
//! fireball), the look ray enters within 3 blocks and before any block it
//! hits. A left click is `EntityPlayer.attackTargetEntityWithCurrentItem`;
//! a right click is the mob's `interact`.

use bevy::ecs::query::QueryData;
use bevy::prelude::*;

use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::combat::Hit;
use crate::entity::combat::Source;
use crate::entity::combat::bordered;
use crate::entity::combat::drop_loot;
use crate::entity::combat::hurt_creature;
use crate::entity::creature::Living;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mobs::drop_item;
use crate::entity::mount::Seat;
use crate::entity::mount::dismount;
use crate::entity::mount::mount;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::item::tools::damage_vs_entity;
use crate::item::tools::hit_durability;
use crate::physics::Aabb;
use crate::random::ItemRng;
use crate::rendering::particles::effects::EffectParticles;

/// Beta's survival entity reach.
pub const ENTITY_REACH: f32 = 3.0;
/// `Entity.getCollisionBorderSize`.
pub const MOB_BORDER: f32 = 0.1;
/// `EntityFireball.getCollisionBorderSize`.
pub const FIREBALL_BORDER: f32 = 1.0;

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct MobTarget {
    pub entity: Entity,
    pub mob: &'static mut Mob,
    pub living: &'static mut Living,
    pub velocity: &'static mut Velocity,
    pub transform: &'static Transform,
    pub size: &'static EntitySize,
    pub seat: Option<&'static Seat>,
}

/// `attackTargetEntityWithCurrentItem`. Falling adds a point, as any
/// downward motion does in Beta (so does standing, which pulls the player
/// down by a hair each tick). Swords wear by 1 a swing and tools by 2.
pub(crate) fn attack<F: bevy::ecs::query::QueryFilter>(
    commands: &mut Commands,
    loot: &mut ItemRng,
    mobs: &mut Query<MobTarget, F>,
    target: Entity,
    eye: Vec3,
    falling: bool,
    hotbar: &mut Hotbar,
) {
    let held = hotbar.selected_stack();
    let amount = damage_vs_entity(held) + i16::from(falling);
    let Ok(struck) = mobs.get(target) else {
        return;
    };
    let kind = struck.mob.kind;
    let reach = struck.size.aabb(struck.transform.translation);
    let was_wild_and_calm = kind == MobType::Wolf && !struck.mob.tamed && !struck.mob.angry;
    if kind == MobType::PigZombie {
        // `EntityPigZombie.attackEntityFrom`: every zombie pigman within 32
        // blocks turns on the player, whether or not the blow lands.
        let near = bordered(reach, 32.0);
        for mut other in mobs.iter_mut() {
            if other.mob.kind == MobType::PigZombie
                && near.intersects(other.size.aabb(other.transform.translation))
            {
                other.mob.angry = true;
                other.living.chasing = true;
            }
        }
    }
    let Ok(mut struck) = mobs.get_mut(target) else {
        return;
    };
    let feet = struck.transform.translation;
    let hit = Hit {
        amount,
        from: Some(eye),
        source: Source::Player,
    };
    let wound = hurt_creature(
        &mut struck.mob,
        &mut struck.living,
        &mut struck.velocity,
        feet,
        hit,
    );
    if wound.died {
        drop_loot(commands, loot, &mut struck.mob, feet);
    }
    if wound.landed && kind == MobType::Wolf {
        // `EntityWolf.attackEntityFrom`: a wild wolf turns on the player and
        // calls the pack within 16 blocks. A tamed wolf forgives its owner.
        if was_wild_and_calm {
            let pack = Aabb::new(
                reach.min - Vec3::new(16.0, 4.0, 16.0),
                reach.max + Vec3::new(16.0, 4.0, 16.0),
            );
            for mut other in mobs.iter_mut() {
                let joins = other.entity == target
                    || other.mob.kind == MobType::Wolf
                        && !other.mob.tamed
                        && !other.living.chasing
                        && pack.intersects(other.size.aabb(other.transform.translation));
                if joins {
                    other.mob.angry = true;
                    other.living.chasing = true;
                }
            }
        } else if let Ok(mut struck) = mobs.get_mut(target)
            && !struck.mob.tamed
        {
            struck.living.chasing = true;
        }
    }
    if let Some(stack) = held {
        hotbar.damage_selected(hit_durability(stack));
    }
}

/// Each mob's `interact`: shear a sheep, saddle or ride a pig, milk a cow, or
/// tame, feed, and seat a wolf. `rider` is the player.
pub(crate) fn interact<F: bevy::ecs::query::QueryFilter>(
    commands: &mut Commands,
    loot: &mut ItemRng,
    mobs: &mut Query<MobTarget, F>,
    target: Entity,
    rider: Entity,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
    mut effects: Option<&mut EffectParticles>,
) {
    let Ok(mut struck) = mobs.get_mut(target) else {
        return;
    };
    let feet = struck.transform.translation;
    let seated = struck.seat.and_then(|seat| seat.rider);
    let (width, height) = (struck.size.width, struck.size.height);
    let held = hotbar.selected_stack().map(ItemStack::item);
    let mob = &mut *struck.mob;
    match mob.kind {
        MobType::Sheep if held == Some(Item::Shears) && !mob.sheared => {
            // `EntitySheep.interact`: two to four wool, popped a block up.
            mob.sheared = true;
            for _ in 0..2 + mob.rng.next_int(3) {
                let wool = Item::from_u16(35).expect("wool");
                drop_item(commands, loot, wool, u16::from(mob.variant), feet + Vec3::Y);
            }
            hotbar.damage_selected(1);
        }
        MobType::Pig if held == Some(Item::Saddle) && !mob.saddled => {
            mob.saddled = true;
            hotbar.take_selected(1);
        }
        // `EntityPig.interact`: climb on, or off again (`mountEntity`
        // toggles). Somebody else's pig stays theirs.
        MobType::Pig if mob.saddled && mob.health > 0 => {
            if seated == Some(rider) {
                dismount(commands, rider);
            } else if seated.is_none() {
                mount(commands, rider, target);
            }
        }
        MobType::Cow if held == Some(Item::Bucket) => {
            let milk = ItemStack::new(Item::MilkBucket, 1).expect("registered bucket");
            if hotbar
                .selected_stack()
                .is_some_and(|stack| stack.count() == 1)
            {
                let selected = hotbar.selected;
                hotbar.slots[selected] = Some(milk);
            } else {
                hotbar.take_selected(1);
                if let Some(overflow) = inventory.insert(hotbar, milk) {
                    spawn_block_drop(commands, loot, feet.floor().as_ivec3(), overflow);
                }
            }
        }
        MobType::Wolf if mob.tamed => {
            if matches!(held, Some(Item::RawPorkchop | Item::CookedPorkchop)) && mob.health < 20 {
                hotbar.take_selected(1);
                mob.health = (mob.health + 3).min(20);
            } else {
                mob.sitting = !mob.sitting;
            }
        }
        MobType::Wolf if held == Some(Item::Bone) && !mob.angry => {
            hotbar.take_selected(1);
            let tamed = mob.rng.next_int(3) == 0;
            if tamed {
                mob.tamed = true;
                mob.sitting = true;
                mob.health = 20;
                mob.owner = Some("Player".to_owned());
                struck.living.chasing = false;
            }
            if let Some(effects) = effects.as_deref_mut() {
                effects.tame_burst(feet, width, height, tamed);
            }
        }
        _ => {}
    }
}

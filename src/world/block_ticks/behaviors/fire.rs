//! Beta `BlockFire`: scheduled age, weather extinction and material-specific spread.
use crate::block::id::Id;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Fire;
pub static FIRE: Fire = Fire;

fn rates(block: Id) -> (u32, u32) {
    match block {
        Id::WoodenPlanks | Id::SprucePlanks | Id::BirchPlanks | Id::Fence | Id::WoodenStairs => {
            (5, 20)
        }
        Id::Wood | Id::SpruceWood | Id::BirchWood => (5, 5),
        Id::Leaves | Id::SpruceLeaves | Id::BirchLeaves => (30, 60),
        Id::Bookshelf => (30, 20),
        Id::Tnt => (15, 100),
        Id::TallGrass | Id::Fern => (60, 100),
        Id::Wool => (30, 60),
        _ => (0, 0),
    }
}

fn nearby(world: &TickWorld, pos: IVec3) -> bool {
    for delta in [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ] {
        if rates(world.block(pos + delta)).0 > 0 {
            return true;
        }
    }
    false
}

pub fn can_stay(world: &TickWorld, pos: IVec3) -> bool {
    world.is_normal_cube(pos - IVec3::Y) || nearby(world, pos)
}

fn catch(world: &mut TickWorld, pos: IVec3, chance: u32, age: u8) {
    let target = world.block(pos);
    let ignition = rates(target).1;
    if ignition == 0 || world.random().next_int(chance) >= ignition {
        return;
    }
    if target == Id::Tnt {
        world.set_block_notify(pos, Id::Air);
        world.emit(TickEffect::PrimedTnt {
            position: pos,
            fuse: 80,
        });
        return;
    }
    if world.random().next_int(u32::from(age) + 10) < 5 && !world.rained_on(pos) {
        let age = age
            .saturating_add((world.random().next_int(5) / 4) as u8)
            .min(15);
        world.set_block_and_metadata_notify(pos, Id::Fire, age);
    } else {
        world.set_block_notify(pos, Id::Air);
    }
}

impl BlockBehavior for Fire {
    fn ticks_randomly(&self, _: Id) -> bool {
        true
    }
    fn tick_rate(&self, _: Id) -> u32 {
        40
    }

    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Id::Air);
        } else {
            world.schedule(pos, Id::Fire, self.tick_rate(Id::Fire));
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, _: Id) {
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Id::Air);
        }
    }

    fn update_tick(&self, world: &mut TickWorld, pos: IVec3) {
        if !world.area_loaded(pos, 2) {
            return;
        }
        let netherrack = world.block(pos - IVec3::Y) == Id::Netherrack;
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Id::Air);
            return;
        }
        if !netherrack
            && world.is_raining()
            && [IVec3::ZERO, IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
                .iter()
                .any(|&offset| world.rained_on(pos + offset))
        {
            world.set_block_notify(pos, Id::Air);
            return;
        }
        let age = world.metadata(pos);
        if age < 15 {
            let increase = world.random().next_int(3) / 2;
            world.set_metadata(pos, age + increase as u8);
        }
        world.schedule(pos, Id::Fire, self.tick_rate(Id::Fire));
        if !netherrack && !nearby(world, pos) {
            if !world.is_normal_cube(pos - IVec3::Y) || age > 3 {
                world.set_block_notify(pos, Id::Air);
            }
            return;
        }
        if !netherrack
            && rates(world.block(pos - IVec3::Y)).0 == 0
            && age == 15
            && world.random().next_int(4) == 0
        {
            world.set_block_notify(pos, Id::Air);
            return;
        }
        for (offset, chance) in [
            (IVec3::X, 300),
            (IVec3::NEG_X, 300),
            (IVec3::Y, 250),
            (IVec3::NEG_Y, 250),
            (IVec3::Z, 300),
            (IVec3::NEG_Z, 300),
        ] {
            catch(world, pos + offset, chance, age);
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                for dy in -1..=4 {
                    let target = pos + IVec3::new(dx, dy, dz);
                    if target == pos || world.block(target) != Id::Air {
                        continue;
                    }
                    let strength = [
                        IVec3::X,
                        IVec3::NEG_X,
                        IVec3::Y,
                        IVec3::NEG_Y,
                        IVec3::Z,
                        IVec3::NEG_Z,
                    ]
                    .into_iter()
                    .map(|offset| rates(world.block(target + offset)).0)
                    .max()
                    .unwrap_or(0);
                    if strength == 0 {
                        continue;
                    }
                    let chance = 100 + (dy - 1).max(0) as u32 * 100;
                    let probability = (strength + 40) / (u32::from(age) + 30);
                    if world.random().next_int(chance) <= probability {
                        if !world.rained_on(target) {
                            world.set_block_and_metadata_notify(target, Id::Fire, age);
                        }
                    }
                }
            }
        }
    }
}

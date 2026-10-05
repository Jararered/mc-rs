//! Beta `BlockFire`: scheduled age, weather extinction and material-specific spread.
use crate::block::blocks::Block;
use crate::world::block_ticks::BlockBehavior;
use crate::world::block_ticks::TickEffect;
use crate::world::block_ticks::TickWorld;
use bevy::math::IVec3;

pub struct Fire;
pub static FIRE: Fire = Fire;

fn rates(block: Block) -> (u32, u32) {
    match block {
        Block::WoodenPlanks | Block::Fence | Block::WoodenStairs => (5, 20),
        Block::Wood => (5, 5),
        Block::Leaves => (30, 60),
        Block::Bookshelf => (30, 20),
        Block::Tnt => (15, 100),
        Block::TallGrass => (60, 100),
        Block::Wool => (30, 60),
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

/// `BlockFire.tryToCatchBlockOnFire`. The roll is made for every neighbor,
/// burnable or not, and TNT is replaced like any other block before it primes.
fn catch(world: &mut TickWorld, pos: IVec3, chance: u32, age: u8) {
    let target = world.block(pos);
    let ignition = rates(target).1;
    if world.random().next_int(chance) >= ignition {
        return;
    }
    if world.random().next_int(u32::from(age) + 10) < 5 && !world.rained_on(pos) {
        let age = age
            .saturating_add((world.random().next_int(5) / 4) as u8)
            .min(15);
        world.set_block_and_metadata_notify(pos, Block::Fire, age);
    } else {
        world.set_block_notify(pos, Block::Air);
    }
    if target == Block::Tnt {
        world.emit(TickEffect::PrimedTnt {
            position: pos,
            fuse: 80,
        });
    }
}

impl BlockBehavior for Fire {
    fn ticks_randomly(&self, _: Block) -> bool {
        true
    }
    fn tick_rate(&self, _: Block) -> u32 {
        40
    }

    fn on_added(&self, world: &mut TickWorld, pos: IVec3) {
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Block::Air);
        } else {
            world.schedule(pos, Block::Fire, self.tick_rate(Block::Fire));
        }
    }

    fn neighbor_changed(&self, world: &mut TickWorld, pos: IVec3, _: Block) {
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Block::Air);
        }
    }

    fn update_tick(&self, world: &mut TickWorld, pos: IVec3) {
        if !world.area_loaded(pos, 2) {
            return;
        }
        let netherrack = world.block(pos - IVec3::Y) == Block::Netherrack;
        if !can_stay(world, pos) {
            world.set_block_notify(pos, Block::Air);
            return;
        }
        if !netherrack
            && world.is_raining()
            && [IVec3::ZERO, IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z]
                .iter()
                .any(|&offset| world.rained_on(pos + offset))
        {
            world.set_block_notify(pos, Block::Air);
            return;
        }
        let age = world.metadata(pos);
        if age < 15 {
            let increase = world.random().next_int(3) / 2;
            world.set_metadata(pos, age + increase as u8);
        }
        world.schedule(pos, Block::Fire, self.tick_rate(Block::Fire));
        if !netherrack && !nearby(world, pos) {
            if !world.is_normal_cube(pos - IVec3::Y) || age > 3 {
                world.set_block_notify(pos, Block::Air);
            }
            return;
        }
        if !netherrack
            && rates(world.block(pos - IVec3::Y)).0 == 0
            && age == 15
            && world.random().next_int(4) == 0
        {
            world.set_block_notify(pos, Block::Air);
            return;
        }
        for (offset, chance) in [
            (IVec3::X, 300),
            (IVec3::NEG_X, 300),
            (IVec3::NEG_Y, 250),
            (IVec3::Y, 250),
            (IVec3::NEG_Z, 300),
            (IVec3::Z, 300),
        ] {
            catch(world, pos + offset, chance, age);
        }
        for dx in -1..=1 {
            for dz in -1..=1 {
                for dy in -1..=4 {
                    let target = pos + IVec3::new(dx, dy, dz);
                    if target == pos || world.block(target) != Block::Air {
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
                    if probability > 0
                        && world.random().next_int(chance) <= probability
                        && !world.rained_on(target)
                    {
                        let age = age
                            .saturating_add((world.random().next_int(5) / 4) as u8)
                            .min(15);
                        world.set_block_and_metadata_notify(target, Block::Fire, age);
                    }
                }
            }
        }
    }
}

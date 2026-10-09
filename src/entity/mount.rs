//! `Entity.mountEntity`, `updateRidden`, and `updateRiderPosition`.
//!
//! A rider carries [`Mounted`]; the vehicle keeps its own back-reference (a
//! cart's `rider`). Riders run their own update as usual, then are put back
//! where the vehicle holds them, as Beta's `updateRidden` does. Nothing here
//! is saved, as in Beta.

use bevy::prelude::*;

use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::minecart::CART_SIZE;
use crate::entity::minecart::MOUNTED_OFFSET;
use crate::entity::minecart::Minecart;
use crate::player::Player;
use crate::player::PlayerInterpolation;

/// The rider's side of a mount.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Mounted {
    pub vehicle: Entity,
    /// `getMountedYOffset() + rider.getYOffset()`: how far above the
    /// vehicle's centre the rider's position sits.
    pub lift: f32,
}

/// `Entity.getYOffset`: the player's is half a block lower than its eye
/// height. Other bodies' positions are their feet.
fn rider_offset(size: &EntitySize, player: bool) -> f32 {
    if player { size.y_offset - 0.5 } else { 0.0 }
}

/// Put `rider` on the empty cart `cart`, taking it off any vehicle it was on
/// and putting off any rider the cart had.
pub fn mount(commands: &mut Commands, rider: Entity, cart: Entity) {
    commands.queue(move |world: &mut World| {
        if !world.entities().contains(rider) || world.get::<Minecart>(cart).is_none() {
            return;
        }
        release(world, rider, true);
        if let Some(old) = world.get::<Minecart>(cart).and_then(|cart| cart.rider) {
            release(world, old, true);
        }
        let size = world.get::<EntitySize>(rider).copied().unwrap_or_default();
        let player = world.get::<Player>(rider).is_some();
        let lift = MOUNTED_OFFSET + rider_offset(&size, player);
        if let Some(mut vehicle) = world.get_mut::<Minecart>(cart) {
            vehicle.rider = Some(rider);
        }
        world.entity_mut(rider).insert(Mounted {
            vehicle: cart,
            lift,
        });
    });
}

/// `mountEntity(null)`: the rider steps off onto the top of its vehicle.
pub fn dismount(commands: &mut Commands, rider: Entity) {
    commands.queue(move |world: &mut World| release(world, rider, true));
}

/// Let go without moving the rider, for one that is being put somewhere else.
pub fn detach(commands: &mut Commands, rider: Entity) {
    commands.queue(move |world: &mut World| release(world, rider, false));
}

fn release(world: &mut World, rider: Entity, place: bool) {
    let Some(mounted) = world.get::<Mounted>(rider).copied() else {
        return;
    };
    let top = world
        .get::<Transform>(mounted.vehicle)
        .filter(|_| place)
        .map(|transform| CART_SIZE.aabb(transform.translation))
        .map(|aabb| {
            Vec3::new(
                (aabb.min.x + aabb.max.x) * 0.5,
                aabb.max.y,
                (aabb.min.z + aabb.max.z) * 0.5,
            )
        });
    if let Some(mut cart) = world.get_mut::<Minecart>(mounted.vehicle)
        && cart.rider == Some(rider)
    {
        cart.rider = None;
    }
    let size = world.get::<EntitySize>(rider).copied().unwrap_or_default();
    let mut entity = world.entity_mut(rider);
    entity.remove::<Mounted>();
    if let Some(top) = top
        && let Some(mut transform) = entity.get_mut::<Transform>()
    {
        transform.translation = top + Vec3::Y * size.y_offset;
    }
    if let Some(mut previous) = entity.get_mut::<PreviousTick>() {
        previous.0 = top.map_or(previous.0, |top| top + Vec3::Y * size.y_offset);
    }
    if let Some(mut interpolation) = entity.get_mut::<PlayerInterpolation>()
        && let Some(top) = top
    {
        interpolation.previous_position = top + Vec3::Y * size.y_offset;
    }
}

/// `updateRiderPosition`, once per frame after the vehicles and creatures
/// have run their ticks.
pub(crate) fn snap_riders(
    vehicles: Query<(&Transform, &PreviousTick), (With<Minecart>, Without<Mounted>)>,
    mut riders: Query<(
        &Mounted,
        &mut Transform,
        Option<&mut PreviousTick>,
        Option<&mut Velocity>,
        Option<&mut PlayerInterpolation>,
    )>,
) {
    for (mounted, mut transform, previous, velocity, interpolation) in &mut riders {
        let Ok((vehicle, vehicle_previous)) = vehicles.get(mounted.vehicle) else {
            continue;
        };
        let lift = Vec3::Y * mounted.lift;
        transform.translation = vehicle.translation + lift;
        if let Some(mut previous) = previous {
            previous.0 = vehicle_previous.0 + lift;
        }
        if let Some(mut velocity) = velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut interpolation) = interpolation {
            interpolation.previous_position = vehicle_previous.0 + lift;
        }
    }
}

/// Clear the links a vanished vehicle or rider left behind.
pub(crate) fn release_orphans(
    mut commands: Commands,
    mut carts: Query<(Entity, &mut Minecart)>,
    riders: Query<(Entity, &Mounted)>,
) {
    for (rider, mounted) in &riders {
        let attached = carts
            .get(mounted.vehicle)
            .is_ok_and(|(_, cart)| cart.rider == Some(rider));
        if !attached {
            commands.entity(rider).remove::<Mounted>();
        }
    }
    for (entity, mut cart) in &mut carts {
        if let Some(rider) = cart.rider
            && !riders
                .get(rider)
                .is_ok_and(|(_, mounted)| mounted.vehicle == entity)
        {
            cart.rider = None;
        }
    }
}

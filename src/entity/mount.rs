//! `Entity.mountEntity`, `updateRidden`, and `updateRiderPosition`.
//!
//! A rider carries [`Mounted`]; the vehicle (a minecart, a boat, a saddled
//! pig) carries a [`Seat`] with the back-reference. Riders run their own
//! update as usual, then are put back where the vehicle holds them, as Beta's
//! `updateRidden` does. Nothing here is saved, as in Beta.

use bevy::prelude::*;

use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::boat::Boat;
use crate::entity::minecart::MOUNTED_OFFSET;
use crate::entity::minecart::Minecart;
use crate::player::Player;
use crate::player::PlayerInterpolation;

/// The rider's side of a mount.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Mounted {
    pub vehicle: Entity,
}

/// The vehicle's side of a mount (`riddenByEntity`). Anything that can be
/// ridden carries one, occupied or not.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Seat {
    pub rider: Option<Entity>,
}

/// `Entity.getYOffset`: the player's is half a block lower than its eye
/// height. Other bodies' positions are their feet.
fn rider_offset(size: &EntitySize, player: bool) -> f32 {
    if player { size.y_offset - 0.5 } else { 0.0 }
}

/// `updateRiderPosition` less the rider's own offset: where a vehicle holds
/// its rider, from its position. A cart and a boat seat it below their
/// centre, the boat also along its yaw; any other vehicle (a pig) carries it
/// three quarters of the way up (`Entity.getMountedYOffset`).
pub fn seat_offset(cart: bool, boat: Option<&Boat>, size: &EntitySize) -> Vec3 {
    if let Some(boat) = boat {
        boat.seat()
    } else if cart {
        Vec3::Y * MOUNTED_OFFSET
    } else {
        Vec3::Y * size.height * 0.75
    }
}

/// Put `rider` on `vehicle`, taking it off any vehicle it was on and putting
/// off any rider the vehicle had.
pub fn mount(commands: &mut Commands, rider: Entity, vehicle: Entity) {
    commands.queue(move |world: &mut World| {
        if !world.entities().contains(rider) {
            return;
        }
        let Some(seat) = world.get::<Seat>(vehicle).copied() else {
            return;
        };
        release(world, rider, true);
        if let Some(old) = seat.rider {
            release(world, old, true);
        }
        set_rider(world, vehicle, Some(rider));
        world.entity_mut(rider).insert(Mounted { vehicle });
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

/// Write `riddenByEntity`. A cart keeps a copy its own update reads.
fn set_rider(world: &mut World, vehicle: Entity, rider: Option<Entity>) {
    if let Some(mut seat) = world.get_mut::<Seat>(vehicle) {
        seat.rider = rider;
    }
    if let Some(mut cart) = world.get_mut::<Minecart>(vehicle) {
        cart.rider = rider;
    }
}

fn release(world: &mut World, rider: Entity, place: bool) {
    let Some(mounted) = world.get::<Mounted>(rider).copied() else {
        return;
    };
    let top = world
        .get::<Transform>(mounted.vehicle)
        .filter(|_| place)
        .zip(world.get::<EntitySize>(mounted.vehicle))
        .map(|(transform, size)| size.aabb(transform.translation))
        .map(|aabb| {
            Vec3::new(
                (aabb.min.x + aabb.max.x) * 0.5,
                aabb.max.y,
                (aabb.min.z + aabb.max.z) * 0.5,
            )
        });
    if world
        .get::<Seat>(mounted.vehicle)
        .is_some_and(|seat| seat.rider == Some(rider))
    {
        set_rider(world, mounted.vehicle, None);
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
    vehicles: Query<
        (
            &Transform,
            &PreviousTick,
            Option<&EntitySize>,
            Option<&Boat>,
            Has<Minecart>,
        ),
        (With<Seat>, Without<Mounted>),
    >,
    mut riders: Query<(
        &Mounted,
        &mut Transform,
        Option<&EntitySize>,
        Has<Player>,
        Option<&mut PreviousTick>,
        Option<&mut Velocity>,
        Option<&mut PlayerInterpolation>,
    )>,
) {
    for (mounted, mut transform, size, player, previous, velocity, interpolation) in &mut riders {
        let Ok((vehicle, vehicle_previous, vehicle_size, boat, cart)) =
            vehicles.get(mounted.vehicle)
        else {
            continue;
        };
        let size = size.copied().unwrap_or_default();
        let vehicle_size = vehicle_size.copied().unwrap_or_default();
        let lift = seat_offset(cart, boat, &vehicle_size) + Vec3::Y * rider_offset(&size, player);
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
    mut seats: Query<(Entity, &mut Seat, Option<&mut Minecart>)>,
    riders: Query<(Entity, &Mounted)>,
) {
    for (rider, mounted) in &riders {
        let attached = seats
            .get(mounted.vehicle)
            .is_ok_and(|(_, seat, _)| seat.rider == Some(rider));
        if !attached {
            commands.entity(rider).remove::<Mounted>();
        }
    }
    for (entity, mut seat, cart) in &mut seats {
        if let Some(rider) = seat.rider
            && !riders
                .get(rider)
                .is_ok_and(|(_, mounted)| mounted.vehicle == entity)
        {
            seat.rider = None;
        }
        if let Some(mut cart) = cart
            && cart.rider != seat.rider
        {
            cart.rider = seat.rider;
        }
    }
}

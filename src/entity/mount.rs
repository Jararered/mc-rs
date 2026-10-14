//! `Entity.mountEntity`, `updateRidden`, and `updateRiderPosition`.
//!
//! A rider carries [`Mounted`]; the vehicle (a minecart, a boat, a saddled
//! pig) carries a [`Seat`] with the back-reference. Riders run their own
//! update as usual, then are put back where the vehicle holds them, as Beta's
//! `updateRidden` does, and their view drifts after the vehicle's turns
//! ([`RiderDrift`]). Nothing here is saved, as in Beta.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::boat::Boat;
use crate::entity::creature::Living;
use crate::entity::minecart::MOUNTED_OFFSET;
use crate::entity::minecart::Minecart;
use crate::player::LocalPlayer;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::world::tick::WorldTick;

/// The rider's side of a mount.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Mounted {
    pub vehicle: Entity,
}

/// `Entity.entityRiderYawDelta` and `entityRiderPitchDelta`: the vehicle's
/// turns the rider's view has still to follow, in Beta's degrees. Mounting
/// starts it afresh, as `mountEntity` zeroes both.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct RiderDrift {
    pub yaw: f32,
    pub pitch: f32,
    /// The vehicle's yaw and pitch when last looked at.
    pub seen: Option<Vec2>,
}

impl RiderDrift {
    /// Most of a turn the view follows in one tick.
    const MAX_STEP: f32 = 10.0;

    /// Add how far the vehicle has turned since it was last seen
    /// (`rotationYaw - prevRotationYaw`). The first look only remembers.
    pub fn follow(&mut self, vehicle: Vec2) {
        if let Some(seen) = self.seen {
            self.yaw += vehicle.x - seen.x;
            self.pitch += vehicle.y - seen.y;
        }
        self.seen = Some(vehicle);
    }

    /// One tick of `updateRidden`: wrap what is owed into `[-180, 180)`, and
    /// return the half of it the rider turns by, at most ten degrees on each
    /// axis, as `(yaw, pitch)`.
    pub fn step(&mut self) -> Vec2 {
        self.yaw = wrap_degrees(self.yaw);
        self.pitch = wrap_degrees(self.pitch);
        let turn = Vec2::new(
            (self.yaw * 0.5).clamp(-Self::MAX_STEP, Self::MAX_STEP),
            (self.pitch * 0.5).clamp(-Self::MAX_STEP, Self::MAX_STEP),
        );
        self.yaw -= turn.x;
        self.pitch -= turn.y;
        turn
    }
}

/// `updateRidden`'s `while` loops: an angle into `[-180, 180)`.
fn wrap_degrees(mut angle: f32) -> f32 {
    while angle >= 180.0 {
        angle -= 360.0;
    }
    while angle < -180.0 {
        angle += 360.0;
    }
    angle
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
        world
            .entity_mut(rider)
            .insert((Mounted { vehicle }, RiderDrift::default()));
    });
}

/// `mountEntity(null)`: the rider steps off onto the top of its vehicle.
pub fn dismount(commands: &mut Commands, rider: Entity) {
    commands.queue(move |world: &mut World| release(world, rider, true));
}

/// [`dismount`] at once, for code that holds the `World`.
pub fn release_now(world: &mut World, rider: Entity) {
    release(world, rider, true);
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
    entity.remove::<(Mounted, RiderDrift)>();
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
        let own = Vec3::Y * rider_offset(&size, player);
        let lift = seat_offset(cart, boat, &vehicle_size) + own;
        // A turning boat swings its seat; the rider is drawn from where the
        // seat was a tick ago, as the boat is.
        let previous_lift = boat.map_or(lift, |boat| boat.previous_seat() + own);
        transform.translation = vehicle.translation + lift;
        if let Some(mut previous) = previous {
            previous.0 = vehicle_previous.0 + previous_lift;
        }
        if let Some(mut velocity) = velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut interpolation) = interpolation {
            interpolation.previous_position = vehicle_previous.0 + previous_lift;
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
            commands.entity(rider).remove::<(Mounted, RiderDrift)>();
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

/// `Entity.updateRidden`'s turn: a rider's view follows its vehicle's yaw
/// and pitch, half of what is owed a tick and at most ten degrees. This
/// client's player and mobs turn; a remote player's look is its client's.
/// Boats and carts never pitch; a pig does as it looks about. A pig the
/// rider steers (`GameSettings::pig_steering`) turns after the rider's view,
/// so the view does not follow it back.
pub(crate) fn drift_riders(
    tick: Res<WorldTick>,
    settings: Option<Res<GameSettings>>,
    mut sets: ParamSet<(
        Query<(Entity, &Mounted), With<RiderDrift>>,
        Query<(Option<&Boat>, Option<&Minecart>, Option<&Living>)>,
        Query<
            (
                &mut RiderDrift,
                Option<&mut Transform>,
                Option<&mut Living>,
                Has<LocalPlayer>,
            ),
            With<Mounted>,
        >,
    )>,
    mut pairs: Local<Vec<(Entity, Entity)>>,
    mut turns: Local<Vec<(Entity, Option<Vec2>)>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    // Look the vehicles up once the riders' borrow has ended.
    pairs.clear();
    pairs.extend(
        sets.p0()
            .iter()
            .map(|(rider, mounted)| (rider, mounted.vehicle)),
    );
    let steering = settings
        .as_ref()
        .is_none_or(|settings| settings.pig_steering);
    turns.clear();
    {
        let vehicles = sets.p1();
        for &(rider, vehicle) in &*pairs {
            let Ok((boat, cart, living)) = vehicles.get(vehicle) else {
                continue;
            };
            let angles = if let Some(boat) = boat {
                Vec2::new(boat.yaw, 0.0)
            } else if let Some(cart) = cart {
                Vec2::new(cart.yaw, 0.0)
            } else if let Some(living) = living {
                if steering {
                    turns.push((rider, None));
                    continue;
                }
                Vec2::new(living.yaw, living.pitch)
            } else {
                continue;
            };
            turns.push((rider, Some(angles)));
        }
    }
    let mut riders = sets.p2();
    for &(rider, angles) in &*turns {
        let Ok((mut drift, transform, living, local)) = riders.get_mut(rider) else {
            continue;
        };
        let Some(angles) = angles else {
            *drift = RiderDrift::default();
            continue;
        };
        drift.follow(angles);
        let mut turn = Vec2::ZERO;
        for _ in 0..ticks {
            turn += drift.step();
        }
        if turn == Vec2::ZERO {
            continue;
        }
        if let Some(mut living) = living {
            living.yaw += turn.x;
            living.pitch += turn.y;
        } else if local && let Some(mut transform) = transform {
            // Bevy's yaw is `PI - ` Beta's and its pitch looks up where
            // Beta's looks down, so both turn the other way.
            let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
            let yaw = yaw - turn.x.to_radians();
            let pitch = (pitch - turn.y.to_radians()).clamp(
                -std::f32::consts::FRAC_PI_2 + 0.01,
                std::f32::consts::FRAC_PI_2 - 0.01,
            );
            transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        }
    }
}

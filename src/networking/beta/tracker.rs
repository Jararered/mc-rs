//! `EntityTracker` and `EntityTrackerEntry`: which entities each client is
//! shown, and what it hears as they move and change.
//!
//! Nothing here is stored in the world. Each tick [`survey`] reads every
//! entity a client could be told about out of a dimension, and
//! [`Tracker::update`] compares that with what was last sent: an entity that
//! came into range is spawned for the client, one that left is destroyed, and
//! the rest get the relative moves, velocities, metadata and status Beta's
//! tracker sends, at the rate it sends them.

use std::collections::HashMap;
use std::collections::HashSet;

use bevy::prelude::*;

use super::codec::WireStack;
use super::codec::Writer;
use crate::block::blocks::Block;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::boat::Boat;
use crate::entity::combat::PlayerCombat;
use crate::entity::creature::Fuse;
use crate::entity::creature::Hover;
use crate::entity::creature::Living;
use crate::entity::drops::items::ItemMotion;
use crate::entity::drops::items::PickupAnimation;
use crate::entity::explosion::PrimedTnt;
use crate::entity::falling_block::FallingBlock;
use crate::entity::fishing::Bobber;
use crate::entity::minecart::CartKind;
use crate::entity::minecart::Minecart;
use crate::entity::mobs::Mob;
use crate::entity::mobs::MobType;
use crate::entity::mount::Mounted;
use crate::entity::projectiles::Arrow;
use crate::entity::projectiles::Fireball;
use crate::entity::thrown::Thrown;
use crate::entity::thrown::ThrownKind;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::player::PlayerMovementInput;
use crate::player::PlayerName;
use crate::player::PlayerSurvival;
use crate::player::sleep::PlayerSleep;
use crate::world::chunk::ChunkPosition;
use crate::world::dimension::Dimension;
use crate::world::tick::TICK_SECONDS;

/// What an entity is to a client: the packet that spawns it.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Kind {
    /// `Packet20NamedEntitySpawn`.
    Player(String),
    /// `Packet24MobSpawn` with `EntityList`'s id.
    Mob(i8),
    /// `Packet21PickupSpawn`.
    Item(WireStack),
    /// `Packet23VehicleSpawn` with its type. `owner` and `launch` are whoever
    /// let it go and the velocity it left with, for the kinds that say.
    Object {
        kind: i8,
        owner: Option<Entity>,
        launch: Option<Vec3>,
    },
}

/// One entity as it stands this tick.
#[derive(Clone, Debug)]
pub(super) struct Seen {
    pub entity: Entity,
    pub kind: Kind,
    /// `posX/Y/Z` as Beta has them: a player's are their feet.
    pub at: [f64; 3],
    /// Beta's degrees.
    pub yaw: f32,
    pub pitch: f32,
    /// Blocks per tick.
    pub motion: Vec3,
    /// The `DataWatcher`, without its end marker. Empty for an entity that
    /// has none.
    pub metadata: Vec<u8>,
    pub riding: Option<Entity>,
    /// `hurtTime`: a rise is a new hit.
    pub hurt_time: i16,
    pub dead: bool,
    /// A player has picked it up, and it is only playing out its flight.
    pub collected: bool,
    /// What a player holds, then wears from the boots up.
    pub equipment: Option<[Option<(i16, i16)>; 5]>,
    /// The bed a player is asleep in.
    pub bed: Option<IVec3>,
    /// `Packet38EntityStatus` beyond hurt and death, made this tick.
    pub status: Option<u8>,
}

/// `EntityTracker.trackEntity`'s table: how far away a kind is shown, how
/// many ticks pass between its updates, and whether its velocity is sent.
fn tracking(kind: &Kind) -> (f64, u32, bool) {
    match kind {
        Kind::Player(_) => (512.0, 2, false),
        Kind::Item(_) => (64.0, 20, true),
        // A squid.
        Kind::Mob(94) => (160.0, 3, true),
        Kind::Mob(_) => (160.0, 3, false),
        Kind::Object { kind, .. } => match kind {
            90 => (64.0, 5, true),
            60 => (64.0, 20, false),
            63 => (64.0, 10, false),
            61 | 62 => (64.0, 10, true),
            1 | 10..=12 => (160.0, 5, true),
            50 => (160.0, 10, true),
            _ => (160.0, 20, true),
        },
    }
}

fn mob_id(kind: MobType) -> i8 {
    match kind {
        MobType::Creeper => 50,
        MobType::Skeleton => 51,
        MobType::Spider => 52,
        MobType::Zombie => 54,
        MobType::Slime => 55,
        MobType::Ghast => 56,
        MobType::PigZombie => 57,
        MobType::Pig => 90,
        MobType::Sheep => 91,
        MobType::Cow => 92,
        MobType::Chicken => 93,
        MobType::Squid => 94,
        MobType::Wolf => 95,
    }
}

/// `DataWatcher.writeWatchableObject` for a byte.
fn watch_byte(out: &mut Vec<u8>, index: u8, value: u8) {
    out.extend([index & 31, value]);
}

fn mob_metadata(mob: &Mob, fuse: Option<&Fuse>, hover: Option<&Hover>) -> Vec<u8> {
    let mut out = Vec::new();
    // `Entity`'s flags: bit 0 is on fire.
    watch_byte(&mut out, 0, u8::from(mob.fire_ticks > 0));
    match mob.kind {
        MobType::Pig => watch_byte(&mut out, 16, u8::from(mob.saddled)),
        MobType::Sheep => watch_byte(
            &mut out,
            16,
            (mob.variant & 15) | (u8::from(mob.sheared) << 4),
        ),
        MobType::Creeper => {
            watch_byte(&mut out, 16, fuse.map_or(-1, Fuse::state) as u8);
            watch_byte(&mut out, 17, u8::from(mob.charged));
        }
        MobType::Slime => watch_byte(&mut out, 16, mob.variant.max(1)),
        MobType::Ghast => watch_byte(
            &mut out,
            16,
            u8::from(hover.is_some_and(|hover| hover.attack_counter > 10)),
        ),
        MobType::Wolf => {
            watch_byte(
                &mut out,
                16,
                u8::from(mob.sitting) | (u8::from(mob.angry) << 1) | (u8::from(mob.tamed) << 2),
            );
            // A string, then an int.
            out.push((4 << 5) | 17);
            let owner: Vec<u16> = mob.owner.as_deref().unwrap_or("").encode_utf16().collect();
            out.extend((owner.len() as i16).to_be_bytes());
            for unit in owner {
                out.extend(unit.to_be_bytes());
            }
            out.push((2 << 5) | 18);
            out.extend(i32::from(mob.health).to_be_bytes());
        }
        _ => {}
    }
    out
}

fn position(at: Vec3) -> [f64; 3] {
    [f64::from(at.x), f64::from(at.y), f64::from(at.z)]
}

/// `Packet5PlayerInventory`'s id and damage for a slot.
fn worn(stack: Option<crate::item::ItemStack>) -> Option<(i16, i16)> {
    stack.map(|stack| (stack.item().as_u16() as i16, stack.data() as i16))
}

/// Beta's yaw and pitch for a Bevy rotation. Beta's yaw 0 looks along +Z and
/// its pitch grows downward.
pub(super) fn beta_angles(rotation: Quat) -> (f32, f32) {
    let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
    (
        (std::f32::consts::PI - yaw).to_degrees(),
        -pitch.to_degrees(),
    )
}

/// Everything in `world` a client can be told about.
#[allow(clippy::too_many_lines)]
pub(super) fn survey(world: &mut World) -> Vec<Seen> {
    let mut seen = Vec::new();
    let plain = |entity, kind, at: Vec3, motion| Seen {
        entity,
        kind,
        at: position(at),
        yaw: 0.0,
        pitch: 0.0,
        motion,
        metadata: Vec::new(),
        riding: None,
        hurt_time: 0,
        dead: false,
        collected: false,
        equipment: None,
        bed: None,
        status: None,
    };

    let mut players = world.query_filtered::<(
        Entity,
        &PlayerName,
        &Transform,
        &PlayerHealth,
        &PlayerCombat,
        &PlayerSurvival,
        &PlayerMovementInput,
        Option<&Mounted>,
        &Hotbar,
        &Inventory,
        &PlayerSleep,
        &Velocity,
    ), With<Player>>();
    for (
        entity,
        name,
        transform,
        health,
        combat,
        survival,
        input,
        mounted,
        hotbar,
        inventory,
        sleep,
        velocity,
    ) in players.iter(world)
    {
        let (yaw, pitch) = beta_angles(transform.rotation);
        let mut metadata = Vec::new();
        watch_byte(
            &mut metadata,
            0,
            u8::from(survival.is_burning())
                | (u8::from(input.sneaking) << 1)
                | (u8::from(mounted.is_some()) << 2),
        );
        let armor = inventory.armor;
        seen.push(Seen {
            yaw,
            pitch,
            metadata,
            riding: mounted.map(|mounted| mounted.vehicle),
            hurt_time: combat.hurt_time,
            dead: health.current == 0,
            equipment: Some([
                worn(hotbar.selected_stack()),
                worn(armor[3]),
                worn(armor[2]),
                worn(armor[1]),
                worn(armor[0]),
            ]),
            bed: sleep.bed.filter(|_| sleep.sleeping),
            ..plain(
                entity,
                Kind::Player(name.0.clone()),
                transform.translation - Vec3::Y * EntitySize::PLAYER.y_offset,
                velocity.0 * TICK_SECONDS,
            )
        });
    }

    let mut mobs = world.query::<(
        Entity,
        &Mob,
        &mut Living,
        &Transform,
        &Velocity,
        Option<&Fuse>,
        Option<&Hover>,
    )>();
    for (entity, mob, mut living, transform, velocity, fuse, hover) in mobs.iter_mut(world) {
        // Read without flagging a change unless there is something to take.
        let status = if living.status.is_some() {
            living.status.take()
        } else {
            None
        };
        seen.push(Seen {
            status,
            yaw: living.yaw,
            pitch: living.pitch,
            metadata: mob_metadata(mob, fuse, hover),
            hurt_time: living.hurt_time,
            dead: mob.health <= 0,
            ..plain(
                entity,
                Kind::Mob(mob_id(mob.kind)),
                transform.translation,
                velocity.0 * TICK_SECONDS,
            )
        });
    }

    let mut items = world.query::<(
        Entity,
        &DroppedItem,
        &Transform,
        Option<&ItemMotion>,
        Has<PickupAnimation>,
    )>();
    for (entity, item, transform, motion, collected) in items.iter(world) {
        let stack = item.0;
        seen.push(Seen {
            collected,
            ..plain(
                entity,
                Kind::Item((
                    stack.item().as_u16() as i16,
                    stack.count() as i8,
                    stack.data() as i16,
                )),
                transform.translation,
                motion.map_or(Vec3::ZERO, |motion| motion.0),
            )
        });
    }

    let object = |kind| Kind::Object {
        kind,
        owner: None,
        launch: None,
    };
    let mut carts = world.query::<(Entity, &Minecart, &Transform)>();
    for (entity, cart, transform) in carts.iter(world) {
        let kind = match cart.kind {
            CartKind::Empty => 10,
            CartKind::Chest => 11,
            CartKind::Furnace => 12,
        };
        seen.push(Seen {
            yaw: cart.yaw,
            ..plain(entity, object(kind), transform.translation, cart.motion)
        });
    }
    let mut boats = world.query::<(Entity, &Boat, &Transform)>();
    for (entity, boat, transform) in boats.iter(world) {
        seen.push(Seen {
            yaw: boat.yaw,
            ..plain(entity, object(1), transform.translation, boat.motion)
        });
    }
    let mut tnt = world.query_filtered::<(Entity, &Transform, &Velocity), With<PrimedTnt>>();
    for (entity, transform, velocity) in tnt.iter(world) {
        seen.push(plain(
            entity,
            object(50),
            transform.translation,
            velocity.0 * TICK_SECONDS,
        ));
    }
    let mut falling = world.query::<(Entity, &FallingBlock, &Transform)>();
    for (entity, block, transform) in falling.iter(world) {
        // Beta's server knows how to spawn only these two.
        let kind = match block.block {
            Block::Sand => 70,
            Block::Gravel => 71,
            _ => continue,
        };
        seen.push(plain(
            entity,
            object(kind),
            transform.translation,
            block.motion,
        ));
    }
    let mut arrows = world.query::<(Entity, &Arrow, &Transform)>();
    for (entity, arrow, transform) in arrows.iter(world) {
        seen.push(Seen {
            yaw: arrow.yaw,
            pitch: arrow.pitch,
            collected: arrow.taken.is_some(),
            ..plain(
                entity,
                Kind::Object {
                    kind: 60,
                    owner: arrow.shooter.map(|shooter| shooter.entity),
                    launch: Some(arrow.motion),
                },
                transform.translation,
                arrow.motion,
            )
        });
    }
    let mut fireballs = world.query::<(Entity, &Fireball, &Transform)>();
    for (entity, fireball, transform) in fireballs.iter(world) {
        seen.push(plain(
            entity,
            Kind::Object {
                kind: 63,
                owner: fireball.owner,
                // The client is given the acceleration it flies by.
                launch: Some(fireball.acceleration),
            },
            transform.translation,
            fireball.motion,
        ));
    }
    let mut thrown = world.query::<(Entity, &Thrown, &Transform)>();
    for (entity, thrown, transform) in thrown.iter(world) {
        let kind = match thrown.kind {
            ThrownKind::Snowball => 61,
            ThrownKind::Egg => 62,
        };
        seen.push(plain(
            entity,
            object(kind),
            transform.translation,
            thrown.motion,
        ));
    }
    let mut bobbers = world.query::<(Entity, &Bobber, &Transform)>();
    for (entity, bobber, transform) in bobbers.iter(world) {
        seen.push(plain(
            entity,
            object(90),
            transform.translation,
            bobber.motion,
        ));
    }
    seen
}

/// A client looking at a dimension.
pub(super) struct Viewer<'a> {
    /// The id the client knows its own player by.
    pub id: i32,
    pub entity: Entity,
    /// Where its player is.
    pub at: Vec3,
    /// The chunks it holds. An entity outside them is not shown: the client
    /// would let it fall through the missing ground.
    pub chunks: &'a HashSet<ChunkPosition>,
    /// The entities it has been shown.
    pub shown: &'a mut HashSet<i32>,
    /// The riders it has been told about, and what each rides.
    pub attached: &'a mut HashMap<i32, i32>,
    pub out: &'a mut Writer,
}

/// `EntityTrackerEntry`.
struct Entry {
    id: i32,
    range: f64,
    period: u32,
    sends_motion: bool,
    /// `encodedPosX/Y/Z`: the position clients hold, in 32nds of a block.
    at: [i32; 3],
    /// `encodedRotationYaw/Pitch`, in 256ths of a turn.
    look: [i32; 2],
    motion: Vec3,
    counter: u32,
    /// Ticks since a teleport last put every client exactly right.
    since_teleport: u32,
    metadata: Vec<u8>,
    hurt_time: i16,
    dead: bool,
    equipment: Option<[Option<(i16, i16)>; 5]>,
    bed: Option<IVec3>,
}

fn encoded(seen: &Seen) -> ([i32; 3], [i32; 2]) {
    (
        seen.at.map(|value| (value * 32.0).floor() as i32),
        [seen.yaw, seen.pitch].map(|degrees| (degrees * 256.0 / 360.0).floor() as i32),
    )
}

impl Entry {
    fn new(id: i32, seen: &Seen) -> Self {
        let (range, period, sends_motion) = tracking(&seen.kind);
        let (at, look) = encoded(seen);
        Self {
            id,
            range,
            period,
            sends_motion,
            at,
            look,
            motion: seen.motion,
            counter: 0,
            since_teleport: 0,
            metadata: seen.metadata.clone(),
            hurt_time: seen.hurt_time,
            dead: false,
            equipment: seen.equipment,
            bed: None,
        }
    }

    /// `updatePlayerList`'s packets: `shared` for the clients that are shown
    /// the entity, and `own` for a player's own client.
    fn step(&mut self, seen: &Seen, shared: &mut Writer, own: &mut Writer) {
        let id = self.id;
        self.counter += 1;
        self.since_teleport += 1;
        if self.counter % self.period == 0 {
            let (at, look) = encoded(seen);
            let delta = [at[0] - self.at[0], at[1] - self.at[1], at[2] - self.at[2]];
            let turned = (look[0] - self.look[0]).abs() >= 8 || (look[1] - self.look[1]).abs() >= 8;
            let bytes = [look[0] as u8, look[1] as u8];
            if delta.iter().all(|value| (-128..128).contains(value)) && self.since_teleport <= 400 {
                let moved = delta != [0; 3];
                shared.entity_step(
                    id,
                    moved.then(|| delta.map(|value| value as i8)),
                    turned.then_some(bytes),
                );
                if turned {
                    self.look = look;
                }
            } else {
                self.since_teleport = 0;
                shared.entity_teleport_fixed(id, at, bytes);
                self.look = look;
            }
            self.at = at;

            if self.sends_motion {
                let change = (seen.motion - self.motion).length_squared();
                if change > 0.02 * 0.02 || (change > 0.0 && seen.motion == Vec3::ZERO) {
                    self.motion = seen.motion;
                    shared.velocity(id, seen.motion);
                }
            }
            if seen.metadata != self.metadata {
                self.metadata.clone_from(&seen.metadata);
                shared.entity_metadata(id, &seen.metadata);
                own.entity_metadata(id, &seen.metadata);
            }
        }

        // `beenAttacked`: the hurt animation and the knockback.
        if seen.hurt_time > self.hurt_time && !seen.dead {
            for out in [&mut *shared, &mut *own] {
                out.entity_status(id, 2);
                out.velocity(id, seen.motion);
            }
        }
        self.hurt_time = seen.hurt_time;
        if let Some(status) = seen.status {
            shared.entity_status(id, status);
        }
        if seen.dead && !self.dead {
            shared.entity_status(id, 3);
            own.entity_status(id, 3);
        }
        self.dead = seen.dead;

        if let (Some(now), Some(before)) = (seen.equipment, self.equipment) {
            for slot in 0..5 {
                if now[slot] != before[slot] {
                    shared.equipment(id, slot as i16, now[slot]);
                }
            }
        }
        self.equipment = seen.equipment;
        if seen.bed != self.bed {
            for out in [&mut *shared, &mut *own] {
                match seen.bed {
                    Some(bed) => out.sleep(id, bed.x, bed.y, bed.z),
                    None => out.animation(id, 3),
                }
            }
            self.bed = seen.bed;
        }
    }

    /// `getSpawnPacket` and what `updatePlayerEntity` sends after it. The
    /// entity appears where the clients already shown it hold it, so the
    /// relative moves that follow mean the same to all of them.
    fn spawn(&self, seen: &Seen, owner: Option<i32>, out: &mut Writer) {
        let id = self.id;
        let at = self.at.map(|value| f64::from(value) / 32.0);
        let [yaw, pitch] = self.look.map(|value| value as f32 * 360.0 / 256.0);
        match &seen.kind {
            Kind::Player(name) => {
                let held = seen
                    .equipment
                    .and_then(|equipment| equipment[0])
                    .map_or(0, |(item, _)| item);
                out.named_entity_spawn(id, name, at, yaw, pitch, held);
                for (slot, item) in seen.equipment.into_iter().flatten().enumerate() {
                    out.equipment(id, slot as i16, item);
                }
                // Beta leaves a sneaking or burning player looking as if
                // they were not until that next changes.
                out.entity_metadata(id, &self.metadata);
                if let Some(bed) = self.bed {
                    out.sleep(id, bed.x, bed.y, bed.z);
                }
            }
            Kind::Mob(kind) => out.mob_spawn(id, *kind, at, yaw, pitch, &self.metadata),
            Kind::Item(stack) => out.pickup_spawn(id, *stack, at, seen.motion),
            Kind::Object { kind, launch, .. } => {
                out.vehicle_spawn(
                    id,
                    *kind,
                    at,
                    launch.map(|launch| (owner.unwrap_or(id), launch)),
                );
            }
        }
        if self.sends_motion {
            out.velocity(id, seen.motion);
        }
    }
}

/// Every client's view of every entity.
#[derive(Default)]
pub(super) struct Tracker {
    last_id: i32,
    entries: HashMap<(Dimension, Entity), Entry>,
}

impl Tracker {
    /// A new entity id, never zero: a thrown thing's owner is only read when
    /// its id is above that.
    pub fn allocate(&mut self) -> i32 {
        self.last_id = self.last_id.checked_add(1).unwrap_or(1);
        self.last_id
    }

    /// The id clients know `entity` by, and what kind of thing it is.
    pub fn lookup(&self, dimension: Dimension, id: i32) -> Option<Entity> {
        self.entries
            .iter()
            .find(|((place, _), entry)| *place == dimension && entry.id == id)
            .map(|((_, entity), _)| *entity)
    }

    pub fn id_of(&self, dimension: Dimension, entity: Entity) -> Option<i32> {
        self.entries.get(&(dimension, entity)).map(|entry| entry.id)
    }

    /// A dimension unloaded, and everything in it with it.
    pub fn forget_all_but(&mut self, loaded: &[Dimension]) {
        self.entries
            .retain(|(dimension, _), _| loaded.contains(dimension));
    }

    /// One tick of `dimension`. `players` gives each player's id by name.
    pub fn update(
        &mut self,
        dimension: Dimension,
        seen: &[Seen],
        players: &HashMap<String, i32>,
        viewers: &mut [Viewer],
    ) {
        let mut live = HashSet::new();
        for seen in seen {
            let key = (dimension, seen.entity);
            if !self.entries.contains_key(&key) {
                // Something picked up before anyone was told of it.
                if seen.collected {
                    continue;
                }
                let id = match &seen.kind {
                    Kind::Player(name) => match players.get(name) {
                        Some(id) => *id,
                        None => continue,
                    },
                    _ => self.allocate(),
                };
                self.entries.insert(key, Entry::new(id, seen));
            }
            if !seen.collected {
                live.insert(seen.entity);
            }
        }

        for seen in seen {
            let key = (dimension, seen.entity);
            let owner = match &seen.kind {
                Kind::Object {
                    owner: Some(owner), ..
                } => self.id_of(dimension, *owner),
                _ => None,
            };
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            let id = entry.id;
            // `recreatePlayerEntity`: a player back from the dead is a new
            // entity to everyone else, whose clients took the body away.
            if entry.dead && !seen.dead {
                for viewer in viewers.iter_mut() {
                    if viewer.shown.remove(&id) {
                        viewer.out.destroy_entity(id);
                        viewer.attached.remove(&id);
                    }
                }
            }
            let mut shared = Writer::default();
            let mut own = Writer::default();
            if seen.collected {
                // `EntityPlayerMP.onItemPickup`: it flies to whoever is
                // nearest, who is whoever took it.
                let from = Vec3::new(seen.at[0] as f32, seen.at[1] as f32, seen.at[2] as f32);
                let collector = viewers.iter().min_by(|a, b| {
                    a.at.distance_squared(from)
                        .total_cmp(&b.at.distance_squared(from))
                });
                if let Some(collector) = collector {
                    shared.collect(id, collector.id);
                }
            } else {
                entry.step(seen, &mut shared, &mut own);
            }
            let chunk = ChunkPosition::from_block(entry.at[0] >> 5, entry.at[2] >> 5);
            for viewer in viewers.iter_mut() {
                if viewer.entity == seen.entity {
                    viewer.out.extend(&own);
                    continue;
                }
                let near = (f64::from(viewer.at.x) - f64::from(entry.at[0] / 32)).abs()
                    <= entry.range
                    && (f64::from(viewer.at.z) - f64::from(entry.at[2] / 32)).abs() <= entry.range
                    && viewer.chunks.contains(&chunk);
                if near && !seen.collected {
                    if viewer.shown.insert(id) {
                        entry.spawn(seen, owner, viewer.out);
                    } else {
                        viewer.out.extend(&shared);
                    }
                } else if viewer.shown.remove(&id) {
                    if seen.collected {
                        viewer.out.extend(&shared);
                    }
                    viewer.out.destroy_entity(id);
                    viewer.attached.remove(&id);
                }
            }
        }

        self.entries.retain(|(place, entity), entry| {
            if *place != dimension || live.contains(entity) {
                return true;
            }
            for viewer in viewers.iter_mut() {
                if viewer.shown.remove(&entry.id) {
                    viewer.out.destroy_entity(entry.id);
                    viewer.attached.remove(&entry.id);
                }
            }
            false
        });

        // `Packet39AttachEntity`, once both ends are on the client.
        for seen in seen {
            let Some(rider) = self.id_of(dimension, seen.entity) else {
                continue;
            };
            let vehicle = seen
                .riding
                .and_then(|vehicle| self.id_of(dimension, vehicle));
            for viewer in viewers.iter_mut() {
                if viewer.entity != seen.entity && !viewer.shown.contains(&rider) {
                    continue;
                }
                let wanted = vehicle
                    .filter(|vehicle| viewer.shown.contains(vehicle))
                    .unwrap_or(-1);
                let told = viewer.attached.get(&rider).copied().unwrap_or(-1);
                if wanted != told {
                    viewer.out.attach(rider, wanted);
                    if wanted == -1 {
                        viewer.attached.remove(&rider);
                    } else {
                        viewer.attached.insert(rider, wanted);
                    }
                }
            }
        }
    }
}

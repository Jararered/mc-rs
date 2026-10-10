//! `EntityMinecart`: rail motion, collisions, damage, and the chest and
//! furnace variants.
//!
//! [`step_minecart`] is `EntityMinecart.onUpdate` for one world tick, in
//! Beta's order. `Transform.translation` is Beta's `posX/posY/posZ`: the
//! centre of the box, which rests 0.5 above the bottom of a flat rail block.
use bevy::prelude::*;
use serde::Deserialize;
use serde::Serialize;

use crate::block::blocks::Block;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Velocity;
use crate::entity::creature::Living;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::drops::items::spawn_entity_drop;
use crate::entity::mobs::Mob;
use crate::entity::mount::Mounted;
use crate::entity::mount::Seat;
use crate::entity::mount::dismount;
use crate::entity::mount::mount;
use crate::item::Item;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::move_entity;
use crate::player::Player;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;

pub const CART_SIZE: EntitySize = EntitySize {
    width: 0.98,
    height: 0.7,
    y_offset: 0.35,
};

/// `EntityMinecart.getMountedYOffset`: a rider sits this far from the centre.
pub const MOUNTED_OFFSET: f32 = -0.3;
/// `getSizeInventory` of a chest cart.
pub const CARGO_SLOTS: usize = 27;
/// Fuel added by one coal.
pub const FUEL_PER_COAL: i32 = 1200;
/// `minecartCurrentDamage` above which a hit breaks the cart.
const BREAK_DAMAGE: i32 = 40;

/// `EntityMinecart.minecartType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CartKind {
    #[default]
    Empty,
    Chest,
    Furnace,
}

impl CartKind {
    /// Beta's `Type` tag.
    pub const fn type_id(self) -> i32 {
        match self {
            Self::Empty => 0,
            Self::Chest => 1,
            Self::Furnace => 2,
        }
    }

    pub const fn from_type_id(id: i32) -> Self {
        match id {
            1 => Self::Chest,
            2 => Self::Furnace,
            _ => Self::Empty,
        }
    }

    /// `ItemMinecart.minecartType`: the cart a held item places.
    pub fn from_item(item: Item) -> Option<Self> {
        match item {
            Item::Minecart => Some(Self::Empty),
            Item::ChestMinecart => Some(Self::Chest),
            Item::FurnaceMinecart => Some(Self::Furnace),
            _ => None,
        }
    }

    /// The block a broken cart leaves behind.
    pub const fn block(self) -> Option<Block> {
        match self {
            Self::Empty => None,
            Self::Chest => Some(Block::Chest),
            Self::Furnace => Some(Block::Furnace),
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct Minecart {
    pub kind: CartKind,
    /// Blocks per world tick (`motionX/Y/Z`).
    pub motion: Vec3,
    /// `fuel`: only a furnace cart burns it, while it is pushing.
    pub fuel: i32,
    /// `pushX/pushZ`: the direction a furnace cart is shoved along.
    pub push: Vec2,
    /// `minecartCurrentDamage`.
    pub damage: i32,
    /// `minecartTimeSinceHit`.
    pub time_since_hit: i32,
    /// `minecartRockDirection`.
    pub rock_direction: i32,
    /// `isInReverse`.
    pub in_reverse: bool,
    /// `rotationYaw` in degrees.
    pub yaw: f32,
    /// `prevRotationYaw`, for rider yaw drift and drawing.
    pub prev_yaw: f32,
    /// `onGround`, from the last sweep.
    pub on_ground: bool,
    /// `riddenByEntity`.
    pub rider: Option<Entity>,
}

impl Default for Minecart {
    fn default() -> Self {
        Self {
            kind: CartKind::Empty,
            motion: Vec3::ZERO,
            fuel: 0,
            push: Vec2::ZERO,
            damage: 0,
            time_since_hit: 0,
            rock_direction: 1,
            in_reverse: false,
            yaw: 0.0,
            prev_yaw: 0.0,
            on_ground: false,
            rider: None,
        }
    }
}

impl Minecart {
    pub fn new(kind: CartKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    /// `addVelocity`.
    fn push_motion(&mut self, x: f32, z: f32) {
        self.motion.x += x;
        self.motion.z += z;
    }

    /// `attackEntityFrom`: shake, add `amount * 10` damage, and report whether
    /// the cart now breaks.
    pub fn hurt(&mut self, amount: i32) -> bool {
        self.rock_direction = -self.rock_direction;
        self.time_since_hit = 10;
        self.damage += amount * 10;
        self.damage > BREAK_DAMAGE
    }
}

/// A chest cart's contents (`cargoItems`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Cargo(pub [Option<ItemStack>; CARGO_SLOTS]);

/// `ItemMinecart.onItemUse`: a cart of `kind` on the rail in `cell`.
pub fn spawn_minecart(commands: &mut Commands, cell: IVec3) -> Entity {
    spawn_cart(commands, cell, CartKind::Empty)
}

pub fn spawn_cart(commands: &mut Commands, cell: IVec3, kind: CartKind) -> Entity {
    spawn_cart_at(
        commands,
        cell.as_vec3() + Vec3::new(0.5, 0.5, 0.5),
        Minecart::new(kind),
        None,
    )
}

/// A resting empty cart whose centre is `center`.
pub fn spawn_minecart_at(commands: &mut Commands, center: Vec3) -> Entity {
    spawn_cart_at(commands, center, Minecart::default(), None)
}

pub fn spawn_cart_at(
    commands: &mut Commands,
    center: Vec3,
    cart: Minecart,
    cargo: Option<Cargo>,
) -> Entity {
    let kind = cart.kind;
    let mut entity = commands.spawn((
        Name::new("Minecart"),
        cart,
        Seat::default(),
        CART_SIZE,
        PreviousTick(center),
        Transform::from_translation(center),
        Visibility::default(),
    ));
    if kind == CartKind::Chest {
        entity.insert(cargo.unwrap_or_default());
    }
    entity.id()
}

fn is_rail(block: Block) -> bool {
    matches!(
        block,
        Block::Rail | Block::PoweredRail | Block::DetectorRail
    )
}

fn rail_block(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> Option<Block> {
    chunks.block_at(x, y, z).filter(|block| is_rail(*block))
}

/// `EntityMinecart.MATRIX`: the two ends of each of the ten rail shapes, as
/// (dx, dy, dz) from the centre of the rail block.
const MATRIX: [[[i32; 3]; 2]; 10] = [
    [[0, 0, -1], [0, 0, 1]],
    [[-1, 0, 0], [1, 0, 0]],
    [[-1, -1, 0], [1, 0, 0]],
    [[-1, 0, 0], [1, -1, 0]],
    [[0, 0, -1], [0, -1, 1]],
    [[0, -1, -1], [0, 0, 1]],
    [[0, 0, 1], [1, 0, 0]],
    [[0, 0, 1], [-1, 0, 0]],
    [[0, 0, -1], [-1, 0, 0]],
    [[0, 0, -1], [1, 0, 0]],
];

/// The rail shape a block's metadata names: powered and detector rails keep
/// their power bit above it.
fn shape(block: Block, metadata: u8) -> usize {
    let shape = if block == Block::Rail {
        metadata
    } else {
        metadata & 7
    };
    usize::from(shape).min(9)
}

/// The rail cell a cart at `p` runs on: its own, or the one below.
fn rail_cell(chunks: &WorldChunks, p: Vec3) -> IVec3 {
    let mut cell = p.floor().as_ivec3();
    if rail_block(chunks, cell.x, cell.y - 1, cell.z).is_some() {
        cell.y -= 1;
    }
    cell
}

/// `EntityMinecart.getPos`: where the rail puts a cart at `p`, or `None` when
/// it is off the track. The result is a centre.
pub fn get_pos(chunks: &WorldChunks, p: Vec3) -> Option<Vec3> {
    let cell = rail_cell(chunks, p);
    let block = rail_block(chunks, cell.x, cell.y, cell.z)?;
    let shape = shape(block, chunks.metadata_at(cell.x, cell.y, cell.z));
    let ends = MATRIX[shape];
    let end = |index: usize| {
        Vec3::new(
            cell.x as f32 + 0.5 + ends[index][0] as f32 * 0.5,
            cell.y as f32 + 0.5 + ends[index][1] as f32 * 0.5,
            cell.z as f32 + 0.5 + ends[index][2] as f32 * 0.5,
        )
    };
    let (a, b) = (end(0), end(1));
    let along = b.x - a.x;
    let rise = (b.y - a.y) * 2.0;
    let across = b.z - a.z;
    let t = if along == 0.0 {
        p.z - cell.z as f32
    } else if across == 0.0 {
        p.x - cell.x as f32
    } else {
        ((p.x - a.x) * along + (p.z - a.z) * across) * 2.0
    };
    let x = a.x + along * t;
    let mut y = a.y + rise * t;
    let z = a.z + across * t;
    if rise < 0.0 {
        y += 1.0;
    }
    if rise > 0.0 {
        y += 0.5;
    }
    Some(Vec3::new(x, y, z))
}

/// `EntityMinecart.getPosOffset`: [`get_pos`] a little way along the track.
pub fn get_pos_offset(chunks: &WorldChunks, p: Vec3, distance: f32) -> Option<Vec3> {
    let cell = rail_cell(chunks, p);
    let block = rail_block(chunks, cell.x, cell.y, cell.z)?;
    let shape = shape(block, chunks.metadata_at(cell.x, cell.y, cell.z));
    let ends = MATRIX[shape];
    let mut y = cell.y as f32;
    if (2..=5).contains(&shape) {
        y += 1.0;
    }
    let dx = (ends[1][0] - ends[0][0]) as f32;
    let dz = (ends[1][2] - ends[0][2]) as f32;
    let length = (dx * dx + dz * dz).sqrt();
    let x = p.x + dx / length * distance;
    let z = p.z + dz / length * distance;
    let reached = |end: [i32; 3]| {
        end[1] != 0 && x.floor() as i32 - cell.x == end[0] && z.floor() as i32 - cell.z == end[2]
    };
    if reached(ends[0]) {
        y += ends[0][1] as f32;
    } else if reached(ends[1]) {
        y += ends[1][1] as f32;
    }
    get_pos(chunks, Vec3::new(x, y, z))
}

/// `Entity.moveEntity` for a cart: sweep the box, zero the motion on the axes
/// that were blocked, and record whether it landed.
fn move_cart(cart: &mut Minecart, center: &mut Vec3, delta: Vec3, chunks: &WorldChunks) {
    let aabb = CART_SIZE.aabb(*center);
    let moved = move_entity(aabb, delta, 0.0, cart.on_ground, chunks);
    *center = CART_SIZE.position_from_aabb(moved.aabb);
    cart.on_ground = delta.y < 0.0 && moved.collision.collided_y;
    if moved.collision.collided_x {
        cart.motion.x = 0.0;
    }
    if moved.collision.collided_y {
        cart.motion.y = 0.0;
    }
    if moved.collision.collided_z {
        cart.motion.z = 0.0;
    }
}

/// `EntityMinecart.onUpdate` for one world tick (the server branch).
/// Returns true when a `largesmoke` particle should spawn this tick.
pub fn step_minecart(
    cart: &mut Minecart,
    center: &mut Vec3,
    chunks: &WorldChunks,
    rng: &mut JavaRandom,
) -> bool {
    if cart.time_since_hit > 0 {
        cart.time_since_hit -= 1;
    }
    if cart.damage > 0 {
        cart.damage -= 1;
    }
    let previous = *center;
    cart.prev_yaw = cart.yaw;
    let yaw_start = cart.yaw;
    cart.motion.y -= 0.04;
    let cell = rail_cell(chunks, *center);
    let max_speed = 0.4;
    let mut pushing = false;
    if let Some(block) = rail_block(chunks, cell.x, cell.y, cell.z) {
        let before = get_pos(chunks, *center);
        let metadata = chunks.metadata_at(cell.x, cell.y, cell.z);
        center.y = cell.y as f32;
        let boosting = block == Block::PoweredRail && metadata & 8 != 0;
        let braking = block == Block::PoweredRail && !boosting;
        let shape_id = shape(block, metadata);
        if (2..=5).contains(&shape_id) {
            center.y = cell.y as f32 + 1.0;
        }
        match shape_id {
            2 => cart.motion.x -= 0.007_812_5,
            3 => cart.motion.x += 0.007_812_5,
            4 => cart.motion.z += 0.007_812_5,
            5 => cart.motion.z -= 0.007_812_5,
            _ => {}
        }
        let ends = MATRIX[shape_id];
        let mut along = (ends[1][0] - ends[0][0]) as f32;
        let mut across = (ends[1][2] - ends[0][2]) as f32;
        let length = (along * along + across * across).sqrt();
        if cart.motion.x * along + cart.motion.z * across < 0.0 {
            along = -along;
            across = -across;
        }
        let speed = cart.motion.xz().length();
        cart.motion.x = speed * along / length;
        cart.motion.z = speed * across / length;
        if braking {
            if cart.motion.xz().length() < 0.03 {
                cart.motion = Vec3::ZERO;
            } else {
                cart.motion.x *= 0.5;
                cart.motion.y = 0.0;
                cart.motion.z *= 0.5;
            }
        }
        let start_x = cell.x as f32 + 0.5 + ends[0][0] as f32 * 0.5;
        let start_z = cell.z as f32 + 0.5 + ends[0][2] as f32 * 0.5;
        let end_x = cell.x as f32 + 0.5 + ends[1][0] as f32 * 0.5;
        let end_z = cell.z as f32 + 0.5 + ends[1][2] as f32 * 0.5;
        along = end_x - start_x;
        across = end_z - start_z;
        let t = if along == 0.0 {
            center.x = cell.x as f32 + 0.5;
            center.z - cell.z as f32
        } else if across == 0.0 {
            center.z = cell.z as f32 + 0.5;
            center.x - cell.x as f32
        } else {
            ((center.x - start_x) * along + (center.z - start_z) * across) * 2.0
        };
        center.x = start_x + along * t;
        center.z = start_z + across * t;
        center.y += CART_SIZE.y_offset;
        let mut move_x = cart.motion.x;
        let mut move_z = cart.motion.z;
        if cart.rider.is_some() {
            move_x *= 0.75;
            move_z *= 0.75;
        }
        move_x = move_x.clamp(-max_speed, max_speed);
        move_z = move_z.clamp(-max_speed, max_speed);
        move_cart(cart, center, Vec3::new(move_x, 0.0, move_z), chunks);
        let reached = |end: [i32; 3]| {
            end[1] != 0
                && center.x.floor() as i32 - cell.x == end[0]
                && center.z.floor() as i32 - cell.z == end[2]
        };
        if reached(ends[0]) {
            center.y += ends[0][1] as f32;
        } else if reached(ends[1]) {
            center.y += ends[1][1] as f32;
        }
        if cart.rider.is_some() {
            cart.motion.x *= 0.997;
            cart.motion.y = 0.0;
            cart.motion.z *= 0.997;
        } else {
            if cart.kind == CartKind::Furnace {
                let push = cart.push.length();
                if push > 0.01 {
                    pushing = true;
                    cart.push /= push;
                    cart.motion.x *= 0.8;
                    cart.motion.y = 0.0;
                    cart.motion.z *= 0.8;
                    cart.motion.x += cart.push.x * 0.04;
                    cart.motion.z += cart.push.y * 0.04;
                } else {
                    cart.motion.x *= 0.9;
                    cart.motion.y = 0.0;
                    cart.motion.z *= 0.9;
                }
            }
            cart.motion.x *= 0.96;
            cart.motion.y = 0.0;
            cart.motion.z *= 0.96;
        }
        let after = get_pos(chunks, *center);
        if let (Some(after), Some(before)) = (after, before) {
            let slope = (before.y - after.y) * 0.05;
            let speed = cart.motion.xz().length();
            if speed > 0.0 {
                cart.motion.x = cart.motion.x / speed * (speed + slope);
                cart.motion.z = cart.motion.z / speed * (speed + slope);
            }
            center.y = after.y;
        }
        let now_x = center.x.floor() as i32;
        let now_z = center.z.floor() as i32;
        if now_x != cell.x || now_z != cell.z {
            let speed = cart.motion.xz().length();
            cart.motion.x = speed * (now_x - cell.x) as f32;
            cart.motion.z = speed * (now_z - cell.z) as f32;
        }
        if cart.kind == CartKind::Furnace {
            let push = cart.push.length();
            if push > 0.01 && cart.motion.xz().length_squared() > 0.001 {
                cart.push /= push;
                if cart.push.dot(cart.motion.xz()) < 0.0 {
                    cart.push = Vec2::ZERO;
                } else {
                    cart.push = cart.motion.xz();
                }
            }
        }
        if boosting {
            let speed = cart.motion.xz().length();
            if speed > 0.01 {
                cart.motion.x += cart.motion.x / speed * 0.06;
                cart.motion.z += cart.motion.z / speed * 0.06;
            } else if shape_id == 1 {
                let solid = |dx: i32| {
                    chunks
                        .block_at(cell.x + dx, cell.y, cell.z)
                        .is_some_and(Block::is_normal_cube)
                };
                if solid(-1) {
                    cart.motion.x = 0.02;
                } else if solid(1) {
                    cart.motion.x = -0.02;
                }
            } else if shape_id == 0 {
                let solid = |dz: i32| {
                    chunks
                        .block_at(cell.x, cell.y, cell.z + dz)
                        .is_some_and(Block::is_normal_cube)
                };
                if solid(-1) {
                    cart.motion.z = 0.02;
                } else if solid(1) {
                    cart.motion.z = -0.02;
                }
            }
        }
    } else {
        cart.motion.x = cart.motion.x.clamp(-max_speed, max_speed);
        cart.motion.z = cart.motion.z.clamp(-max_speed, max_speed);
        if cart.on_ground {
            cart.motion *= 0.5;
        }
        let motion = cart.motion;
        move_cart(cart, center, motion, chunks);
        if !cart.on_ground {
            cart.motion *= 0.95;
        }
    }
    let dx = previous.x - center.x;
    let dz = previous.z - center.z;
    if dx * dx + dz * dz > 0.001 {
        cart.yaw = dz.atan2(dx).to_degrees();
        if cart.in_reverse {
            cart.yaw += 180.0;
        }
    }
    // The cart turns half a turn when it reverses on the spot.
    let mut change = cart.yaw - yaw_start;
    while change >= 180.0 {
        change -= 360.0;
    }
    while change < -180.0 {
        change += 360.0;
    }
    if !(-170.0..170.0).contains(&change) {
        cart.yaw += 180.0;
        cart.in_reverse = !cart.in_reverse;
    }
    if pushing && rng.next_int(4) == 0 {
        cart.fuel -= 1;
        if cart.fuel < 0 {
            cart.push = Vec2::ZERO;
        }
        return true;
    }
    false
}

/// The cart state `applyEntityCollision` reads and writes.
pub struct CartBody<'a> {
    pub cart: &'a mut Minecart,
    pub center: Vec3,
    /// `prevPosX/Z`: where the cart stood at the start of its last update.
    pub previous: Vec3,
}

/// The scaled offset `applyEntityCollision` pushes along, or `None` when the
/// two are too close to tell apart. `from` is the pusher.
pub(crate) fn push_offset(this: Vec3, from: Vec3) -> Option<Vec2> {
    let mut x = from.x - this.x;
    let mut z = from.z - this.z;
    let distance_squared = x * x + z * z;
    if distance_squared < 0.0001 {
        return None;
    }
    let distance = distance_squared.sqrt();
    x /= distance;
    z /= distance;
    let scale = (1.0 / distance).min(1.0);
    Some(Vec2::new(x * scale * 0.1 * 0.5, z * scale * 0.1 * 0.5))
}

/// `EntityMinecart.applyEntityCollision` against another cart: `this`
/// receives the bump from `other`.
pub fn collide_carts(this: &mut CartBody, other: &mut CartBody) {
    let Some(offset) = push_offset(this.center, other.center) else {
        return;
    };
    let across = other.center - this.center;
    let side = across.x * other.cart.motion.z + across.z * other.previous.x;
    if side * side > 5.0 {
        return;
    }
    let (a, b) = (&mut *this.cart, &mut *other.cart);
    let mut half = Vec2::new(b.motion.x + a.motion.x, b.motion.z + a.motion.z);
    if b.kind == CartKind::Furnace && a.kind != CartKind::Furnace {
        a.motion.x *= 0.2;
        a.motion.z *= 0.2;
        a.push_motion(b.motion.x - offset.x, b.motion.z - offset.y);
        b.motion.x *= 0.7;
        b.motion.z *= 0.7;
    } else if b.kind != CartKind::Furnace && a.kind == CartKind::Furnace {
        b.motion.x *= 0.2;
        b.motion.z *= 0.2;
        b.push_motion(a.motion.x + offset.x, a.motion.z + offset.y);
        a.motion.x *= 0.7;
        a.motion.z *= 0.7;
    } else {
        half /= 2.0;
        a.motion.x *= 0.2;
        a.motion.z *= 0.2;
        a.push_motion(half.x - offset.x, half.y - offset.y);
        b.motion.x *= 0.2;
        b.motion.z *= 0.2;
        b.push_motion(half.x + offset.x, half.y + offset.y);
    }
}

/// `applyEntityCollision` against a body that is not a cart. The cart is
/// shoved away from `body`; the velocity the body takes in return is
/// returned.
pub fn push_cart(cart: &mut Minecart, center: Vec3, body: Vec3) -> Option<Vec2> {
    let offset = push_offset(center, body)?;
    cart.push_motion(-offset.x, -offset.y);
    Some(offset / 4.0)
}

/// Whether a bumping non-player creature boards: `applyEntityCollision`'s
/// check on an empty, free cart that is moving.
pub fn boards(cart: &Minecart) -> bool {
    cart.kind == CartKind::Empty
        && cart.rider.is_none()
        && cart.motion.x * cart.motion.x + cart.motion.z * cart.motion.z > 0.01
}

/// `attackEntityFrom` past the damage limit: the rider steps off, a chest
/// cart spills its cargo (`setEntityDead`), and the cart leaves a minecart
/// item and the chest or furnace it carried.
pub fn break_cart(
    commands: &mut Commands,
    rng: &mut ItemRng,
    entity: Entity,
    cart: &Minecart,
    center: Vec3,
    cargo: Option<&Cargo>,
) {
    if let Some(rider) = cart.rider {
        dismount(commands, rider);
    }
    if let Some(cargo) = cargo {
        spawn_chest_drops(
            commands,
            rng,
            center.floor().as_ivec3(),
            cargo.0.iter().flatten().copied(),
        );
    }
    commands.entity(entity).despawn();
    let mut leave = vec![Item::Minecart];
    if let Some(block) = cart.kind.block()
        && let Some(item) = Item::from_block(block)
    {
        leave.push(item);
    }
    for item in leave {
        if let Ok(stack) = ItemStack::new(item, 1) {
            spawn_entity_drop(commands, rng, center, stack);
        }
    }
}

/// `EntityLiving.onLivingUpdate`'s last step for the creatures and the player
/// that overlap a cart: the cart takes `applyEntityCollision`, the body is
/// nudged back, and a creature that bumps a free, moving empty cart climbs
/// aboard.
pub(crate) fn bump_carts(
    mut commands: Commands,
    tick: Res<WorldTick>,
    mut carts: Query<(Entity, &Transform, &mut Minecart), (Without<Living>, Without<Player>)>,
    mut bodies: Query<
        (
            Entity,
            &Transform,
            &EntitySize,
            &mut Velocity,
            Option<&Mob>,
            Has<Player>,
        ),
        (Or<(With<Living>, With<Player>)>, Without<Mounted>),
    >,
) {
    for _ in 0..tick.ticks_this_frame() {
        for (cart_entity, cart_transform, mut cart) in &mut carts {
            let center = cart_transform.translation;
            let cart_box = CART_SIZE.aabb(center);
            for (body, transform, size, mut velocity, mob, player) in &mut bodies {
                if mob.is_some_and(|mob| mob.health <= 0) {
                    continue;
                }
                let reach = grow(size.aabb(transform.translation), Vec3::new(0.2, 0.0, 0.2));
                if !reach.intersects(cart_box) {
                    continue;
                }
                if !player && boards(&cart) {
                    cart.rider = Some(body);
                    mount(&mut commands, body, cart_entity);
                }
                if let Some(back) = push_cart(&mut cart, center, transform.translation) {
                    velocity.0.x += back.x / TICK_SECONDS;
                    velocity.0.z += back.y / TICK_SECONDS;
                }
            }
        }
    }
}

/// Seconds per world tick, to turn a per-tick change of motion into
/// [`Velocity`]'s blocks per second.
const TICK_SECONDS: f32 = 0.05;

/// Seeds `Local<CartRandom>`.
pub(crate) struct CartRandom(JavaRandom);

impl Default for CartRandom {
    fn default() -> Self {
        Self(JavaRandom::new(0x4341_5254))
    }
}

/// Run the carts' Beta update once per world tick, in a stable order, with
/// the cart-to-cart bumps of `EntityMinecart.onUpdate` after each cart moves.
pub(crate) fn tick_minecarts(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    mut particles: ResMut<crate::entity::ParticleEmits>,
    mut carts: Query<(Entity, &mut Minecart, &mut Transform, &mut PreviousTick)>,
    mut random: Local<CartRandom>,
    mut order: Local<Vec<Entity>>,
) {
    let ticks = tick.ticks_this_frame();
    if ticks == 0 {
        return;
    }
    order.clear();
    order.extend(carts.iter().map(|(entity, ..)| entity));
    order.sort();
    for _ in 0..ticks {
        for &entity in order.iter() {
            let Ok((_, mut cart, mut transform, mut previous)) = carts.get_mut(entity) else {
                continue;
            };
            if !chunks.contains(ChunkPosition::from_world(
                transform.translation.x,
                transform.translation.z,
            )) {
                continue;
            }
            previous.0 = transform.translation;
            let mut center = transform.translation;
            let smoke = step_minecart(&mut cart, &mut center, &chunks, &mut random.0);
            transform.translation = center;
            if smoke {
                particles.large_smoke(center + Vec3::Y * 0.8);
            }
            let reach = grow(CART_SIZE.aabb(center), Vec3::new(0.2, 0.0, 0.2));
            let before = previous.0;
            drop((cart, transform, previous));
            for &other in order.iter() {
                if other == entity {
                    continue;
                }
                let Ok(
                    [
                        (_, mut this, this_transform, _),
                        (_, mut mover, mover_transform, _),
                    ],
                ) = carts.get_many_mut([other, entity])
                else {
                    continue;
                };
                if !CART_SIZE.aabb(this_transform.translation).intersects(reach) {
                    continue;
                }
                collide_carts(
                    &mut CartBody {
                        cart: &mut this,
                        center: this_transform.translation,
                        previous: this_transform.translation,
                    },
                    &mut CartBody {
                        cart: &mut mover,
                        center: mover_transform.translation,
                        previous: before,
                    },
                );
            }
        }
    }
}

fn grow(aabb: Aabb, amount: Vec3) -> Aabb {
    Aabb::new(aabb.min - amount, aabb.max + amount)
}

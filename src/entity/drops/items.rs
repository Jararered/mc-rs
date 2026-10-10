//! World item entities: Beta `EntityItem` motion, hazards, and pickup.
//!
//! Motion is in blocks per tick at 20 Hz, separate from the player integrator.
//! `rendering::dropped_items` draws them.

use bevy::prelude::*;

use crate::app::settings::GameSettings;
use crate::block::blocks::Block;
use crate::block::fluids::is_lava;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::PreviousTick;
use crate::entity::Shadow;
use crate::entity::drops::blocks::DropRoll;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::Aabb;
use crate::physics::PhysicsSet;
use crate::physics::WATER_CURRENT_PER_TICK;
use crate::physics::burning_in;
use crate::physics::eye_in_water;
use crate::physics::lava_contains;
use crate::physics::move_entity;
use crate::physics::touches_cactus;
use crate::physics::water_current;
use crate::player::Player;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::world::chunk::ChunkDroppedItem;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::tick::WorldTick;

const ITEM_LIFETIME_TICKS: u32 = 6000;
const PICKUP_DELAY_TICKS: u16 = 10;
const THROW_DELAY_TICKS: u16 = 40;
/// `posY - 0.3 + EntityPlayer.getEyeHeight()`. Eye height is 0.12, and our
/// transform is already at eye level.
const THROW_EYE_DROP: f32 = -0.18;
const PICKUP_TICKS: u8 = 3;
const ITEM_GRAVITY_PER_TICK: f32 = 0.04;
const ITEM_VERTICAL_DRAG: f32 = 0.98;
const DEFAULT_SLIPPERINESS: f32 = 0.6;
const ICE_SLIPPERINESS: f32 = 0.98;
const FLOAT_RISE_PER_TICK: f32 = 5.0e-4;
const FLOAT_RISE_LIMIT: f32 = 0.06;
const FLOAT_PLUNGE_DRAG: f32 = 0.8;
pub struct DroppedItemPlugin;

impl Plugin for DroppedItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (tick_dropped_items, pickup_dropped_items)
                .chain()
                .after(PhysicsSet::Integrate)
                // A hosted dimension has no screen state and always plays.
                .run_if(crate::world::tick::playing),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub struct DroppedItemState {
    pub pickup_delay_ticks: u16,
    pub age_ticks: u32,
    pub hover_start: f32,
    /// `EntityItem.health`: fire, lava, and cactus wear it down, and the item
    /// is gone at zero.
    pub health: u8,
    /// `Entity.fire`: ticks left burning, or negative while not alight.
    pub fire: i16,
    rng: JavaRandom,
}

impl DroppedItemState {
    pub fn new(pickup_delay_ticks: u16, hover_start: f32, rng_seed: u64) -> Self {
        Self {
            pickup_delay_ticks,
            age_ticks: 0,
            hover_start,
            health: ChunkDroppedItem::FULL_HEALTH,
            fire: 0,
            rng: JavaRandom::new(rng_seed),
        }
    }

    /// `EntityItem.attackEntityFrom`.
    fn hurt(&mut self, amount: u8) {
        self.health = self.health.saturating_sub(amount);
    }

    /// The fire and lava part of `Entity.onEntityUpdate`, before the item
    /// moves: water puts it out, a burning item loses a point every second,
    /// and lava costs four and sets it alight.
    pub fn update_hazards(&mut self, in_water: bool, in_lava: bool) {
        if in_water {
            self.fire = 0;
        }
        if self.fire > 0 {
            if self.fire % 20 == 0 {
                self.hurt(1);
            }
            self.fire -= 1;
        }
        if in_lava {
            self.hurt(4);
            self.fire = 600;
        }
    }

    /// The tail of `Entity.moveEntity`, after the item has moved: a cactus it
    /// touches and any fire or lava its box reaches each cost a point.
    pub fn contact_hazards(&mut self, touches_cactus: bool, burning: bool, wet: bool) {
        if touches_cactus {
            self.hurt(1);
        }
        if burning {
            self.hurt(1);
            if !wet {
                self.fire += 1;
                if self.fire == 0 {
                    self.fire = 300;
                }
            }
        } else if self.fire <= 0 {
            self.fire = -1;
        }
        if wet && self.fire > 0 {
            self.fire = -1;
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.health == 0
    }

    pub fn from_saved(
        pickup_delay_ticks: u16,
        age_ticks: u32,
        hover_start: f32,
        rng_state: u64,
    ) -> Self {
        Self {
            pickup_delay_ticks,
            age_ticks,
            hover_start,
            health: ChunkDroppedItem::FULL_HEALTH,
            fire: 0,
            rng: JavaRandom::from_state(rng_state),
        }
    }

    pub fn with_hazards(mut self, health: u8, fire: i16) -> Self {
        self.health = health;
        self.fire = fire;
        self
    }

    pub fn rng_state(&self) -> u64 {
        self.rng.state()
    }
}

/// Velocity in blocks per tick, matching `EntityItem.motion`.
#[derive(Component, Clone, Copy, Debug)]
pub struct ItemMotion(pub Vec3);

/// `EntityPickupFX`: the item flies to the player for 3 ticks, then disappears.
/// Physics and saving skip this component.
#[derive(Component, Clone, Copy, Debug)]
pub struct PickupAnimation {
    pub start: Vec3,
    /// Ticks since the flight began. It lasts [`PICKUP_TICKS`].
    pub age_ticks: u32,
}

#[derive(Component)]
struct ItemChunkHome(ChunkPosition);

/// Break-drop an item inside a block cell. Position and motion match
/// `Block.dropBlockAsItem_do` plus the `EntityItem` constructor. Pickup waits 10 ticks.
pub fn spawn_block_drop(
    commands: &mut Commands,
    rng: &mut ItemRng,
    block: IVec3,
    stack: ItemStack,
) {
    let jitter = Vec3::new(rng.unit(), rng.unit(), rng.unit());
    spawn_item(
        commands,
        block_drop_position(block, jitter),
        stack,
        item_constructor_motion(rng.unit(), rng.unit()),
        PICKUP_DELAY_TICKS,
        rng.unit() * std::f32::consts::TAU,
        rng.next_u64(),
    );
}

/// `EntityLiving.entityDropItem`: an item dropped at a creature's feet, with
/// the `EntityItem` constructor's hop. Pickup waits 10 ticks.
pub fn spawn_entity_drop(commands: &mut Commands, rng: &mut ItemRng, feet: Vec3, stack: ItemStack) {
    spawn_item(
        commands,
        feet + Vec3::Y * EntitySize::DROPPED_ITEM.y_offset,
        stack,
        item_constructor_motion(rng.unit(), rng.unit()),
        PICKUP_DELAY_TICKS,
        rng.unit() * std::f32::consts::TAU,
        rng.next_u64(),
    );
}

/// Spill chest contents as Beta-style random piles when the chest block breaks.
pub fn spawn_chest_drops(
    commands: &mut Commands,
    rng: &mut ItemRng,
    block: IVec3,
    stacks: impl IntoIterator<Item = ItemStack>,
) {
    for stack in stacks {
        let position = Vec3::new(
            block.x as f32 + rng.unit() * 0.8 + 0.1,
            block.y as f32 + rng.unit() * 0.8 + 0.1,
            block.z as f32 + rng.unit() * 0.8 + 0.1,
        );
        let mut remaining = stack.count();
        while remaining > 0 {
            let count = ((rng.unit() * 21.0) as u8 + 10).min(remaining);
            remaining -= count;
            let pile = ItemStack::with_data(stack.item(), count, stack.data())
                .expect("a chest pile keeps valid item data and stack size");
            let (gaussian_x, gaussian_y) = gaussian_pair(rng);
            let (gaussian_z, _) = gaussian_pair(rng);
            let motion = Vec3::new(
                gaussian_x * 0.05,
                gaussian_y * 0.05 + 0.2,
                gaussian_z * 0.05,
            );
            spawn_item(
                commands,
                position,
                pile,
                motion,
                PICKUP_DELAY_TICKS,
                rng.unit() * std::f32::consts::TAU,
                rng.next_u64(),
            );
        }
    }
}

/// `BlockDispenser.dispenseItem` for an ordinary item: eject it from the
/// front at roughly 0.2 blocks/tick, with a small random spread.
pub fn spawn_dispensed_item(
    commands: &mut Commands,
    rng: &mut ItemRng,
    cell: IVec3,
    facing: u8,
    stack: ItemStack,
) {
    let direction = dispenser_direction(facing);
    let position = cell.as_vec3() + Vec3::new(0.5, 0.2, 0.5) + direction * 0.6;
    let (x, y) = gaussian_pair(rng);
    let (z, _) = gaussian_pair(rng);
    let motion = direction * (0.2 + rng.unit() * 0.1) + Vec3::new(x, y, z) * 0.045 + Vec3::Y * 0.2;
    spawn_item(
        commands,
        position,
        stack,
        motion,
        PICKUP_DELAY_TICKS,
        rng.unit() * std::f32::consts::TAU,
        rng.next_u64(),
    );
}

/// The unit vector a dispenser with Beta metadata `facing` (2 north, 3 south,
/// 4 west, 5 east) fires along.
pub fn dispenser_direction(facing: u8) -> Vec3 {
    match facing {
        2 => Vec3::NEG_Z,
        3 => Vec3::Z,
        4 => Vec3::NEG_X,
        _ => Vec3::X,
    }
}

fn gaussian_pair(rng: &mut ItemRng) -> (f32, f32) {
    let first = rng.unit().max(f32::MIN_POSITIVE);
    let second = rng.unit();
    let radius = (-2.0 * first.ln()).sqrt();
    let angle = second * std::f32::consts::TAU;
    (radius * angle.cos(), radius * angle.sin())
}

/// Player throw from eye height. `look` is the view direction. Pickup waits 40 ticks.
pub fn spawn_thrown_item(
    commands: &mut Commands,
    rng: &mut ItemRng,
    eye: &Transform,
    look: Vec3,
    stack: ItemStack,
) {
    let motion = thrown_item_motion(
        look,
        rng.unit() * std::f32::consts::TAU,
        rng.unit(),
        rng.unit(),
        rng.unit(),
    );
    spawn_item(
        commands,
        eye.translation + Vec3::new(0.0, THROW_EYE_DROP, 0.0),
        stack,
        motion,
        THROW_DELAY_TICKS,
        rng.unit() * std::f32::consts::TAU,
        rng.next_u64(),
    );
}

/// An item flung with a given motion, as `EntityFish.catchFish` sends its
/// catch to the angler. Pickup waits 10 ticks.
pub fn spawn_flung_item(
    commands: &mut Commands,
    rng: &mut ItemRng,
    position: Vec3,
    motion: Vec3,
    stack: ItemStack,
) {
    spawn_item(
        commands,
        position,
        stack,
        motion,
        PICKUP_DELAY_TICKS,
        rng.unit() * std::f32::consts::TAU,
        rng.next_u64(),
    );
}

/// Respawn an item that was stored in a chunk file.
pub fn spawn_saved_item(commands: &mut Commands, item: crate::world::chunk::ChunkDroppedItem) {
    commands.spawn((
        Name::new("Dropped item"),
        DroppedItem(item.stack),
        DroppedItemState::from_saved(
            item.pickup_delay_ticks,
            item.age_ticks,
            item.hover_start,
            item.rng_state,
        )
        .with_hazards(item.health, item.fire),
        Transform::from_translation(Vec3::from_array(item.position)),
        PreviousTick(Vec3::from_array(item.position)),
        ItemMotion(Vec3::from_array(item.motion)),
        CollisionState::default(),
        EntitySize::DROPPED_ITEM,
        Shadow::DROPPED_ITEM,
        ItemChunkHome(ChunkPosition::from_block(
            item.position[0].floor() as i32,
            item.position[2].floor() as i32,
        )),
    ));
}

pub fn chunk_record(
    stack: ItemStack,
    position: Vec3,
    motion: Vec3,
    state: &DroppedItemState,
) -> crate::world::chunk::ChunkDroppedItem {
    crate::world::chunk::ChunkDroppedItem {
        stack,
        position: position.to_array(),
        motion: motion.to_array(),
        age_ticks: state.age_ticks,
        pickup_delay_ticks: state.pickup_delay_ticks,
        hover_start: state.hover_start,
        rng_state: state.rng_state(),
        health: state.health,
        fire: state.fire,
    }
}

fn spawn_item(
    commands: &mut Commands,
    position: Vec3,
    stack: ItemStack,
    motion: Vec3,
    pickup_delay_ticks: u16,
    hover_start: f32,
    rng_seed: u64,
) {
    commands.spawn((
        Name::new("Dropped item"),
        DroppedItem(stack),
        DroppedItemState::new(pickup_delay_ticks, hover_start, rng_seed),
        Transform::from_translation(position),
        PreviousTick(position),
        ItemMotion(motion),
        CollisionState::default(),
        EntitySize::DROPPED_ITEM,
        Shadow::DROPPED_ITEM,
    ));
}

impl DropRoll for ItemRng {
    fn next_int(&mut self, bound: u32) -> u32 {
        let value = (self.unit() * bound as f32) as u32;
        if value >= bound { bound - 1 } else { value }
    }
}

/// `dropBlockAsItem_do`: each axis is `block + rand * 0.7 + 0.15`.
pub fn block_drop_position(block: IVec3, jitter: Vec3) -> Vec3 {
    let spread = 0.7;
    let edge = (1.0 - spread) * 0.5;
    Vec3::new(
        block.x as f32 + jitter.x * spread + edge,
        block.y as f32 + jitter.y * spread + edge,
        block.z as f32 + jitter.z * spread + edge,
    )
}

/// `EntityItem` constructor motion, in blocks per tick. `rx` and `rz` are in `[0, 1)`.
///
/// Beta's horizontal range is `±0.1`. That speed lasts the whole hop (about
/// eleven ticks), which lands the item in the next block. A tenth of that
/// still scatters the drop inside the broken cell.
pub fn item_constructor_motion(rx: f32, rz: f32) -> Vec3 {
    Vec3::new(rx * 0.02 - 0.01, 0.2, rz * 0.02 - 0.01)
}

/// Aimed throw from `EntityPlayer.dropPlayerItemWithRandomChoice` (`randomChoice == false`).
/// `look` is unit length in our camera space. The extra `+ 0.1` on Y is applied after the aim.
pub fn thrown_item_motion(look: Vec3, angle: f32, scale: f32, y_a: f32, y_b: f32) -> Vec3 {
    let mut motion = look.normalize_or_zero() * 0.3;
    motion.y += 0.1;
    let spread = 0.02 * scale;
    motion.x += angle.cos() * spread;
    motion.y += (y_a - y_b) * 0.1;
    motion.z += angle.sin() * spread;
    motion
}

pub fn apply_item_gravity(mut motion: Vec3) -> Vec3 {
    motion.y -= ITEM_GRAVITY_PER_TICK;
    motion
}

/// The Floating Items feature, after modern `ItemEntity.setUnderwaterMovement`:
/// a submerged item drifts with a little drag and rises until it climbs at
/// 0.06 blocks per tick. Its plunge is also damped, so a thrown item turns
/// around within a block or so rather than carrying on to the bottom.
pub fn item_float_motion(mut motion: Vec3) -> Vec3 {
    motion.x *= 0.99;
    motion.z *= 0.99;
    if motion.y < 0.0 {
        motion.y *= FLOAT_PLUNGE_DRAG;
    }
    if motion.y < FLOAT_RISE_LIMIT {
        motion.y += FLOAT_RISE_PER_TICK;
    }
    motion
}

/// Whether water covers an item deeply enough to carry it: its surface is
/// more than 0.1 above the bottom of the item's box.
pub fn item_floats(aabb: Aabb, chunks: &WorldChunks) -> bool {
    let center = (aabb.min + aabb.max) * 0.5;
    eye_in_water(Vec3::new(center.x, aabb.min.y + 0.1, center.z), chunks)
}

pub fn item_slipperiness(block: Option<Block>) -> f32 {
    if block == Some(Block::Ice) {
        ICE_SLIPPERINESS
    } else {
        DEFAULT_SLIPPERINESS
    }
}

/// Drag applied after `moveEntity` has zeroed the colliding axes.
/// The ground bounce multiplies Y after that zero, so a landing item does not hop.
pub fn item_motion_after_collision(
    mut motion: Vec3,
    collided_x: bool,
    collided_y: bool,
    collided_z: bool,
    on_ground: bool,
    slipperiness: f32,
) -> Vec3 {
    if collided_x {
        motion.x = 0.0;
    }
    if collided_y {
        motion.y = 0.0;
    }
    if collided_z {
        motion.z = 0.0;
    }
    let horizontal = if on_ground {
        slipperiness * ITEM_VERTICAL_DRAG
    } else {
        ITEM_VERTICAL_DRAG
    };
    motion.x *= horizontal;
    motion.y *= ITEM_VERTICAL_DRAG;
    motion.z *= horizontal;
    if on_ground {
        motion.y *= -0.5;
    }
    motion
}

/// Player box expanded by one block on X and Z, matching `boundingBox.expand(1, 0, 1)`.
pub fn item_reaches_player(player_size: EntitySize, player: Vec3, item: Vec3) -> bool {
    let mut reach = player_size.aabb(player);
    reach.min.x -= 1.0;
    reach.min.z -= 1.0;
    reach.max.x += 1.0;
    reach.max.z += 1.0;
    reach.intersects(EntitySize::DROPPED_ITEM.aabb(item))
}

/// `EntityPickupFX`: `t = ((age + partial) / 3)^2` toward the eyes, 0.5 blocks down.
pub fn pickup_position(start: Vec3, player_eye: Vec3, age_ticks: f32, partial: f32) -> Vec3 {
    let t = ((age_ticks + partial) / f32::from(PICKUP_TICKS)).clamp(0.0, 1.0);
    start.lerp(player_eye + Vec3::new(0.0, -0.5, 0.0), t * t)
}

/// `GuiIngame.renderInventorySlot` scale while `animationsToGo` is counting down.
pub fn hotbar_icon_scale(pop: u8, partial: f32) -> Vec2 {
    let pop = f32::from(pop) - partial;
    if pop <= 0.0 {
        return Vec2::ONE;
    }
    let grow = 1.0 + pop / 5.0;
    Vec2::new(1.0 / grow, (grow + 1.0) / 2.0)
}

fn tick_dropped_items(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    settings: Option<Res<GameSettings>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut commands: Commands,
    mut hotbars: Query<&mut Hotbar>,
    mut pickups: Query<(Entity, &mut PickupAnimation)>,
    mut items: Query<(
        Entity,
        &mut Transform,
        &mut PreviousTick,
        &mut ItemMotion,
        &mut CollisionState,
        &EntitySize,
        &mut DroppedItemState,
        Option<&mut ItemChunkHome>,
    )>,
) {
    let steps = tick.ticks_this_frame();
    for (entity, mut pickup) in &mut pickups {
        pickup.age_ticks = pickup.age_ticks.saturating_add(steps);
        if pickup.age_ticks >= u32::from(PICKUP_TICKS) {
            commands.entity(entity).despawn();
        }
    }
    if steps == 0 {
        return;
    }
    for mut hotbar in &mut hotbars {
        // Reading through Mut does not invalidate the HUD. Only an active
        // pickup animation needs to write the component.
        if hotbar.pop.iter().any(|&pop| pop > 0) {
            for pop in &mut hotbar.pop {
                *pop = pop.saturating_sub(steps.min(u32::from(u8::MAX)) as u8);
            }
        }
    }

    let floating = settings.is_some_and(|settings| settings.floating_items);
    for (
        entity,
        mut transform,
        mut previous_tick,
        mut motion,
        mut collision,
        size,
        mut state,
        mut home,
    ) in &mut items
    {
        if !chunks.contains(ChunkPosition::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }
        let mut removed = false;
        for _ in 0..steps {
            previous_tick.0 = transform.translation;
            state.age_ticks += 1;
            if state.pickup_delay_ticks > 0 {
                state.pickup_delay_ticks -= 1;
            }
            if state.age_ticks >= ITEM_LIFETIME_TICKS {
                mark_chunk(&mut persistence, transform.translation);
                commands.entity(entity).despawn();
                removed = true;
                break;
            }
            let aabb = size.aabb(transform.translation);
            let (in_water, current) = water_current(aabb, &chunks);
            state.update_hazards(in_water, lava_contains(aabb, &chunks));
            motion.0 += current * WATER_CURRENT_PER_TICK;
            // Beta's items sink; the Floating Items feature carries them up.
            motion.0 = if floating && item_floats(aabb, &chunks) {
                item_float_motion(motion.0)
            } else {
                apply_item_gravity(motion.0)
            };
            // `EntityItem.onUpdate`: an item in lava is spat back out.
            let cell = transform.translation.floor().as_ivec3();
            if chunks.block_at(cell.x, cell.y, cell.z).is_some_and(is_lava) {
                let rng = &mut state.rng;
                motion.0 = Vec3::new(
                    (rng.next_float() - rng.next_float()) * 0.2,
                    0.2,
                    (rng.next_float() - rng.next_float()) * 0.2,
                );
            }
            push_out_of_blocks(&chunks, &mut transform.translation, &mut motion.0);
            let movement = move_entity(
                size.aabb(transform.translation),
                motion.0,
                0.0,
                collision.on_ground,
                &chunks,
            );
            transform.translation = size.position_from_aabb(movement.aabb);
            *collision = movement.collision;
            let inset = Aabb::new(
                movement.aabb.min + Vec3::splat(0.001),
                movement.aabb.max - Vec3::splat(0.001),
            );
            state.contact_hazards(
                touches_cactus(movement.aabb, &chunks),
                burning_in(inset, &chunks),
                in_water,
            );
            if state.is_destroyed() {
                mark_chunk(&mut persistence, transform.translation);
                commands.entity(entity).despawn();
                removed = true;
                break;
            }
            let slip = item_slipperiness(block_under_item(&chunks, transform.translation, *size));
            motion.0 = item_motion_after_collision(
                motion.0,
                collision.collided_x,
                collision.collided_y,
                collision.collided_z,
                collision.on_ground,
                slip,
            );
        }
        if removed {
            continue;
        }
        let now = ChunkPosition::from_block(
            transform.translation.x.floor() as i32,
            transform.translation.z.floor() as i32,
        );
        match &mut home {
            Some(home) if home.0 != now => {
                mark_chunk_pos(&mut persistence, home.0);
                mark_chunk_pos(&mut persistence, now);
                home.0 = now;
            }
            None => {
                mark_chunk_pos(&mut persistence, now);
                commands.entity(entity).insert(ItemChunkHome(now));
            }
            Some(_) => {}
        }
    }
}

fn mark_chunk(persistence: &mut Option<ResMut<WorldPersistence>>, position: Vec3) {
    mark_chunk_pos(
        persistence,
        ChunkPosition::from_block(position.x.floor() as i32, position.z.floor() as i32),
    );
}

fn mark_chunk_pos(persistence: &mut Option<ResMut<WorldPersistence>>, position: ChunkPosition) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(position);
    }
}

fn block_under_item(chunks: &WorldChunks, position: Vec3, size: EntitySize) -> Option<Block> {
    let feet = size.aabb(position).min.y;
    let y = feet.floor() as i32 - 1;
    chunks.block_at(position.x.floor() as i32, y, position.z.floor() as i32)
}

fn push_out_of_blocks(chunks: &WorldChunks, position: &mut Vec3, motion: &mut Vec3) {
    let x = position.x.floor() as i32;
    let y = position.y.floor() as i32;
    let z = position.z.floor() as i32;
    if !chunks.block_at(x, y, z).is_some_and(Block::is_opaque_cube) {
        return;
    }
    let local_x = position.x - x as f32;
    let local_y = position.y - y as f32;
    let local_z = position.z - z as f32;
    let open = |bx: i32, by: i32, bz: i32| {
        !chunks
            .block_at(bx, by, bz)
            .is_some_and(Block::is_opaque_cube)
    };
    let mut best = -1i32;
    let mut best_distance = 9999.0f32;
    let candidates = [
        (0, open(x - 1, y, z), local_x),
        (1, open(x + 1, y, z), 1.0 - local_x),
        (2, open(x, y - 1, z), local_y),
        (3, open(x, y + 1, z), 1.0 - local_y),
        (4, open(x, y, z - 1), local_z),
        (5, open(x, y, z + 1), 1.0 - local_z),
    ];
    for (direction, is_open, distance) in candidates {
        if is_open && distance < best_distance {
            best_distance = distance;
            best = direction;
        }
    }
    // Place the 0.25 box just outside the solid. Beta instead assigns a
    // 0.1–0.3 block/tick velocity, which keeps going for the rest of the hop
    // and throws the item into the next block.
    let clear = EntitySize::DROPPED_ITEM.width * 0.5 + 0.001;
    match best {
        0 => {
            position.x = x as f32 - clear;
            motion.x = motion.x.min(0.0);
        }
        1 => {
            position.x = (x + 1) as f32 + clear;
            motion.x = motion.x.max(0.0);
        }
        2 => {
            position.y = y as f32 - clear;
            motion.y = motion.y.min(0.0);
        }
        3 => {
            position.y = (y + 1) as f32 + clear;
            motion.y = motion.y.max(0.0);
        }
        4 => {
            position.z = z as f32 - clear;
            motion.z = motion.z.min(0.0);
        }
        5 => {
            position.z = (z + 1) as f32 + clear;
            motion.z = motion.z.max(0.0);
        }
        _ => {}
    }
}

pub(crate) fn pickup_dropped_items(
    mut commands: Commands,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut player: Query<(&Transform, &EntitySize, &mut Hotbar, &mut Inventory), With<Player>>,
    mut items: Query<(
        Entity,
        &Transform,
        &DroppedItemState,
        &mut DroppedItem,
        &ItemMotion,
    )>,
) {
    'items: for (entity, transform, state, mut dropped, _motion) in &mut items {
        if state.pickup_delay_ticks > 0 {
            continue;
        }
        // `onCollideWithPlayer` runs for each player touching the item; the
        // first with room for any of it takes what fits.
        for (player_transform, player_size, mut hotbar, mut inventory) in &mut player {
            if !item_reaches_player(
                *player_size,
                player_transform.translation,
                transform.translation,
            ) {
                continue;
            }
            let original = dropped.0;
            // An item that does not fit changes nothing, so it must not flag
            // the inventory and hotbar as changed every frame the player
            // stands on it.
            let remainder = inventory
                .bypass_change_detection()
                .insert(hotbar.bypass_change_detection(), original);
            if remainder.is_none_or(|remainder| remainder.count() != original.count()) {
                inventory.set_changed();
                hotbar.set_changed();
            }
            if let Some(remainder) = remainder {
                if remainder.count() == original.count() {
                    continue;
                }
                dropped.0 = remainder;
                let taken = original.count() - remainder.count();
                if let Ok(stack) = ItemStack::with_data(original.item(), taken, original.data()) {
                    spawn_pickup_flyer(&mut commands, transform.translation, stack, state);
                }
            } else {
                commands
                    .entity(entity)
                    .remove::<ItemMotion>()
                    .insert(PickupAnimation {
                        start: transform.translation,
                        age_ticks: 0,
                    });
            }
            mark_chunk(&mut persistence, transform.translation);
            continue 'items;
        }
    }
}

fn spawn_pickup_flyer(
    commands: &mut Commands,
    origin: Vec3,
    stack: ItemStack,
    state: &DroppedItemState,
) {
    commands.spawn((
        Name::new("Item pickup"),
        DroppedItem(stack),
        DroppedItemState::from_saved(0, state.age_ticks, state.hover_start, state.rng_state()),
        Transform::from_translation(origin),
        PickupAnimation {
            start: origin,
            age_ticks: 0,
        },
        EntitySize::DROPPED_ITEM,
    ));
}

//! World item entities: Beta `EntityItem` motion and `RenderItem` drawing.
//!
//! Motion is in blocks per tick at 20 Hz, separate from the player integrator.
//! Full cubes use the world block mesh and spin about Y. Everything else is an
//! upright sprite that yaws to face the camera and does not spin.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::ecs::system::SystemParam;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::block_drops::DropRoll;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::PhysicsSet;
use crate::physics::move_entity;
use crate::player::Player;
use crate::random::ItemRng;
use crate::random::JavaRandom;
use crate::ui::block_icons::BlockIcons;
use crate::world::block::block::BlockId;
use crate::world::block::properties::is_crossed_plant;
use crate::world::block::properties::is_opaque_cube;
use crate::world::block::properties::is_torch;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPos;
use crate::world::chunk::WorldChunks;
use crate::world::meshing::dropped_block_meshes;
use crate::world::persistence::WorldPersistence;
use crate::world::textures::CutoutMaterial;
use crate::world::textures::FoliageColors;
use crate::world::textures::GrassColors;
use crate::world::textures::GrassOverlayMaterial;
use crate::world::textures::TerrainMaterial;
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
const CUBE_SCALE: f32 = 0.25;
const SPRITE_SCALE: f32 = 0.5;
/// `RenderItem` reseeds `java.util.Random` to this value every frame.
const PILE_SEED: u64 = 187;

pub struct DroppedItemPlugin;

impl Plugin for DroppedItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                tick_dropped_items,
                pickup_dropped_items,
                sync_item_rendering,
            )
                .chain()
                .after(PhysicsSet::Integrate)
                .run_if(in_state(AppScreen::Playing)),
        );
    }
}

#[derive(Component, Clone, Debug)]
pub struct DroppedItemState {
    pub pickup_delay_ticks: u16,
    pub age_ticks: u32,
    pub hover_start: f32,
    /// Position before the latest tick. Rendering lerps from here with `partial`.
    previous_position: Vec3,
    rng: JavaRandom,
}

impl DroppedItemState {
    pub fn new(pickup_delay_ticks: u16, hover_start: f32, rng_seed: u64, position: Vec3) -> Self {
        Self {
            pickup_delay_ticks,
            age_ticks: 0,
            hover_start,
            previous_position: position,
            rng: JavaRandom::new(rng_seed),
        }
    }

    pub fn from_saved(
        pickup_delay_ticks: u16,
        age_ticks: u32,
        hover_start: f32,
        rng_state: u64,
        position: Vec3,
    ) -> Self {
        Self {
            pickup_delay_ticks,
            age_ticks,
            hover_start,
            previous_position: position,
            rng: JavaRandom::from_state(rng_state),
        }
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
struct ItemChunkHome(ChunkPos);

#[derive(Component)]
struct ItemVisual {
    stack: ItemStack,
    fancy: bool,
    cube: bool,
}

#[derive(Component)]
struct ItemPilePiece {
    offset: Vec3,
}

#[derive(Resource)]
struct ItemDropMaterial(Handle<StandardMaterial>);

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

/// Respawn an item that was stored in a chunk file.
pub fn spawn_saved_item(commands: &mut Commands, item: crate::world::generation::ChunkDroppedItem) {
    commands.spawn((
        Name::new("Dropped item"),
        DroppedItem(item.stack),
        DroppedItemState::from_saved(
            item.pickup_delay_ticks,
            item.age_ticks,
            item.hover_start,
            item.rng_state,
            Vec3::from_array(item.position),
        ),
        Transform::from_translation(Vec3::from_array(item.position)),
        ItemMotion(Vec3::from_array(item.motion)),
        CollisionState::default(),
        EntitySize::DROPPED_ITEM,
        ItemChunkHome(ChunkPos::from_block(
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
) -> crate::world::generation::ChunkDroppedItem {
    crate::world::generation::ChunkDroppedItem {
        stack,
        position: position.to_array(),
        motion: motion.to_array(),
        age_ticks: state.age_ticks,
        pickup_delay_ticks: state.pickup_delay_ticks,
        hover_start: state.hover_start,
        rng_state: state.rng_state(),
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
        DroppedItemState::new(pickup_delay_ticks, hover_start, rng_seed, position),
        Transform::from_translation(position),
        ItemMotion(motion),
        CollisionState::default(),
        EntitySize::DROPPED_ITEM,
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

pub fn item_slipperiness(block: Option<BlockId>) -> f32 {
    if block == Some(BlockId::Ice) {
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

/// Render position between the previous tick and the current one.
///
/// Same shape as the arm swing: `previous + (current - previous) * partial`.
pub fn interpolated_item_position(previous: Vec3, current: Vec3, partial: f32) -> Vec3 {
    previous.lerp(current, partial.clamp(0.0, 1.0))
}

/// `sin((age + partial) / 10 + hover) * 0.1 + 0.1`. Age is in ticks.
pub fn item_bob_offset(age_ticks: f32, partial: f32, hover: f32) -> f32 {
    ((age_ticks + partial) / 10.0 + hover).sin() * 0.1 + 0.1
}

/// Cube yaw in radians: `(age + partial) / 20 + hover`, about +Y.
pub fn item_spin_yaw(age_ticks: f32, partial: f32, hover: f32) -> f32 {
    (age_ticks + partial) / 20.0 + hover
}

/// Sprites take the camera yaw. Cubes take the spin and ignore the camera.
pub fn item_visual_yaw(cube: bool, spin: f32, camera_yaw: f32) -> f32 {
    if cube { spin } else { camera_yaw }
}

pub fn item_stack_copies(count: u8) -> u8 {
    if count > 20 {
        4
    } else if count > 5 {
        3
    } else if count > 1 {
        2
    } else {
        1
    }
}

/// Local pile offsets. Copy 0 is the origin. Later copies read `java.util.Random(187)`.
/// Cube spreads are divided by the render scale, matching `RenderItem`.
pub fn item_pile_offsets(copies: u8, cube: bool, scale: f32) -> Vec<Vec3> {
    let mut random = JavaRandom::new(PILE_SEED);
    let spread = if cube { 0.2 / scale } else { 0.3 };
    (0..copies)
        .map(|index| {
            if index == 0 {
                Vec3::ZERO
            } else {
                Vec3::new(
                    (random.next_float() * 2.0 - 1.0) * spread,
                    (random.next_float() * 2.0 - 1.0) * spread,
                    (random.next_float() * 2.0 - 1.0) * spread,
                )
            }
        })
        .collect()
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

pub fn item_piece_transform(
    bob: f32,
    yaw: f32,
    scale: f32,
    offset: Vec3,
    slide: Vec3,
) -> Transform {
    let rotation = Quat::from_rotation_y(yaw);
    Transform::from_translation(slide + Vec3::Y * bob + rotation * (offset * scale))
        .with_rotation(rotation)
        .with_scale(Vec3::splat(scale))
}

/// Full cubes we already mesh in the world. Torches stay sprites (`renderType` 2).
pub fn dropped_block_model(stack: ItemStack) -> Option<BlockId> {
    let block = stack.runtime_block()?;
    if is_torch(block) || is_crossed_plant(block) {
        None
    } else {
        Some(block)
    }
}

fn tick_dropped_items(
    tick: Res<WorldTick>,
    chunks: Res<WorldChunks>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut commands: Commands,
    mut hotbars: Query<&mut Hotbar>,
    mut pickups: Query<(Entity, &mut PickupAnimation)>,
    mut items: Query<(
        Entity,
        &mut Transform,
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
    for _ in 0..steps {
        for mut hotbar in &mut hotbars {
            for pop in &mut hotbar.pop {
                *pop = pop.saturating_sub(1);
            }
        }
    }

    for (entity, mut transform, mut motion, mut collision, size, mut state, mut home) in &mut items
    {
        if !chunks.contains(ChunkPos::from_world(
            transform.translation.x,
            transform.translation.z,
        )) {
            continue;
        }
        for _ in 0..steps {
            state.previous_position = transform.translation;
            state.age_ticks += 1;
            if state.pickup_delay_ticks > 0 {
                state.pickup_delay_ticks -= 1;
            }
            if state.age_ticks >= ITEM_LIFETIME_TICKS {
                mark_chunk(&mut persistence, transform.translation);
                commands.entity(entity).despawn();
                break;
            }
            motion.0 = apply_item_gravity(motion.0);
            push_out_of_blocks(
                &chunks,
                &mut transform.translation,
                &mut motion.0,
                &mut state.rng,
            );
            let movement = move_entity(
                size.aabb(transform.translation),
                motion.0,
                0.0,
                collision.on_ground,
                &chunks,
            );
            transform.translation = size.position_from_aabb(movement.aabb);
            *collision = movement.collision;
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
        let now = ChunkPos::from_block(
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
        ChunkPos::from_block(position.x.floor() as i32, position.z.floor() as i32),
    );
}

fn mark_chunk_pos(persistence: &mut Option<ResMut<WorldPersistence>>, position: ChunkPos) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(position);
    }
}

fn block_under_item(chunks: &WorldChunks, position: Vec3, size: EntitySize) -> Option<BlockId> {
    let feet = size.aabb(position).min.y;
    let y = feet.floor() as i32 - 1;
    chunks.block_at(position.x.floor() as i32, y, position.z.floor() as i32)
}

fn push_out_of_blocks(
    chunks: &WorldChunks,
    position: &mut Vec3,
    motion: &mut Vec3,
    _rng: &mut JavaRandom,
) {
    let x = position.x.floor() as i32;
    let y = position.y.floor() as i32;
    let z = position.z.floor() as i32;
    if !chunks.block_at(x, y, z).is_some_and(is_opaque_cube) {
        return;
    }
    let local_x = position.x - x as f32;
    let local_y = position.y - y as f32;
    let local_z = position.z - z as f32;
    let open = |bx: i32, by: i32, bz: i32| !chunks.block_at(bx, by, bz).is_some_and(is_opaque_cube);
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

fn pickup_dropped_items(
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
    let Ok((player_transform, player_size, mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    for (entity, transform, state, mut dropped, _motion) in &mut items {
        if state.pickup_delay_ticks > 0
            || !item_reaches_player(
                *player_size,
                player_transform.translation,
                transform.translation,
            )
        {
            continue;
        }
        let original = dropped.0;
        if let Some(remainder) = inventory.insert(&mut hotbar, original) {
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
        DroppedItemState::from_saved(
            0,
            state.age_ticks,
            state.hover_start,
            state.rng_state(),
            origin,
        ),
        Transform::from_translation(origin),
        PickupAnimation {
            start: origin,
            age_ticks: 0,
        },
        EntitySize::DROPPED_ITEM,
    ));
}

#[derive(SystemParam)]
struct ItemRenderResources<'w> {
    settings: Res<'w, GameSettings>,
    chunks: Res<'w, WorldChunks>,
    grass_colors: Option<Res<'w, GrassColors>>,
    foliage_colors: Option<Res<'w, FoliageColors>>,
    terrain: Option<Res<'w, TerrainMaterial>>,
    grass_overlay: Option<Res<'w, GrassOverlayMaterial>>,
    cutout: Option<Res<'w, CutoutMaterial>>,
    icons: Option<Res<'w, BlockIcons>>,
}

fn sync_item_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    world: ItemRenderResources,
    camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    player: Query<&Transform, (With<Player>, Without<DroppedItem>, Without<ItemPilePiece>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    icon_material: Option<Res<ItemDropMaterial>>,
    mut items: Query<(
        Entity,
        &mut Transform,
        &DroppedItem,
        &DroppedItemState,
        Option<&ItemVisual>,
        Option<&Children>,
        Option<&PickupAnimation>,
    )>,
    mut pieces: Query<(&ItemPilePiece, &mut Transform), Without<DroppedItem>>,
) {
    let fancy = world.settings.graphics.fancy_leaves();
    let camera_yaw = camera.single().map_or(0.0, |camera| {
        let (yaw, _, _) = camera.rotation().to_euler(EulerRot::YXZ);
        yaw
    });
    let icon_material = icon_material
        .as_ref()
        .map(|material| material.0.clone())
        .or_else(|| {
            let icons = world.icons.as_ref()?;
            if !icons.ready() {
                return None;
            }
            let handle = materials.add(StandardMaterial {
                base_color_texture: Some(icons.image.clone()),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            commands.insert_resource(ItemDropMaterial(handle.clone()));
            Some(handle)
        });

    let player_eye = player.single().ok().map(|transform| transform.translation);
    for (entity, mut transform, dropped, state, visual, children, pickup) in &mut items {
        if let (Some(pickup), Some(player_eye)) = (pickup, player_eye) {
            transform.translation = pickup_position(
                pickup.start,
                player_eye,
                pickup.age_ticks as f32,
                tick.partial(),
            );
        }
        let model = dropped_block_model(dropped.0);
        let cube = model.is_some();
        let current = visual.is_some_and(|visual| {
            visual.stack == dropped.0 && visual.fancy == fancy && visual.cube == cube
        });
        let bob = item_bob_offset(state.age_ticks as f32, tick.partial(), state.hover_start);
        let spin = item_spin_yaw(state.age_ticks as f32, tick.partial(), state.hover_start);
        // Physics keeps the post-tick position. The mesh is a child, so this
        // local slide shows the in-between point without moving the simulation.
        let slide = if pickup.is_some() {
            Vec3::ZERO
        } else {
            interpolated_item_position(
                state.previous_position,
                transform.translation,
                tick.partial(),
            ) - transform.translation
        };
        let yaw = item_visual_yaw(cube, spin, camera_yaw);
        let scale = if cube { CUBE_SCALE } else { SPRITE_SCALE };
        if !current {
            if !visual_ready(
                cube,
                world.terrain.as_deref(),
                world.icons.as_deref(),
                dropped.0,
            ) {
                continue;
            }
            if let Some(children) = children {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }
            let offsets = item_pile_offsets(item_stack_copies(dropped.0.count()), cube, scale);
            if let Some(block) = model {
                spawn_block_pieces(
                    &mut commands,
                    &mut meshes,
                    entity,
                    block,
                    fancy,
                    climate_tints(
                        &world.chunks,
                        world.grass_colors.as_deref(),
                        world.foliage_colors.as_deref(),
                        transform.translation,
                    ),
                    world.terrain.as_deref(),
                    world.grass_overlay.as_deref(),
                    world.cutout.as_deref(),
                    &offsets,
                    bob,
                    yaw,
                    scale,
                    slide,
                );
            } else {
                let Some(material) = icon_material.clone() else {
                    continue;
                };
                let Some((u0, v0, u1, v1)) = world
                    .icons
                    .as_ref()
                    .and_then(|icons| icons.uv_for_stack(dropped.0))
                else {
                    continue;
                };
                for offset in offsets {
                    let child = commands
                        .spawn((
                            item_quad(u0, v0, u1, v1, &mut meshes),
                            MeshMaterial3d(material.clone()),
                            item_piece_transform(bob, yaw, scale, offset, slide),
                            ItemPilePiece { offset },
                            NoFrustumCulling,
                        ))
                        .id();
                    commands.entity(entity).add_child(child);
                }
            }
            commands.entity(entity).insert(ItemVisual {
                stack: dropped.0,
                fancy,
                cube,
            });
            continue;
        }
        let Some(children) = children else {
            continue;
        };
        for child in children.iter() {
            let Ok((piece, mut piece_transform)) = pieces.get_mut(child) else {
                continue;
            };
            *piece_transform = item_piece_transform(bob, yaw, scale, piece.offset, slide);
        }
    }
}

fn visual_ready(
    cube: bool,
    terrain: Option<&TerrainMaterial>,
    icons: Option<&BlockIcons>,
    stack: ItemStack,
) -> bool {
    if cube {
        terrain.is_some()
    } else {
        icons.is_some_and(|icons| icons.ready() && icons.uv_for_stack(stack).is_some())
    }
}

fn climate_tints(
    chunks: &WorldChunks,
    grass: Option<&GrassColors>,
    foliage: Option<&FoliageColors>,
    position: Vec3,
) -> ([f32; 3], [f32; 3]) {
    let climate = chunks
        .get(ChunkPos::from_block(
            position.x.floor() as i32,
            position.z.floor() as i32,
        ))
        .map(|chunk| {
            chunk.biomes.get(
                (position.x.floor() as i32).rem_euclid(CHUNK_SIZE as i32) as usize,
                (position.z.floor() as i32).rem_euclid(CHUNK_SIZE as i32) as usize,
            )
        });
    let grass_tint = climate
        .and_then(|climate| grass.map(|colors| colors.sample(climate)))
        .unwrap_or([0.55, 0.8, 0.4]);
    let foliage_tint = climate
        .and_then(|climate| foliage.map(|colors| colors.sample(climate)))
        .unwrap_or([0.28, 0.71, 0.09]);
    (grass_tint, foliage_tint)
}

fn spawn_block_pieces(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    block: BlockId,
    fancy: bool,
    tints: ([f32; 3], [f32; 3]),
    terrain: Option<&TerrainMaterial>,
    grass_overlay: Option<&GrassOverlayMaterial>,
    cutout: Option<&CutoutMaterial>,
    offsets: &[Vec3],
    bob: f32,
    yaw: f32,
    scale: f32,
    slide: Vec3,
) {
    let Some(terrain) = terrain else {
        return;
    };
    let built = dropped_block_meshes(block, fancy, tints.0, tints.1);
    let body = meshes.add(built.body);
    let overlay = built.overlay.map(|mesh| meshes.add(mesh));
    let use_cutout = built.cutout;
    let cutout_handle = cutout.map(|material| material.0.clone());
    for offset in offsets {
        let pose = item_piece_transform(bob, yaw, scale, *offset, slide);
        if use_cutout && let Some(material) = cutout_handle.clone() {
            let child = commands
                .spawn((
                    Mesh3d(body.clone()),
                    MeshMaterial3d(material),
                    pose,
                    ItemPilePiece { offset: *offset },
                    NoFrustumCulling,
                ))
                .id();
            commands.entity(parent).add_child(child);
        } else {
            let child = commands
                .spawn((
                    Mesh3d(body.clone()),
                    MeshMaterial3d(terrain.0.clone()),
                    pose,
                    ItemPilePiece { offset: *offset },
                    NoFrustumCulling,
                ))
                .id();
            commands.entity(parent).add_child(child);
        }
        if let Some(overlay) = overlay.clone()
            && let Some(material) = grass_overlay
        {
            let child = commands
                .spawn((
                    Mesh3d(overlay),
                    MeshMaterial3d(material.0.clone()),
                    pose,
                    ItemPilePiece { offset: *offset },
                    NoFrustumCulling,
                ))
                .id();
            commands.entity(parent).add_child(child);
        }
    }
}

fn item_quad(u0: f32, v0: f32, u1: f32, v1: f32, meshes: &mut Assets<Mesh>) -> Mesh3d {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.25, 0.0],
            [0.5, -0.25, 0.0],
            [0.5, 0.75, 0.0],
            [-0.5, 0.75, 0.0],
        ],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
    );
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    Mesh3d(meshes.add(mesh))
}

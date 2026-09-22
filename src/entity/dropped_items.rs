//! World item entities: physics, pickup, and Beta-style item rendering.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::entity::CollisionState;
use crate::entity::DroppedItem;
use crate::entity::EntitySize;
use crate::entity::Gravity;
use crate::entity::StepHeight;
use crate::entity::Velocity;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemStack;
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::ui::block_icons::BlockIcons;
use crate::world::block::block::BlockId;

const PICKUP_DELAY: f32 = 0.5;
const ITEM_LIFETIME: f32 = 300.0;
const ITEM_GRAVITY: f32 = 24.0;
const ITEM_BOB_SPEED: f32 = 2.0;
const ITEM_BOB_HEIGHT: f32 = 0.04;

pub struct DroppedItemPlugin;

impl Plugin for DroppedItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (update_item_state, pickup_dropped_items, sync_item_rendering)
                .chain()
                .after(PhysicsSet::Integrate)
                .run_if(in_state(AppScreen::Playing)),
        );
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct DroppedItemState {
    pub pickup_delay: f32,
    pub age: f32,
    pub bob_phase: f32,
    pub spin: f32,
}

impl DroppedItemState {
    pub fn new(pickup_delay: f32, phase: f32) -> Self {
        Self {
            pickup_delay,
            age: 0.0,
            bob_phase: phase,
            spin: 0.0,
        }
    }
}

#[derive(Component)]
struct ItemDropVisual {
    stack: ItemStack,
}

#[derive(Resource)]
struct ItemDropMaterial(Handle<StandardMaterial>);

/// Spawn an item entity with Beta-like launch motion.
pub fn spawn_dropped_item(
    commands: &mut Commands,
    position: Vec3,
    stack: ItemStack,
    velocity: Vec3,
) {
    commands.spawn((
        Name::new("Dropped item"),
        DroppedItem(stack),
        DroppedItemState::new(PICKUP_DELAY, position.x * 0.37 + position.z * 0.19),
        Transform::from_translation(position),
        Velocity(velocity),
        CollisionState::default(),
        Gravity(ITEM_GRAVITY),
        EntitySize::DROPPED_ITEM,
        StepHeight::default(),
    ));
}

/// Return the currently implemented inventory drop for a broken block.
pub fn block_drop(block: BlockId) -> Option<ItemStack> {
    let block = (block == BlockId::Stone)
        .then_some(BlockId::Cobblestone)
        .unwrap_or(block);
    ItemStack::from_block(block, 1).ok()
}

fn update_item_state(
    time: Res<Time>,
    mut commands: Commands,
    mut items: Query<(Entity, &mut DroppedItemState, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut state, mut transform) in &mut items {
        state.age += dt;
        state.pickup_delay = (state.pickup_delay - dt).max(0.0);
        state.spin = (state.spin + dt * 2.5) % std::f32::consts::TAU;
        let previous_bob =
            (state.bob_phase + (state.age - dt) * ITEM_BOB_SPEED).sin() * ITEM_BOB_HEIGHT;
        let current_bob = (state.bob_phase + state.age * ITEM_BOB_SPEED).sin() * ITEM_BOB_HEIGHT;
        transform.translation.y += current_bob - previous_bob;
        if state.age >= ITEM_LIFETIME {
            commands.entity(entity).despawn();
        }
    }
}

fn pickup_dropped_items(
    mut commands: Commands,
    mut player: Query<(&Transform, &mut Hotbar, &mut Inventory), With<Player>>,
    mut items: Query<(Entity, &Transform, &DroppedItemState, &mut DroppedItem)>,
) {
    let Ok((player_transform, mut hotbar, mut inventory)) = player.single_mut() else {
        return;
    };
    for (entity, transform, state, mut dropped) in &mut items {
        if state.pickup_delay > 0.0
            || transform
                .translation
                .distance_squared(player_transform.translation)
                > 2.25
        {
            continue;
        }
        if let Some(remainder) = inventory.insert(&mut hotbar, dropped.0) {
            dropped.0 = remainder;
        } else {
            commands.entity(entity).despawn();
        }
    }
}

fn sync_item_rendering(
    mut commands: Commands,
    icons: Option<Res<BlockIcons>>,
    camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material: Option<Res<ItemDropMaterial>>,
    mut items: Query<(
        Entity,
        &DroppedItem,
        &DroppedItemState,
        &mut Transform,
        Option<&ItemDropVisual>,
    )>,
) {
    let Some(icons) = icons.filter(|icons| icons.ready()) else {
        return;
    };
    let material = material.map_or_else(
        || {
            let handle = materials.add(StandardMaterial {
                base_color_texture: Some(icons.image.clone()),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            commands.insert_resource(ItemDropMaterial(handle.clone()));
            handle
        },
        |material| material.0.clone(),
    );
    let camera_rotation = camera
        .single()
        .map_or(Quat::IDENTITY, GlobalTransform::rotation);
    for (entity, dropped, state, mut transform, visual) in &mut items {
        transform.rotation = camera_rotation * Quat::from_rotation_z(state.spin);
        let count_scale = match dropped.0.count() {
            1 => 1.0,
            2..=16 => 1.05,
            17..=32 => 1.1,
            _ => 1.15,
        };
        transform.scale = Vec3::splat(0.9 * count_scale);
        if visual.is_some_and(|visual| visual.stack == dropped.0) {
            continue;
        }
        let Some((u0, v0, u1, v1)) = icons.uv_for_stack(dropped.0) else {
            continue;
        };
        let mut mesh = Mesh::from(Rectangle::new(0.32, 0.32));
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            vec![[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        );
        let mesh = meshes.add(mesh);
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            ItemDropVisual { stack: dropped.0 },
            NoFrustumCulling,
        ));
    }
}

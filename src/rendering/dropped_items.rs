//! `RenderItem`: drawing dropped items.
//!
//! Full cubes use the world block mesh and spin about Y. Everything else is an
//! upright sprite that yaws to face the camera and does not spin.

use bevy::camera::visibility::NoFrustumCulling;
use bevy::ecs::system::SystemParam;
use bevy::mesh::Indices;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::block::blocks::Block;
use crate::entity::DroppedItem;
use crate::entity::PreviousTick;
use crate::entity::drops::items::DroppedItemState;
use crate::entity::drops::items::PickupAnimation;
use crate::entity::drops::items::pickup_dropped_items;
use crate::entity::drops::items::pickup_position;
use crate::item::ItemStack;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::player::rendered_eye;
use crate::random::JavaRandom;
use crate::rendering::appearance::Shape;
use crate::rendering::appearance::block_appearance;
use crate::rendering::icons::BlockIcons;
use crate::rendering::meshing::dropped_block_meshes;
use crate::rendering::textures::AlphaMaskMaterial;
use crate::rendering::textures::CutoutMaterial;
use crate::rendering::textures::FoliageColors;
use crate::rendering::textures::GrassColors;
use crate::rendering::textures::GrassOverlayMaterial;
use crate::rendering::textures::TerrainMaterial;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::tick::WorldTick;

const CUBE_SCALE: f32 = 0.25;
const SPRITE_SCALE: f32 = 0.5;
/// `RenderItem` reseeds `java.util.Random` to this value every frame.
const PILE_SEED: u64 = 187;

/// Draws what `DroppedItemPlugin` simulates.
pub struct DroppedItemRenderPlugin;

impl Plugin for DroppedItemRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_item_rendering
                .after(pickup_dropped_items)
                .run_if(in_state(AppScreen::Playing)),
        );
    }
}

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

/// A block item's meshes and which material its body takes.
#[derive(Clone)]
struct BlockPieces {
    body: Handle<Mesh>,
    overlay: Option<Handle<Mesh>>,
    cutout: bool,
    alpha_masked: bool,
}

/// Meshes shared by every drop that looks the same: a sprite quad per icon,
/// and a block mesh per block state, leaf style and climate tint. Emptied
/// once no item is left in the world.
#[derive(Default)]
struct ItemMeshes {
    quads: HashMap<[u32; 4], Handle<Mesh>>,
    blocks: HashMap<(Block, u8, bool, [u32; 3], [u32; 3]), BlockPieces>,
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

/// Blocks drawn as a 3D model. `RenderBlocks.renderItemIn3d` accepts only a few
/// render types; torches, ladders, plants, repeaters, levers, and the rest use
/// Beta's flat item sprite instead (`Shape::Flat`).
pub fn dropped_block_model(stack: ItemStack) -> Option<(Block, u8)> {
    let (block, metadata) = stack.runtime_block()?;
    if block.is_ladder()
        || block.is_torch()
        || block.is_crossed_plant()
        || block == Block::Cobweb
        || block_appearance(block.as_u8(), u16::from(metadata)).shape == Shape::Flat
    {
        None
    } else {
        Some((block, metadata))
    }
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
    alpha_mask: Option<Res<'w, AlphaMaskMaterial>>,
    icons: Option<Res<'w, BlockIcons>>,
}

fn sync_item_rendering(
    mut commands: Commands,
    tick: Res<WorldTick>,
    world: ItemRenderResources,
    camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    player: Query<
        (&Transform, Option<&PlayerInterpolation>),
        (With<Player>, Without<DroppedItem>, Without<ItemPilePiece>),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    icon_material: Option<Res<ItemDropMaterial>>,
    mut items: Query<(
        Entity,
        &mut Transform,
        &DroppedItem,
        &DroppedItemState,
        Option<&PreviousTick>,
        Option<&ItemVisual>,
        Option<&Children>,
        Option<&PickupAnimation>,
    )>,
    mut pieces: Query<(&ItemPilePiece, &mut Transform), Without<DroppedItem>>,
    mut cache: Local<ItemMeshes>,
) {
    if items.is_empty() {
        *cache = ItemMeshes::default();
    }
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

    // The eyes as drawn this frame. The simulated position only moves on a
    // tick, and a flight aimed at it would step 20 times a second against
    // the smoothly moving view.
    let player_eye = player
        .single()
        .ok()
        .map(|(at, interpolation)| rendered_eye(at, interpolation, tick.partial()));
    for (entity, mut transform, dropped, state, previous_tick, visual, children, pickup) in
        &mut items
    {
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
        // Pickup flight is already continuous: an item taken whole still
        // carries the `PreviousTick` of where it lay, which must not pull it
        // back toward the ground at the start of every tick.
        let slide = match previous_tick {
            Some(previous_tick) if pickup.is_none() => {
                interpolated_item_position(previous_tick.0, transform.translation, tick.partial())
                    - transform.translation
            }
            _ => Vec3::ZERO,
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
            if let Some((block, metadata)) = model {
                spawn_block_pieces(
                    &mut commands,
                    &mut meshes,
                    &mut cache,
                    entity,
                    block,
                    metadata,
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
                    world.alpha_mask.as_deref(),
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
                            item_quad(u0, v0, u1, v1, &mut meshes, &mut cache),
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
        .get(ChunkPosition::from_block(
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
    cache: &mut ItemMeshes,
    parent: Entity,
    block: Block,
    metadata: u8,
    fancy: bool,
    tints: ([f32; 3], [f32; 3]),
    terrain: Option<&TerrainMaterial>,
    grass_overlay: Option<&GrassOverlayMaterial>,
    cutout: Option<&CutoutMaterial>,
    alpha_mask: Option<&AlphaMaskMaterial>,
    offsets: &[Vec3],
    bob: f32,
    yaw: f32,
    scale: f32,
    slide: Vec3,
) {
    let Some(terrain) = terrain else {
        return;
    };
    let key = (
        block,
        metadata,
        fancy,
        tints.0.map(f32::to_bits),
        tints.1.map(f32::to_bits),
    );
    let built = cache
        .blocks
        .entry(key)
        .or_insert_with(|| {
            let built = dropped_block_meshes(block, metadata, fancy, tints.0, tints.1);
            BlockPieces {
                cutout: built.cutout,
                alpha_masked: built.alpha_masked,
                overlay: built.overlay.map(|mesh| meshes.add(mesh.into_mesh())),
                body: meshes.add(built.body.into_mesh()),
            }
        })
        .clone();
    let body = built.body;
    let overlay = built.overlay;
    let use_cutout = built.cutout;
    let cutout_handle = cutout.map(|material| material.0.clone());
    let alpha_mask_handle = alpha_mask.map(|material| material.0.clone());
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
        } else if built.alpha_masked
            && let Some(material) = alpha_mask_handle.clone()
        {
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

fn item_quad(
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    meshes: &mut Assets<Mesh>,
    cache: &mut ItemMeshes,
) -> Mesh3d {
    let key = [u0, v0, u1, v1].map(f32::to_bits);
    if let Some(mesh) = cache.quads.get(&key) {
        return Mesh3d(mesh.clone());
    }
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
    let mesh = meshes.add(mesh);
    cache.quads.insert(key, mesh.clone());
    Mesh3d(mesh)
}

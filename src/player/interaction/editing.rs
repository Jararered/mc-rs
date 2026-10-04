//! Break and place blocks along the camera ray.
//!
//! Selected hotbar blocks can be placed. Mining speed, drops, and tool wear
//! follow the held stack.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::block::blocks::Block;
use crate::block::direction::HorizontalFacing;
use crate::block::fluids::FluidType;
use crate::block::fluids::is_water;
use crate::block::properties::cactus_can_stay;
use crate::block::properties::sugar_cane_can_stay;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::combat::bordered;
use crate::entity::combat::pick;
use crate::entity::drops::blocks::natural_drops_with_metadata;
use crate::entity::drops::blocks::player_break_drops_with_metadata;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::drops::items::spawn_thrown_item;
use crate::entity::particles::block::BlockParticles;
use crate::entity::projectiles::FIREBALL_SIZE;
use crate::entity::projectiles::Fireball;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::session::close_crafting_session;
use crate::item::Item;
use crate::item::ItemStack;
use crate::item::tools::break_durability;
use crate::item::tools::can_harvest;
use crate::item::tools::is_hoe;
use crate::physics::Aabb;
use crate::physics::BLOCK_REACH;
use crate::physics::BlockFace;
use crate::physics::BlockHit;
use crate::physics::block_hit_distance;
use crate::physics::raycast_blocks;
use crate::physics::raycast_blocks_or_liquid;
use crate::player::interaction::attack::ENTITY_REACH;
use crate::player::interaction::attack::FIREBALL_BORDER;
use crate::player::interaction::attack::MOB_BORDER;
use crate::player::interaction::attack::MobTarget;
use crate::player::interaction::attack::attack;
use crate::player::interaction::attack::interact;
use crate::random::ItemRng;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::behaviors::leaves::CHECK_DECAY;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

use super::mining::MiningState;
use super::overlay::BlockFocus;
use crate::player::Player;
use crate::player::PlayerCamera;

/// Held-button place repeat, matching Beta's `ticksPerSecond / 4`.
const PLACE_DELAY_TICKS: i32 = 5;

/// Torch used by the standalone placement helper and legacy tests.
pub const PLACED_BLOCK: Block = Block::Torch;

#[derive(Default)]
pub(crate) struct BlockInteractState {
    place_delay: i32,
    mining: MiningState,
}

/// What the crosshair rests on.
#[derive(Clone, Copy)]
enum Pointed {
    Mob(Entity),
    Fireball(Entity),
}

pub(crate) fn interact_blocks(
    mut commands: Commands,
    tick: Res<WorldTick>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut player: Query<
        (
            &Transform,
            &EntitySize,
            &CollisionState,
            &mut Hotbar,
            &mut Inventory,
            &Velocity,
        ),
        With<Player>,
    >,
    camera: Query<&Transform, With<PlayerCamera>>,
    mut chunks: ResMut<WorldChunks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    (mut persistence, mut block_ticks): (
        Option<ResMut<WorldPersistence>>,
        Option<ResMut<BlockTicks>>,
    ),
    (mut particles, mut mobs, mut fireballs): (
        Option<ResMut<BlockParticles>>,
        Query<MobTarget, Without<Player>>,
        Query<(Entity, &mut Fireball, &Transform), Without<Player>>,
    ),
    mut focus: ResMut<BlockFocus>,
    mut state: Local<BlockInteractState>,
    mut inventory_screen: ResMut<InventorySession>,
    mut workbench: ResMut<ActiveWorkbench>,
    mut item_rng: Local<ItemRng>,
) {
    let ticks = tick.ticks_this_frame();
    for _ in 0..ticks {
        if state.place_delay > 0 {
            state.place_delay -= 1;
        }
    }

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }

    let Ok((transform, size, collision, mut hotbar, mut inventory, velocity)) = player.single_mut()
    else {
        *focus = BlockFocus::default();
        return;
    };

    if locked
        && !inventory_screen.open
        && keys.just_pressed(KeyCode::KeyQ)
        && let Some(stack) = hotbar.take_selected(1)
    {
        spawn_thrown_item(
            &mut commands,
            &mut item_rng,
            transform,
            *transform.forward(),
            stack,
        );
        if let Some(persistence) = persistence.as_deref_mut() {
            persistence.mark_dirty(ChunkPosition::from_block(
                transform.translation.x.floor() as i32,
                transform.translation.z.floor() as i32,
            ));
        }
    }

    let left_click = mouse.just_pressed(MouseButton::Left);
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left);
    let right_held = mouse.pressed(MouseButton::Right);

    if !left_held {
        state.mining.reset();
    }

    let camera_transform = camera.single().ok();
    let view_rotation = transform.rotation
        * camera_transform
            .map(|camera| camera.rotation)
            .unwrap_or(Quat::IDENTITY);
    let view_origin = transform.translation
        + transform.rotation
            * camera_transform
                .map(|camera| camera.translation)
                .unwrap_or(Vec3::ZERO);
    let hit = raycast_blocks(
        &chunks,
        view_origin,
        view_rotation * Vec3::NEG_Z,
        BLOCK_REACH,
    );
    if !inventory_screen.open && (left_click || right_click) {
        let look = view_rotation * Vec3::NEG_Z;
        // `getMouseOver`: an entity counts only nearer than the block in view.
        let reach = hit.map_or(ENTITY_REACH, |hit| {
            block_hit_distance(&chunks, &hit, view_origin, look).min(ENTITY_REACH)
        });
        let pointed = pick(
            view_origin,
            look,
            reach,
            mobs.iter()
                .map(|mob| {
                    let aabb = mob.size.aabb(mob.transform.translation);
                    (Pointed::Mob(mob.entity), bordered(aabb, MOB_BORDER))
                })
                .chain(fireballs.iter().map(|(entity, _, transform)| {
                    let aabb = FIREBALL_SIZE.aabb(transform.translation);
                    (Pointed::Fireball(entity), bordered(aabb, FIREBALL_BORDER))
                })),
        );
        if let Some(pointed) = pointed {
            match pointed {
                Pointed::Mob(target) if left_click => attack(
                    &mut commands,
                    &mut item_rng,
                    &mut mobs,
                    target,
                    transform.translation,
                    velocity.0.y < 0.0,
                    &mut hotbar,
                ),
                Pointed::Mob(target) => interact(
                    &mut commands,
                    &mut item_rng,
                    &mut mobs,
                    target,
                    &mut hotbar,
                    &mut inventory,
                ),
                Pointed::Fireball(target) => {
                    if left_click && let Ok((_, mut fireball, _)) = fireballs.get_mut(target) {
                        fireball.deflect(look);
                    }
                }
            }
            state.mining.reset();
            *focus = BlockFocus::default();
            return;
        }
    }
    if right_click && !inventory_screen.open && hit.is_some_and(|hit| hit.block.is_furnace()) {
        let hit = hit.expect("checked above");
        close_crafting_session(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
        inventory_screen.open = true;
        inventory_screen.workbench = false;
        inventory_screen.furnace = true;
        inventory_screen.furnace_position = Some((hit.x, hit.y, hit.z));
        inventory_screen.chest = false;
        inventory_screen.chest_position = None;
        inventory_screen.chest_group = None;
        if let Ok((_, mut cursor)) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    if right_click && !inventory_screen.open && hit.is_some_and(|hit| hit.block.is_chest()) {
        let hit = hit.expect("checked above");
        let Some(group) = chunks.chest_group_at(hit.x, hit.y, hit.z) else {
            state.mining.reset();
            *focus = BlockFocus::default();
            return;
        };
        let blocked = [Some(group.first), group.second]
            .into_iter()
            .flatten()
            .any(|(x, y, z)| {
                chunks
                    .block_at(x, y + 1, z)
                    .is_some_and(Block::is_opaque_cube)
            });
        if blocked {
            state.mining.reset();
            *focus = BlockFocus::default();
            return;
        }
        close_crafting_session(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
        inventory_screen.open = true;
        inventory_screen.workbench = false;
        inventory_screen.furnace = false;
        inventory_screen.furnace_position = None;
        inventory_screen.chest = true;
        inventory_screen.chest_position = Some((hit.x, hit.y, hit.z));
        inventory_screen.chest_group = Some(group);
        if let Ok((_, mut cursor)) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    if right_click
        && !inventory_screen.open
        && hit.is_some_and(|hit| hit.block == Block::CraftingTable)
    {
        let hit = hit.expect("checked above");
        // Do not let stale player-grid contents leak into a new workbench
        // session if an earlier interface was interrupted before its close
        // system ran.
        close_crafting_session(
            &mut commands,
            transform,
            &mut item_rng,
            &mut hotbar,
            &mut inventory,
            &mut workbench,
        );
        inventory_screen.open = true;
        inventory_screen.workbench = true;
        inventory_screen.furnace = false;
        inventory_screen.furnace_position = None;
        inventory_screen.chest = false;
        inventory_screen.chest_position = None;
        inventory_screen.chest_group = None;
        workbench.position = Some((hit.x, hit.y, hit.z));
        if let Ok((_, mut cursor)) = windows.single_mut() {
            cursor.visible = true;
            cursor.grab_mode = CursorGrabMode::None;
        }
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }
    let in_water = chunks
        .block_at(
            transform.translation.x.floor() as i32,
            transform.translation.y.floor() as i32,
            transform.translation.z.floor() as i32,
        )
        .is_some_and(is_water);
    let on_ground = collision.on_ground;

    if left_held {
        if left_click && let Some(hit) = hit {
            // `Block.onBlockClicked`, when the player starts to dig.
            push_event(
                &mut block_ticks,
                BlockEvent::Clicked {
                    position: IVec3::new(hit.x, hit.y, hit.z),
                },
            );
            let tool = hotbar.selected_stack();
            if let Some(broken) = state.mining.try_instant(hit, tool, on_ground, in_water) {
                apply_break(
                    &mut commands,
                    &mut item_rng,
                    &mut chunks,
                    &mut streaming,
                    &mut persistence,
                    &mut block_ticks,
                    &mut particles,
                    &mut hotbar,
                    broken,
                );
            }
        }
        for _ in 0..ticks {
            let old_damage = state.mining.damage();
            let tool = hotbar.selected_stack();
            if let Some(broken) = state.mining.tick(hit, tool, on_ground, in_water) {
                apply_break(
                    &mut commands,
                    &mut item_rng,
                    &mut chunks,
                    &mut streaming,
                    &mut persistence,
                    &mut block_ticks,
                    &mut particles,
                    &mut hotbar,
                    broken,
                );
            } else if state.mining.damage() > old_damage
                && let Some(hit) = hit
                && let Some(particles) = particles.as_deref_mut()
            {
                particles.emit_hit(hit);
            }
        }
    }

    let can_place = right_click || (right_held && state.place_delay <= 0 && !left_held);
    if can_place {
        state.place_delay = PLACE_DELAY_TICKS;
        if let Some(hit) = hit {
            // `Block.blockActivated` runs before the held item is used.
            push_event(
                &mut block_ticks,
                BlockEvent::Activated {
                    position: IVec3::new(hit.x, hit.y, hit.z),
                },
            );
        }
        if hotbar
            .selected_stack()
            .is_some_and(|stack| stack.item() == Item::Bucket)
            && let Some((x, y, z, previous, fluid)) =
                pick_up_fluid(&mut chunks, view_origin, view_rotation * Vec3::NEG_Z)
        {
            let filled = match fluid {
                FluidType::Water => Item::WaterBucket,
                FluidType::Lava => Item::LavaBucket,
            };
            let selected = hotbar.selected;
            hotbar.slots[selected] = ItemStack::new(filled, 1).ok();
            push_event(
                &mut block_ticks,
                BlockEvent::Changed {
                    position: IVec3::new(x, y, z),
                    previous,
                    metadata: 0,
                },
            );
            notify_edit(&mut streaming, &mut persistence, x, y, z, false);
        } else if let Some(hit) = hit
            && let Some(stack) = hotbar.selected_stack()
        {
            let target = hit.face.neighbor(hit.x, hit.y, hit.z);
            let replaced = chunks.block_at(target.0, target.1, target.2);
            let replaced_metadata = chunks.metadata_at(target.0, target.1, target.2);
            let placed = stack.runtime_block().is_some_and(|(block, metadata)| {
                place_selected_block_facing(
                    &mut chunks,
                    hit,
                    size.aabb(transform.translation),
                    block,
                    metadata,
                    furnace_facing_toward_player(transform.rotation * Vec3::NEG_Z),
                )
            });
            if placed {
                let selected = hotbar.selected;
                hotbar.slots[selected] =
                    ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
                let (x, y, z) = target;
                // `ItemLeaves.getPlacedBlockMetadata`: placed leaves check
                // for a log on their next random tick.
                if chunks.block_at(x, y, z).is_some_and(Block::is_leaves) {
                    let species = chunks.metadata_at(x, y, z);
                    chunks.set_metadata(x, y, z, species | CHECK_DECAY);
                }
                if let Some(replaced) = replaced {
                    push_event(
                        &mut block_ticks,
                        BlockEvent::Changed {
                            position: IVec3::new(x, y, z),
                            previous: replaced,
                            metadata: replaced_metadata,
                        },
                    );
                }
                notify_edit(&mut streaming, &mut persistence, x, y, z, true);
            } else if stack.item() == Item::Seeds && plant_seeds(&mut chunks, hit) {
                let selected = hotbar.selected;
                hotbar.slots[selected] =
                    ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
                push_event(
                    &mut block_ticks,
                    BlockEvent::Changed {
                        position: IVec3::new(hit.x, hit.y + 1, hit.z),
                        previous: Block::Air,
                        metadata: 0,
                    },
                );
                notify_edit(
                    &mut streaming,
                    &mut persistence,
                    hit.x,
                    hit.y + 1,
                    hit.z,
                    false,
                );
            } else if till_with_selected_hoe(&mut chunks, &mut hotbar, hit) {
                push_event(
                    &mut block_ticks,
                    BlockEvent::Changed {
                        position: IVec3::new(hit.x, hit.y, hit.z),
                        previous: hit.block,
                        metadata: 0,
                    },
                );
                notify_edit(&mut streaming, &mut persistence, hit.x, hit.y, hit.z, false);
            } else if let Some(fluid) = match stack.item() {
                Item::WaterBucket => Some(FluidType::Water),
                Item::LavaBucket => Some(FluidType::Lava),
                _ => None,
            } && let Some((x, y, z, previous, previous_metadata)) =
                place_fluid(&mut chunks, hit, fluid)
            {
                let selected = hotbar.selected;
                hotbar.slots[selected] = ItemStack::new(Item::Bucket, 1).ok();
                push_event(
                    &mut block_ticks,
                    BlockEvent::Changed {
                        position: IVec3::new(x, y, z),
                        previous,
                        metadata: previous_metadata,
                    },
                );
                notify_edit(&mut streaming, &mut persistence, x, y, z, false);
            }
        }
    }

    focus.hit = hit;
    focus.mining_damage = state.mining.damage();
}

pub fn till_with_selected_hoe(
    chunks: &mut WorldChunks,
    hotbar: &mut Hotbar,
    hit: BlockHit,
) -> bool {
    if !hotbar
        .selected_stack()
        .is_some_and(|stack| is_hoe(stack.item()))
    {
        return false;
    }
    if !till_block(chunks, hit) {
        return false;
    }
    hotbar.damage_selected(1);
    true
}

/// `ItemSeeds.onItemUse`: plant crops on the top face of farmland with air
/// above.
pub fn plant_seeds(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if hit.face != BlockFace::Up
        || chunks.block_at(hit.x, hit.y, hit.z) != Some(Block::Farmland)
        || chunks.block_at(hit.x, hit.y + 1, hit.z) != Some(Block::Air)
    {
        return false;
    }
    chunks
        .set_block(hit.x, hit.y + 1, hit.z, Block::Crops)
        .is_some()
}

pub fn till_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if chunks.block_at(hit.x, hit.y, hit.z) != Some(hit.block) {
        return false;
    }
    let can_till = match hit.block {
        Block::Dirt => true,
        Block::Grass => {
            hit.face != BlockFace::Down
                && chunks
                    .block_at(hit.x, hit.y + 1, hit.z)
                    .is_none_or(|block| block == Block::Air)
        }
        _ => false,
    };
    if !can_till {
        return false;
    }
    chunks
        .set_block(hit.x, hit.y, hit.z, Block::Farmland)
        .is_some_and(|previous| previous == hit.block)
}

/// `ItemBucket.onItemRightClick` when empty: pick up the water or lava
/// source the camera ray hits first. Unlike the normal block pick, this
/// raycast also stops on fluid so it can target one at all. Flowing
/// (non-source) fluid, a solid block, or nothing in reach leaves the bucket
/// empty, matching Beta's `getBlockMetadata(...) == 0` gate.
pub fn pick_up_fluid(
    chunks: &mut WorldChunks,
    origin: Vec3,
    direction: Vec3,
) -> Option<(i32, i32, i32, Block, FluidType)> {
    let hit = raycast_blocks_or_liquid(chunks, origin, direction, BLOCK_REACH)?;
    let fluid = FluidType::of(hit.block)?;
    if chunks.metadata_at(hit.x, hit.y, hit.z) != 0 {
        return None;
    }
    let previous = chunks.set_block(hit.x, hit.y, hit.z, Block::Air)?;
    Some((hit.x, hit.y, hit.z, previous, fluid))
}

/// `ItemBucket.onItemRightClick` when full: empty the held fluid into the
/// non-solid cell beside the hit face, the same target a torch would attach
/// to but without requiring a solid block behind it. Use the flowing block
/// value so `onBlockAdded` schedules its first spread tick.
pub fn place_fluid(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    fluid: FluidType,
) -> Option<(i32, i32, i32, Block, u8)> {
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return None;
    }
    let current = chunks.block_at(x, y, z)?;
    if current.is_solid_material() {
        return None;
    }
    let metadata = chunks.metadata_at(x, y, z);
    let previous = chunks.set_block(x, y, z, fluid.flowing())?;
    Some((x, y, z, previous, metadata))
}

/// Queue a block event for the next tick pass.
fn push_event(ticks: &mut Option<ResMut<BlockTicks>>, event: BlockEvent) {
    if let Some(ticks) = ticks.as_deref_mut() {
        ticks.push_event(event);
    }
}

fn apply_break(
    commands: &mut Commands,
    rng: &mut ItemRng,
    chunks: &mut WorldChunks,
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    ticks: &mut Option<ResMut<BlockTicks>>,
    particles: &mut Option<ResMut<BlockParticles>>,
    hotbar: &mut Hotbar,
    hit: BlockHit,
) {
    let metadata = chunks.metadata_at(hit.x, hit.y, hit.z);
    let mut attached = attached_blocks(chunks, hit);
    if let Some(plant) = chunks
        .block_at(hit.x, hit.y + 1, hit.z)
        .filter(|block| (*block).is_crossed_plant())
    {
        let plant_metadata = chunks.metadata_at(hit.x, hit.y + 1, hit.z);
        attached.push((hit.x, hit.y + 1, hit.z, plant, plant_metadata));
    }
    let furnace_drops = chunks
        .furnace_at(hit.x, hit.y, hit.z)
        .map(|furnace| furnace.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let chest_drops = chunks
        .chest_at(hit.x, hit.y, hit.z)
        .map(|chest| chest.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let light_edit = hit.block.is_torch()
        || hit.block == Block::LitFurnace
        || attached
            .iter()
            .any(|&(_, _, _, attached_block, _)| attached_block.is_torch());
    let tool = hotbar.selected_stack();
    // `canHarvestBlock` gates the harvest drop. The tool still takes durability
    // when the block comes out, including a block the tool cannot harvest.
    // TNT's player-destroy drop is not part of that gate.
    if break_block(chunks, hit) {
        for stack in furnace_drops {
            spawn_block_drop(commands, rng, IVec3::new(hit.x, hit.y, hit.z), stack);
        }
        spawn_chest_drops(commands, rng, IVec3::new(hit.x, hit.y, hit.z), chest_drops);
        for stack in player_break_drops_with_metadata(hit.block, metadata, tool, rng) {
            spawn_block_drop(commands, rng, IVec3::new(hit.x, hit.y, hit.z), stack);
        }
        let position = IVec3::new(hit.x, hit.y, hit.z);
        push_event(
            ticks,
            BlockEvent::Changed {
                position,
                previous: hit.block,
                metadata,
            },
        );
        if can_harvest(tool, hit.block) {
            push_event(
                ticks,
                BlockEvent::Harvested {
                    position,
                    block: hit.block,
                    metadata,
                },
            );
        }
        for (x, y, z, attached_block, attached_metadata) in attached {
            for stack in natural_drops_with_metadata(attached_block, attached_metadata, rng) {
                spawn_block_drop(commands, rng, IVec3::new(x, y, z), stack);
            }
            push_event(
                ticks,
                BlockEvent::Changed {
                    position: IVec3::new(x, y, z),
                    previous: attached_block,
                    metadata: attached_metadata,
                },
            );
        }
        if let Some(particles) = particles.as_deref_mut() {
            particles.emit_break_state(hit, metadata);
        }
        notify_edit(streaming, persistence, hit.x, hit.y, hit.z, light_edit);
        if let Some(tool) = tool {
            let cost = break_durability(tool, hit.block);
            if cost > 0 {
                hotbar.damage_selected(cost);
            }
        }
    }
}

/// Torches and ladders hanging on the block `hit` targets, and a torch
/// standing on it, with their position and metadata.
fn attached_blocks(chunks: &WorldChunks, hit: BlockHit) -> Vec<(i32, i32, i32, Block, u8)> {
    // A torch on top has no facing; one beside the block hangs on its side.
    let mut attached = Vec::new();
    let mut push = |dx: i32, dy: i32, dz: i32, block: Block, facing: Option<HorizontalFacing>| {
        let (x, y, z) = (hit.x + dx, hit.y + dy, hit.z + dz);
        if chunks.block_at(x, y, z) != Some(block) {
            return;
        }
        let metadata = chunks.metadata_at(x, y, z);
        if block.facing(metadata) == facing {
            attached.push((x, y, z, block, metadata));
        }
    };
    push(0, 1, 0, Block::Torch, None);
    for side in HorizontalFacing::ALL {
        // The torch or ladder at `side` of the block names the block as its
        // support, which lies on the opposite side of the attached cell.
        let [dx, dy, dz] = side.offset();
        let facing = match side {
            HorizontalFacing::North => HorizontalFacing::South,
            HorizontalFacing::South => HorizontalFacing::North,
            HorizontalFacing::East => HorizontalFacing::West,
            HorizontalFacing::West => HorizontalFacing::East,
        };
        push(dx, dy, dz, Block::Torch, Some(facing));
        push(dx, dy, dz, Block::Ladder, Some(facing));
    }
    attached
}

/// Remove a targeted block. Bedrock and missing chunks are left unchanged.
pub fn break_block(chunks: &mut WorldChunks, hit: BlockHit) -> bool {
    if !hit.block.is_breakable() {
        return false;
    }
    let attached = attached_blocks(chunks, hit);
    let broken = chunks
        .set_block(hit.x, hit.y, hit.z, Block::Air)
        .is_some_and(|previous| previous != Block::Air);
    if broken {
        for (x, y, z, _, _) in attached {
            chunks.set_block(x, y, z, Block::Air);
        }
        if chunks
            .block_at(hit.x, hit.y + 1, hit.z)
            .is_some_and(Block::is_crossed_plant)
        {
            chunks.set_block(hit.x, hit.y + 1, hit.z, Block::Air);
        }
    }
    broken
}

/// Attach a torch to the hit face. Fails without a solid support block.
pub fn place_block(chunks: &mut WorldChunks, hit: BlockHit, player: Aabb) -> bool {
    place_selected_block(chunks, hit, player, Block::Torch)
}

pub fn place_selected_block(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: Block,
) -> bool {
    place_selected_block_facing(chunks, hit, player, selected, 0, HorizontalFacing::South)
}

pub fn place_selected_block_facing(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: Block,
    species: u8,
    front: HorizontalFacing,
) -> bool {
    let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
    if y < 0 || y >= CHUNK_HEIGHT as i32 {
        return false;
    }
    let Some(current) = chunks.block_at(x, y, z) else {
        return false;
    };
    if !current.is_replaceable() {
        return false;
    }
    if selected == Block::Chest && !chest_can_place_at(chunks, x, y, z) {
        return false;
    }
    if selected == Block::Ladder && ladder_facing(chunks, x, y, z, hit.face, hit.block).is_none() {
        return false;
    }
    if selected.is_crossed_plant()
        && selected != Block::SugarCane
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(Block::supports_plants)
    {
        return false;
    }
    if selected == Block::SugarCane {
        let Some(below) = chunks.block_at(x, y - 1, z) else {
            return false;
        };
        let adjacent_water = [
            chunks.block_at(x - 1, y - 1, z),
            chunks.block_at(x + 1, y - 1, z),
            chunks.block_at(x, y - 1, z - 1),
            chunks.block_at(x, y - 1, z + 1),
        ]
        .map(|block| matches!(block, Some(Block::Water | Block::FlowingWater)));
        if !sugar_cane_can_stay(below, adjacent_water) {
            return false;
        }
    }
    if selected == Block::Cactus {
        let Some(below) = chunks.block_at(x, y - 1, z) else {
            return false;
        };
        let [Some(west), Some(east), Some(north), Some(south)] = [
            chunks.block_at(x - 1, y, z),
            chunks.block_at(x + 1, y, z),
            chunks.block_at(x, y, z - 1),
            chunks.block_at(x, y, z + 1),
        ] else {
            return false;
        };
        if !cactus_can_stay(below, [west, east, north, south]) {
            return false;
        }
    }
    if selected == Block::Pumpkin
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(Block::is_opaque_cube)
    {
        return false;
    }
    if !hit.block.is_opaque_cube() && selected == Block::Torch {
        return false;
    }
    let (block, metadata) = match selected {
        Block::Torch => {
            let support = match hit.face {
                BlockFace::Up => None,
                BlockFace::Down => return false,
                BlockFace::West => Some(HorizontalFacing::East),
                BlockFace::East => Some(HorizontalFacing::West),
                BlockFace::North => Some(HorizontalFacing::South),
                BlockFace::South => Some(HorizontalFacing::North),
            };
            (
                selected,
                support.map_or(0, |facing| selected.facing_metadata(facing)),
            )
        }
        Block::Furnace | Block::Pumpkin | Block::Chest => {
            (selected, selected.facing_metadata(front))
        }
        Block::Ladder => {
            let Some(support) = ladder_facing(chunks, x, y, z, hit.face, hit.block) else {
                return false;
            };
            (selected, selected.facing_metadata(support))
        }
        _ => (selected, species),
    };
    if block.is_opaque_cube()
        && player.intersects(Aabb::new(
            Vec3::new(x as f32, y as f32, z as f32),
            Vec3::new(x as f32 + 1.0, y as f32 + 1.0, z as f32 + 1.0),
        ))
    {
        return false;
    }
    chunks
        .set_block_with_metadata(x, y, z, block, metadata)
        .is_some_and(|(previous, _)| previous != block)
}

fn ladder_facing(
    chunks: &WorldChunks,
    x: i32,
    y: i32,
    z: i32,
    hit_face: BlockFace,
    hit_block: Block,
) -> Option<HorizontalFacing> {
    let support_at = |facing: HorizontalFacing| {
        let [dx, dy, dz] = facing.offset();
        chunks
            .block_at(x + dx, y + dy, z + dz)
            .filter(|block| (*block).is_opaque_cube())
            .map(|_| facing)
    };

    let clicked_wall = match hit_face {
        BlockFace::West if hit_block.is_opaque_cube() => Some(HorizontalFacing::East),
        BlockFace::East if hit_block.is_opaque_cube() => Some(HorizontalFacing::West),
        BlockFace::North if hit_block.is_opaque_cube() => Some(HorizontalFacing::South),
        BlockFace::South if hit_block.is_opaque_cube() => Some(HorizontalFacing::North),
        _ => None,
    };
    clicked_wall
        .and_then(support_at)
        // Beta's onBlockPlaced fallback checks +Z, -Z, +X, -X.
        .or_else(|| support_at(HorizontalFacing::South))
        .or_else(|| support_at(HorizontalFacing::North))
        .or_else(|| support_at(HorizontalFacing::East))
        .or_else(|| support_at(HorizontalFacing::West))
}

fn chest_can_place_at(chunks: &WorldChunks, x: i32, y: i32, z: i32) -> bool {
    let neighbors = [(x - 1, y, z), (x + 1, y, z), (x, y, z - 1), (x, y, z + 1)];
    let chests = neighbors
        .into_iter()
        .filter(|&(nx, ny, nz)| chunks.block_at(nx, ny, nz).is_some_and(Block::is_chest))
        .collect::<Vec<_>>();
    match chests.as_slice() {
        [] => true,
        [neighbor] => chunks
            .chest_group_at(neighbor.0, neighbor.1, neighbor.2)
            .is_some_and(|group| !group.is_double()),
        _ => false,
    }
}

fn furnace_facing_toward_player(player_forward: Vec3) -> HorizontalFacing {
    let toward_player = Vec2::new(-player_forward.x, -player_forward.z);
    if toward_player.x.abs() > toward_player.y.abs() {
        if toward_player.x >= 0.0 {
            HorizontalFacing::East
        } else {
            HorizontalFacing::West
        }
    } else if toward_player.y >= 0.0 {
        HorizontalFacing::South
    } else {
        HorizontalFacing::North
    }
}

fn notify_edit(
    streaming: &mut Option<ResMut<WorldStreaming>>,
    persistence: &mut Option<ResMut<WorldPersistence>>,
    x: i32,
    y: i32,
    z: i32,
    light_edit: bool,
) {
    if let Some(persistence) = persistence.as_deref_mut() {
        persistence.mark_dirty(ChunkPosition::from_block(x, z));
        if light_edit {
            // A detached wall torch may belong to the neighboring chunk.
            for position in remesh_chunks_touching(x, z) {
                persistence.mark_dirty(position);
            }
        }
    }
    // Relighting decides which sections actually changed, so torches and
    // plain blocks take the same path.
    if let Some(streaming) = streaming.as_deref_mut() {
        streaming.request_block_update(x, y, z);
    }
}

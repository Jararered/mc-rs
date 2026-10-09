//! Break and place blocks along the camera ray.
//!
//! Selected hotbar blocks can be placed. Mining speed, drops, and tool wear
//! follow the held stack.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::block::blocks::Block;
use crate::block::direction::Direction;
use crate::block::fluids::Fluid;
use crate::block::fluids::is_water;
use crate::block::properties::cactus_can_stay;
use crate::block::properties::plant_ground_can_hold;
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
use crate::entity::minecart::CART_SIZE;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::CartKind;
use crate::entity::minecart::FUEL_PER_COAL;
use crate::entity::minecart::Minecart;
use crate::entity::minecart::break_cart;
use crate::entity::minecart::spawn_cart;
use crate::entity::mount::dismount;
use crate::entity::mount::mount;
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
use crate::item::tools::damage_vs_entity;
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
use crate::world::chunk::ChestGroup;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::OpenCart;
use crate::world::chunk::WorldChunks;
use crate::world::chunk::remesh_chunks_touching;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

use super::mining::MiningState;
use super::overlay::BlockFocus;
use crate::block::bed;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::player::PlayerHealth;
use crate::player::sleep::BedUse;
use crate::player::sleep::PlayerSleep;

/// Held-button place repeat, matching Beta's `ticksPerSecond / 4`.
const PLACE_DELAY_TICKS: i32 = 5;

/// Torch used by the standalone placement helper and legacy tests.
pub const PLACED_BLOCK: Block = Block::Torch;

#[derive(Default)]
pub(crate) struct BlockInteractState {
    place_delay: i32,
    mining: MiningState,
    /// Set while the cursor is free; the left button must be released once
    /// after the cursor is grabbed before it can mine or attack.
    wait_for_release: bool,
    /// Frame this system last ran; a gap means the world was not being played
    /// (menus, pause, chat), so the cursor grab and its click are fresh.
    last_frame: Option<u32>,
}

/// What the crosshair rests on.
#[derive(Clone, Copy)]
enum Pointed {
    Mob(Entity),
    Fireball(Entity),
    Minecart(Entity),
}

pub(crate) fn interact_blocks(
    mut commands: Commands,
    tick: Res<WorldTick>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut player: Query<
        (
            Entity,
            &Transform,
            &EntitySize,
            &CollisionState,
            &mut Hotbar,
            &mut Inventory,
            &Velocity,
            Option<&PlayerSleep>,
            &mut PlayerHealth,
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
    (mut particles, mut mobs, mut fireballs, mut carts): (
        Option<ResMut<BlockParticles>>,
        Query<MobTarget, Without<Player>>,
        Query<(Entity, &mut Fireball, &Transform), Without<Player>>,
        Query<(Entity, &Transform, &mut Minecart, Option<&Cargo>), Without<Player>>,
    ),
    mut focus: ResMut<BlockFocus>,
    (mut state, frame): (Local<BlockInteractState>, Res<bevy::diagnostic::FrameCount>),
    (mut inventory_screen, mut bed_uses): (ResMut<InventorySession>, MessageWriter<BedUse>),
    mut workbench: ResMut<ActiveWorkbench>,
    mut item_rng: Local<ItemRng>,
) {
    let ticks = tick.ticks_this_frame();
    for _ in 0..ticks {
        if state.place_delay > 0 {
            state.place_delay -= 1;
        }
    }

    if state
        .last_frame
        .is_none_or(|last| last.wrapping_add(1) != frame.0)
    {
        state.wait_for_release = true;
    }
    state.last_frame = Some(frame.0);

    let locked = windows
        .single()
        .is_ok_and(|(window, cursor)| window.focused && cursor.grab_mode == CursorGrabMode::Locked);
    if !locked {
        state.mining.reset();
        state.wait_for_release = true;
        *focus = BlockFocus::default();
        return;
    }

    // The click that grabbed the cursor must not also break a block.
    if state.wait_for_release {
        state.wait_for_release = mouse.pressed(MouseButton::Left);
    }
    let click_carried = state.wait_for_release;

    let Ok((
        player_entity,
        transform,
        size,
        collision,
        mut hotbar,
        mut inventory,
        velocity,
        sleep,
        mut health,
    )) = player.single_mut()
    else {
        *focus = BlockFocus::default();
        return;
    };
    // `isMovementBlocked`: a sleeping player's hands are still.
    if sleep.is_some_and(|sleep| sleep.sleeping) {
        state.mining.reset();
        *focus = BlockFocus::default();
        return;
    }

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

    let left_click = mouse.just_pressed(MouseButton::Left) && !click_carried;
    let right_click = mouse.just_pressed(MouseButton::Right);
    let left_held = mouse.pressed(MouseButton::Left) && !click_carried;
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
                }))
                .chain(carts.iter().map(|(entity, transform, ..)| {
                    let aabb = CART_SIZE.aabb(transform.translation);
                    (Pointed::Minecart(entity), bordered(aabb, MOB_BORDER))
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
                Pointed::Minecart(target) if left_click => {
                    // `EntityMinecart.attackEntityFrom`: the blow's damage
                    // (a point more when falling) shakes the cart, and enough
                    // of it breaks the cart.
                    let amount = i32::from(damage_vs_entity(hotbar.selected_stack()))
                        + i32::from(velocity.0.y < 0.0);
                    if let Ok((_, at, mut cart, cargo)) = carts.get_mut(target)
                        && cart.hurt(amount)
                    {
                        let position = at.translation;
                        break_cart(&mut commands, &mut item_rng, target, &cart, position, cargo);
                        if let Some(persistence) = persistence.as_deref_mut() {
                            persistence
                                .mark_dirty(ChunkPosition::from_world(position.x, position.z));
                        }
                    }
                }
                Pointed::Minecart(target) => {
                    // `EntityMinecart.interact`.
                    if let Ok((_, at, mut cart, cargo)) = carts.get_mut(target) {
                        match cart.kind {
                            CartKind::Empty => {
                                // Riding again steps off, as `mountEntity` does.
                                if cart.rider == Some(player_entity) {
                                    dismount(&mut commands, player_entity);
                                } else {
                                    mount(&mut commands, player_entity, target);
                                }
                            }
                            CartKind::Chest => {
                                close_crafting_session(
                                    &mut commands,
                                    transform,
                                    &mut item_rng,
                                    &mut hotbar,
                                    &mut inventory,
                                    &mut workbench,
                                );
                                let cell = at.translation.floor().as_ivec3();
                                chunks.open_cart = Some(OpenCart {
                                    cart: target,
                                    slots: cargo.map_or([None; 27], |cargo| cargo.0),
                                });
                                inventory_screen.open = true;
                                inventory_screen.workbench = false;
                                inventory_screen.furnace = false;
                                inventory_screen.furnace_position = None;
                                inventory_screen.chest = true;
                                inventory_screen.chest_position = Some((cell.x, cell.y, cell.z));
                                inventory_screen.chest_group = Some(ChestGroup {
                                    first: (cell.x, cell.y, cell.z),
                                    second: None,
                                    dispenser: false,
                                    cart: true,
                                });
                                inventory_screen.cart = Some(target);
                                if let Ok((_, mut cursor)) = windows.single_mut() {
                                    cursor.visible = true;
                                    cursor.grab_mode = CursorGrabMode::None;
                                }
                            }
                            CartKind::Furnace => {
                                // Coal fuels it, and any click shoves it away
                                // from the player.
                                if hotbar
                                    .selected_stack()
                                    .is_some_and(|stack| stack.item() == Item::Coal)
                                {
                                    hotbar.take_selected(1);
                                    cart.fuel += FUEL_PER_COAL;
                                }
                                cart.push = Vec2::new(
                                    at.translation.x - transform.translation.x,
                                    at.translation.z - transform.translation.z,
                                );
                            }
                        }
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
        inventory_screen.cart = None;
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
        && hit.is_some_and(|hit| hit.block.is_chest() || hit.block == Block::Dispenser)
    {
        let hit = hit.expect("checked above");
        let Some(group) = chunks.container_group_at(hit.x, hit.y, hit.z) else {
            state.mining.reset();
            *focus = BlockFocus::default();
            return;
        };
        // Only a chest is blocked by a cube on top.
        let blocked = !group.dispenser
            && [Some(group.first), group.second]
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
        inventory_screen.cart = None;
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
            // `BlockTNT.onBlockClicked`: flint and steel marks the block, and
            // breaking it then lights the fuse instead of dropping it.
            if hit.block == Block::Tnt
                && hotbar
                    .selected_stack()
                    .is_some_and(|stack| stack.item() == Item::FlintAndSteel)
            {
                chunks.set_metadata(hit.x, hit.y, hit.z, 1);
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(hit.x, hit.z));
                }
            }
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
            // `BlockBed.blockActivated` needs the player, so it is not a
            // block behavior.
            if hit.block == Block::Bed {
                bed_uses.write(BedUse {
                    position: IVec3::new(hit.x, hit.y, hit.z),
                });
            }
        }
        // `Block.blockActivated` returning true keeps the held item unused.
        let activated = hit.is_some_and(|hit| {
            hit.block.is_door()
                || matches!(
                    hit.block,
                    Block::Bed
                        | Block::Trapdoor
                        | Block::Lever
                        | Block::StoneButton
                        | Block::Repeater
                        | Block::PoweredRepeater
                        | Block::NoteBlock
                )
        });
        if activated {
        } else if right_click
            && let Some(heal) = hotbar
                .selected_stack()
                .and_then(|stack| stack.item().heal_amount())
        {
            // `ItemFood.onItemRightClick`: eaten at once, whatever is clicked.
            health.heal(heal);
            if hotbar
                .selected_stack()
                .is_some_and(|stack| stack.item() == Item::MushroomStew)
            {
                // `ItemSoup`: the bowl stays behind.
                let selected = hotbar.selected;
                hotbar.slots[selected] = ItemStack::new(Item::Bowl, 1).ok();
            } else {
                hotbar.take_selected(1);
            }
        } else if let Some(hit) = hit
            && matches!(
                hit.block,
                Block::Rail | Block::PoweredRail | Block::DetectorRail
            )
            && let Some(kind) = hotbar
                .selected_stack()
                .and_then(|stack| CartKind::from_item(stack.item()))
        {
            // `ItemMinecart.onItemUse`: a cart on the clicked rail.
            spawn_cart(&mut commands, IVec3::new(hit.x, hit.y, hit.z), kind);
            let selected = hotbar.selected;
            hotbar.slots[selected] = hotbar.slots[selected].and_then(|stack| {
                ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok()
            });
            if let Some(persistence) = persistence.as_deref_mut() {
                persistence.mark_dirty(ChunkPosition::from_block(hit.x, hit.z));
            }
        } else if hotbar
            .selected_stack()
            .is_some_and(|stack| stack.item() == Item::Bucket)
            && let Some((x, y, z, previous, fluid)) =
                pick_up_fluid(&mut chunks, view_origin, view_rotation * Vec3::NEG_Z)
        {
            let filled = match fluid {
                Fluid::Water => Item::WaterBucket,
                Fluid::Lava => Item::LavaBucket,
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
            } else if let Some(door) = match stack.item() {
                Item::WoodenDoor => Some(Block::WoodenDoor),
                Item::IronDoor => Some(Block::IronDoor),
                _ => None,
            } && let previous = [0, 1].map(|dy| {
                (
                    chunks.block_at(hit.x, hit.y + 1 + dy, hit.z),
                    chunks.metadata_at(hit.x, hit.y + 1 + dy, hit.z),
                )
            }) && place_door(
                &mut chunks,
                hit,
                door,
                furnace_facing_toward_player(transform.rotation * Vec3::NEG_Z),
            ) {
                let selected = hotbar.selected;
                hotbar.slots[selected] =
                    ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
                for (dy, (previous, metadata)) in previous.into_iter().enumerate() {
                    let y = hit.y + 1 + dy as i32;
                    push_event(
                        &mut block_ticks,
                        BlockEvent::Changed {
                            position: IVec3::new(hit.x, y, hit.z),
                            previous: previous.unwrap_or(Block::Air),
                            metadata,
                        },
                    );
                    notify_edit(&mut streaming, &mut persistence, hit.x, y, hit.z, false);
                }
            } else if stack.item() == Item::Bed
                && let Some(cells) = place_bed(
                    &mut chunks,
                    hit,
                    furnace_facing_toward_player(transform.rotation * Vec3::NEG_Z),
                )
            {
                let selected = hotbar.selected;
                hotbar.slots[selected] =
                    ItemStack::with_data(stack.item(), stack.count() - 1, stack.data()).ok();
                for cell in cells {
                    push_event(
                        &mut block_ticks,
                        BlockEvent::Changed {
                            position: cell,
                            previous: Block::Air,
                            metadata: 0,
                        },
                    );
                    notify_edit(
                        &mut streaming,
                        &mut persistence,
                        cell.x,
                        cell.y,
                        cell.z,
                        false,
                    );
                }
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
            } else if stack.item() == Item::FlintAndSteel {
                // `ItemFlintAndSteel.onItemUse`: fire in the cell against the
                // clicked face if it is empty, and one use either way. Fire's
                // `onBlockAdded` decides whether it stays or lights a portal.
                let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
                if chunks.block_at(x, y, z) == Some(Block::Air)
                    && chunks.set_block(x, y, z, Block::Fire).is_some()
                {
                    push_event(
                        &mut block_ticks,
                        BlockEvent::Changed {
                            position: IVec3::new(x, y, z),
                            previous: Block::Air,
                            metadata: 0,
                        },
                    );
                    notify_edit(&mut streaming, &mut persistence, x, y, z, true);
                }
                hotbar.damage_selected(1);
            } else if stack.item() == Item::WaterBucket
                && block_ticks
                    .as_ref()
                    .is_some_and(|ticks| ticks.dimension().is_hell())
            {
                // `ItemBucket`: water poured in the Nether boils away, and
                // the bucket comes back empty.
                let (x, y, z) = hit.face.neighbor(hit.x, hit.y, hit.z);
                if chunks
                    .block_at(x, y, z)
                    .is_some_and(|block| !block.is_solid_material())
                {
                    let selected = hotbar.selected;
                    hotbar.slots[selected] = ItemStack::new(Item::Bucket, 1).ok();
                }
            } else if let Some(fluid) = match stack.item() {
                Item::WaterBucket => Some(Fluid::Water),
                Item::LavaBucket => Some(Fluid::Lava),
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
) -> Option<(i32, i32, i32, Block, Fluid)> {
    let hit = raycast_blocks_or_liquid(chunks, origin, direction, BLOCK_REACH)?;
    let fluid = Fluid::of(hit.block)?;
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
    fluid: Fluid,
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
    let dispenser_drops = chunks
        .dispenser_at(hit.x, hit.y, hit.z)
        .map(|dispenser| dispenser.slots.into_iter().flatten().collect::<Vec<_>>())
        .unwrap_or_default();
    let light_edit = hit.block.is_torch()
        || matches!(hit.block, Block::RedstoneTorch | Block::UnlitRedstoneTorch)
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
        spawn_chest_drops(
            commands,
            rng,
            IVec3::new(hit.x, hit.y, hit.z),
            dispenser_drops,
        );
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
    let mut push = |dx: i32, dy: i32, dz: i32, block: Block, facing: Option<Direction>| {
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
    for side in Direction::ALL {
        // The torch or ladder at `side` of the block names the block as its
        // support, which lies on the opposite side of the attached cell.
        let [dx, dy, dz] = side.offset();
        let facing = match side {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
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
    place_selected_block_facing(chunks, hit, player, selected, 0, Direction::South)
}

pub fn place_selected_block_facing(
    chunks: &mut WorldChunks,
    hit: BlockHit,
    player: Aabb,
    selected: Block,
    species: u8,
    front: Direction,
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
            .is_some_and(|ground| plant_ground_can_hold(selected, ground))
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
    // `canPlaceBlockAt`: dust, repeaters, plates, and rails sit on a full cube.
    if matches!(
        selected,
        Block::RedstoneWire
            | Block::Repeater
            | Block::StonePressurePlate
            | Block::WoodenPressurePlate
            | Block::Rail
            | Block::PoweredRail
            | Block::DetectorRail
    ) && !chunks
        .block_at(x, y - 1, z)
        .is_some_and(Block::is_normal_cube)
    {
        return false;
    }
    // Levers, buttons, and redstone torches hang on the side or top of a cube.
    if matches!(
        selected,
        Block::Lever | Block::StoneButton | Block::RedstoneTorch
    ) && (!hit.block.is_normal_cube()
        || hit.face == BlockFace::Down
        || (selected == Block::StoneButton && hit.face == BlockFace::Up))
    {
        return false;
    }
    // `BlockFence.canPlaceBlockAt`: on another fence or on solid ground.
    if selected == Block::Fence
        && !chunks
            .block_at(x, y - 1, z)
            .is_some_and(|below| below == Block::Fence || below.is_solid_material())
    {
        return false;
    }
    let (block, metadata) = match selected {
        Block::Torch => {
            let support = match hit.face {
                BlockFace::Up => None,
                BlockFace::Down => return false,
                BlockFace::West => Some(Direction::East),
                BlockFace::East => Some(Direction::West),
                BlockFace::North => Some(Direction::South),
                BlockFace::South => Some(Direction::North),
            };
            (
                selected,
                support.map_or(0, |facing| selected.facing_metadata(facing)),
            )
        }
        Block::Furnace | Block::Pumpkin | Block::Chest | Block::Dispenser => {
            (selected, selected.facing_metadata(front))
        }
        // `BlockStairs.onBlockPlacedBy`: the steps climb away from the player.
        Block::WoodenStairs | Block::CobblestoneStairs => (
            selected,
            match front {
                Direction::North => 2,
                Direction::East => 1,
                Direction::South => 3,
                Direction::West => 0,
            },
        ),
        // `BlockTrapDoor.canPlaceBlockOnSide` and `onBlockPlaced`: hinged on
        // the side of a full cube.
        Block::Trapdoor => {
            if !hit.block.is_normal_cube() {
                return false;
            }
            let metadata = match hit.face {
                BlockFace::North => 0,
                BlockFace::South => 1,
                BlockFace::West => 2,
                BlockFace::East => 3,
                BlockFace::Up | BlockFace::Down => return false,
            };
            (selected, metadata)
        }
        Block::Ladder => {
            let Some(support) = ladder_facing(chunks, x, y, z, hit.face, hit.block) else {
                return false;
            };
            (selected, selected.facing_metadata(support))
        }
        // Beta's `onBlockPlaced` names the clicked side: 5 is the floor.
        Block::Lever | Block::StoneButton | Block::RedstoneTorch => (
            selected,
            match hit.face {
                BlockFace::West => 2,
                BlockFace::East => 1,
                BlockFace::North => 4,
                BlockFace::South => 3,
                BlockFace::Up | BlockFace::Down => 5,
            },
        ),
        // `BlockRedstoneRepeater.onBlockPlacedBy`: the repeater points away
        // from the player and takes its input from behind.
        Block::Repeater => (
            selected,
            match front {
                Direction::South => 0,
                Direction::West => 1,
                Direction::North => 2,
                Direction::East => 3,
            },
        ),
        Block::Piston | Block::StickyPiston => {
            (selected, piston_placement_facing(player, (x, y, z), front))
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

/// `ItemDoor.onItemUse`: stand a two-block door on the top face of a full
/// cube. `front` is the side facing the player, as furnaces take it. The
/// hinge goes to the side with more solid blocks, or beside another door so
/// the pair opens from the middle.
pub fn place_door(chunks: &mut WorldChunks, hit: BlockHit, door: Block, front: Direction) -> bool {
    if hit.face != BlockFace::Up {
        return false;
    }
    let (x, y, z) = (hit.x, hit.y + 1, hit.z);
    // `BlockDoor.canPlaceBlockAt`.
    let free = |chunks: &WorldChunks, y: i32| {
        chunks
            .block_at(x, y, z)
            .is_some_and(|block| block.is_replaceable())
    };
    if y >= CHUNK_HEIGHT as i32 - 1
        || !hit.block.is_normal_cube()
        || !free(chunks, y)
        || !free(chunks, y + 1)
    {
        return false;
    }
    // Beta's `(yaw + 180) * 4 / 360 - 0.5` quadrant.
    let mut facing: u8 = match front {
        Direction::North => 1,
        Direction::East => 2,
        Direction::South => 3,
        Direction::West => 0,
    };
    let (dx, dz) = match facing {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    };
    let cubes = |chunks: &WorldChunks, sx: i32, sz: i32| {
        (0..2)
            .filter(|dy| {
                chunks
                    .block_at(sx, y + dy, sz)
                    .is_some_and(Block::is_opaque_cube)
            })
            .count()
    };
    let has_door = |chunks: &WorldChunks, sx: i32, sz: i32| {
        (0..2).any(|dy| chunks.block_at(sx, y + dy, sz) == Some(door))
    };
    let behind = cubes(chunks, x - dx, z - dz);
    let ahead = cubes(chunks, x + dx, z + dz);
    let door_behind = has_door(chunks, x - dx, z - dz);
    let door_ahead = has_door(chunks, x + dx, z + dz);
    if (door_behind && !door_ahead) || ahead > behind {
        facing = (facing.wrapping_sub(1) & 3) + 4;
    }
    chunks.set_block_with_metadata(x, y, z, door, facing);
    chunks.set_block_with_metadata(x, y + 1, z, door, facing + 8);
    true
}

/// `BlockPistonBase.determineOrientation`: close to the placed block, the
/// player's eye height takes priority over horizontal facing. The collision
/// box starts at the feet, so `min.y + 1.82` is Beta's placement height
/// (`posY + 1.82 - yOffset`).
fn piston_placement_facing(player: Aabb, (x, y, z): (i32, i32, i32), front: Direction) -> u8 {
    let player_x = (player.min.x + player.max.x) * 0.5;
    let player_z = (player.min.z + player.max.z) * 0.5;
    if (player_x - x as f32).abs() < 2.0 && (player_z - z as f32).abs() < 2.0 {
        let placement_height = player.min.y + 1.82;
        if placement_height - y as f32 > 2.0 {
            return 1;
        }
        if y as f32 - placement_height > 0.0 {
            return 0;
        }
    }
    match front {
        Direction::North => 2,
        Direction::East => 5,
        Direction::South => 3,
        Direction::West => 4,
    }
}

fn ladder_facing(
    chunks: &WorldChunks,
    x: i32,
    y: i32,
    z: i32,
    hit_face: BlockFace,
    hit_block: Block,
) -> Option<Direction> {
    let support_at = |facing: Direction| {
        let [dx, dy, dz] = facing.offset();
        chunks
            .block_at(x + dx, y + dy, z + dz)
            .filter(|block| (*block).is_opaque_cube())
            .map(|_| facing)
    };

    let clicked_wall = match hit_face {
        BlockFace::West if hit_block.is_opaque_cube() => Some(Direction::East),
        BlockFace::East if hit_block.is_opaque_cube() => Some(Direction::West),
        BlockFace::North if hit_block.is_opaque_cube() => Some(Direction::South),
        BlockFace::South if hit_block.is_opaque_cube() => Some(Direction::North),
        _ => None,
    };
    clicked_wall
        .and_then(support_at)
        // Beta's onBlockPlaced fallback checks +Z, -Z, +X, -X.
        .or_else(|| support_at(Direction::South))
        .or_else(|| support_at(Direction::North))
        .or_else(|| support_at(Direction::East))
        .or_else(|| support_at(Direction::West))
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

/// `ItemBed.onItemUse`: lay a bed on the top face of a normal cube, pointing
/// away from the player. `front` is the side facing the player, as furnaces
/// take it. Returns the two cells written, the clicked one first.
pub fn place_bed(chunks: &mut WorldChunks, hit: BlockHit, front: Direction) -> Option<[IVec3; 2]> {
    if hit.face != BlockFace::Up {
        return None;
    }
    // Beta's `yaw * 4 / 360 + 0.5` quadrant: the way the player looks.
    let direction: u8 = match front {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
    };
    let near = IVec3::new(hit.x, hit.y + 1, hit.z);
    let far = near + bed::head_to_foot(direction);
    let fits = |chunks: &WorldChunks, cell: IVec3| {
        chunks.block_at(cell.x, cell.y, cell.z) == Some(Block::Air)
            && chunks
                .block_at(cell.x, cell.y - 1, cell.z)
                .is_some_and(Block::is_normal_cube)
    };
    if !fits(chunks, near) || !fits(chunks, far) {
        return None;
    }
    chunks.set_block_with_metadata(near.x, near.y, near.z, Block::Bed, direction);
    chunks.set_block_with_metadata(far.x, far.y, far.z, Block::Bed, direction + bed::FOOT);
    Some([near, far])
}

fn furnace_facing_toward_player(player_forward: Vec3) -> Direction {
    let toward_player = Vec2::new(-player_forward.x, -player_forward.z);
    if toward_player.x.abs() > toward_player.y.abs() {
        if toward_player.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if toward_player.y >= 0.0 {
        Direction::South
    } else {
        Direction::North
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

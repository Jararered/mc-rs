//! Break and place blocks along the camera ray.
//!
//! Selected hotbar blocks can be placed. Mining speed, drops, and tool wear
//! follow the held stack.

use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::block::fluids::is_water;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::boat::BOAT_SIZE;
use crate::entity::boat::Boat;
use crate::entity::boat::break_boat;
use crate::entity::boat::spawn_boat;
use crate::entity::combat::bordered;
use crate::entity::combat::pick;
use crate::entity::drops::items::spawn_thrown_item;
use crate::entity::minecart::CART_SIZE;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::CartKind;
use crate::entity::minecart::FUEL_PER_COAL;
use crate::entity::minecart::Minecart;
use crate::entity::minecart::break_cart;
use crate::entity::minecart::spawn_cart;
use crate::entity::mount::Seat;
use crate::entity::mount::dismount;
use crate::entity::mount::mount;
use crate::entity::projectiles::FIREBALL_SIZE;
use crate::entity::projectiles::Fireball;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::InventorySession;
use crate::inventory::session::close_crafting_session;
use crate::item::Item;
use crate::item::ItemStack;
use crate::item::tools::damage_vs_entity;
use crate::physics::BLOCK_REACH;
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
use crate::rendering::particles::block::BlockParticles;
use crate::rendering::particles::effects::EffectParticles;
use crate::rendering::particles::effects::FxKind;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::behaviors::leaves::PLAYER_PLACED;
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
use super::use_item::ItemUse;
use super::use_item::launches;
use crate::player::Player;
use crate::player::PlayerCamera;
use crate::player::PlayerHealth;
use crate::player::sleep::BedUse;
use crate::player::sleep::PlayerSleep;

mod breaking;
mod placement;
mod tools;

use breaking::apply_break;
pub use breaking::break_block;
use placement::furnace_facing_toward_player;
pub use placement::place_bed;
pub use placement::place_block;
pub use placement::place_door;
pub use placement::place_selected_block;
pub use placement::place_selected_block_facing;
pub use tools::pick_up_fluid;
pub use tools::place_fluid;
pub use tools::plant_seeds;
pub use tools::till_block;
pub use tools::till_with_selected_hoe;

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

/// `ItemBoat`'s own reach.
const BOAT_REACH: f32 = 5.0;

/// What the crosshair rests on.
#[derive(Clone, Copy)]
enum Pointed {
    Mob(Entity),
    Fireball(Entity),
    Minecart(Entity),
    Boat(Entity),
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
    (mut particles, mut effects, mut mobs, mut fireballs, mut carts, mut boats): (
        Option<ResMut<BlockParticles>>,
        Option<ResMut<EffectParticles>>,
        Query<MobTarget, Without<Player>>,
        Query<(Entity, &mut Fireball, &Transform), Without<Player>>,
        Query<(Entity, &Transform, &mut Minecart, Option<&Cargo>), Without<Player>>,
        Query<(Entity, &Transform, &mut Boat, &Seat), Without<Player>>,
    ),
    mut focus: ResMut<BlockFocus>,
    (mut state, frame): (Local<BlockInteractState>, Res<bevy::diagnostic::FrameCount>),
    (mut inventory_screen, mut bed_uses, mut item_uses): (
        ResMut<InventorySession>,
        MessageWriter<BedUse>,
        MessageWriter<ItemUse>,
    ),
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
                }))
                .chain(boats.iter().map(|(entity, transform, ..)| {
                    let aabb = BOAT_SIZE.aabb(transform.translation);
                    (Pointed::Boat(entity), bordered(aabb, MOB_BORDER))
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
                Pointed::Mob(target) => {
                    interact(
                        &mut commands,
                        &mut item_rng,
                        &mut mobs,
                        target,
                        player_entity,
                        &mut hotbar,
                        &mut inventory,
                        effects.as_deref_mut(),
                    );
                    // `Minecraft.clickMouse` goes on to `sendUseItem` after
                    // `interactWithEntity`.
                    if hotbar
                        .selected_stack()
                        .is_some_and(|stack| launches(stack.item()))
                    {
                        item_uses.write(ItemUse {
                            eye: transform.translation,
                            look,
                        });
                    }
                }
                Pointed::Fireball(target) => {
                    if left_click && let Ok((_, mut fireball, _)) = fireballs.get_mut(target) {
                        fireball.deflect(look);
                    }
                }
                Pointed::Boat(target) if left_click => {
                    // `EntityBoat.attackEntityFrom`.
                    let amount = i32::from(damage_vs_entity(hotbar.selected_stack()))
                        + i32::from(velocity.0.y < 0.0);
                    if let Ok((_, at, mut boat, seat)) = boats.get_mut(target)
                        && boat.hurt(amount)
                    {
                        let position = at.translation;
                        break_boat(&mut commands, &mut item_rng, target, seat.rider, position);
                        if let Some(persistence) = persistence.as_deref_mut() {
                            persistence
                                .mark_dirty(ChunkPosition::from_world(position.x, position.z));
                        }
                    }
                }
                Pointed::Boat(target) => {
                    // `EntityBoat.interact`: board, or step off again
                    // (`mountEntity` toggles).
                    if let Ok((_, _, _, seat)) = boats.get(target) {
                        if seat.rider == Some(player_entity) {
                            dismount(&mut commands, player_entity);
                        } else {
                            mount(&mut commands, player_entity, target);
                        }
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
        } else if hotbar
            .selected_stack()
            .is_some_and(|stack| launches(stack.item()))
        {
            // `Item.onItemRightClick`: the bow, snowballs, eggs and the rod
            // do nothing to the block clicked and are used instead.
            item_uses.write(ItemUse {
                eye: transform.translation,
                look: view_rotation * Vec3::NEG_Z,
            });
        } else if hotbar
            .selected_stack()
            .is_some_and(|stack| stack.item() == Item::Boat)
        {
            // `ItemBoat.onItemRightClick`: a boat on the block or the water
            // in view, within five blocks. One set on a snow layer rests on
            // the block under it.
            if let Some(hit) = raycast_blocks_or_liquid(
                &chunks,
                view_origin,
                view_rotation * Vec3::NEG_Z,
                BOAT_REACH,
            ) {
                let mut cell = IVec3::new(hit.x, hit.y, hit.z);
                if hit.block == Block::SnowLayer {
                    cell.y -= 1;
                }
                spawn_boat(&mut commands, cell);
                hotbar.take_selected(1);
                if let Some(persistence) = persistence.as_deref_mut() {
                    persistence.mark_dirty(ChunkPosition::from_block(cell.x, cell.z));
                }
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
                // Placed leaves are persistent: they never decay.
                if chunks.block_at(x, y, z).is_some_and(Block::is_leaves) {
                    let species = chunks.metadata_at(x, y, z);
                    chunks.set_metadata(x, y, z, species | PLAYER_PLACED);
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
                    if let Some(effects) = effects.as_deref_mut() {
                        for _ in 0..8 {
                            let jitter =
                                Vec3::new(item_rng.unit(), item_rng.unit(), item_rng.unit());
                            effects.spawn(
                                FxKind::LargeSmoke,
                                IVec3::new(x, y, z).as_vec3() + jitter,
                                Vec3::ZERO,
                            );
                        }
                    }
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

/// Queue a block event for the next tick pass.
fn push_event(ticks: &mut Option<ResMut<BlockTicks>>, event: BlockEvent) {
    if let Some(ticks) = ticks.as_deref_mut() {
        ticks.push_event(event);
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

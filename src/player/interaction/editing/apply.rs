//! The world's side of a player's hands: [`PlayerAction`]s applied to the
//! player who sent them.
//!
//! This is what a Beta server does with `Packet14BlockDig`, `Packet15Place`
//! and `Packet7UseEntity`. It reads no input and no camera, so it runs for
//! any player in a headless world; `interact_blocks` is the client half that
//! decides what the mouse meant.

use bevy::prelude::*;

use super::BOAT_REACH;
use super::breaking::apply_break;
use super::notify_edit;
use super::pick_up_fluid;
use super::place_bed;
use super::place_door;
use super::place_fluid;
use super::place_selected_block_facing;
use super::place_sign;
use super::placement::furnace_facing_toward_player;
use super::plant_seeds;
use super::push_event;
use super::till_with_selected_hoe;
use crate::block::blocks::Block;
use crate::block::fluids::Fluid;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::boat::Boat;
use crate::entity::boat::break_boat;
use crate::entity::boat::spawn_boat;
use crate::entity::drops::items::spawn_thrown_item;
use crate::entity::minecart::Cargo;
use crate::entity::minecart::CartKind;
use crate::entity::minecart::FUEL_PER_COAL;
use crate::entity::minecart::Minecart;
use crate::entity::minecart::break_cart;
use crate::entity::minecart::spawn_cart;
use crate::entity::mount::Seat;
use crate::entity::mount::dismount;
use crate::entity::mount::mount;
use crate::entity::projectiles::Fireball;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::inventory::session::ActiveWorkbench;
use crate::inventory::session::close_crafting_session;
use crate::item::Item;
use crate::item::ItemStack;
use crate::item::tools::damage_vs_entity;
use crate::physics::raycast_blocks_or_liquid;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::player::actions::Action;
use crate::player::actions::PlayerAction;
use crate::player::actions::Pointed;
use crate::player::actions::Window;
use crate::player::actions::WindowOpen;
use crate::player::interaction::attack::MobTarget;
use crate::player::interaction::attack::attack;
use crate::player::interaction::attack::interact;
use crate::player::interaction::use_item::ItemUse;
use crate::player::interaction::use_item::launches;
use crate::player::sleep::BedUse;
use crate::player::sleep::PlayerSleep;
use crate::random::ItemRng;
use crate::rendering::particles::block::BlockParticles;
use crate::rendering::particles::effects::EffectParticles;
use crate::rendering::particles::effects::FxKind;
use crate::world::block_ticks::BlockEvent;
use crate::world::block_ticks::BlockTicks;
use crate::world::block_ticks::behaviors::leaves::PLAYER_PLACED;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::OpenCart;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn apply_player_actions(
    mut commands: Commands,
    mut actions: MessageReader<PlayerAction>,
    mut players: Query<
        (
            &Transform,
            &EntitySize,
            &mut Hotbar,
            &mut Inventory,
            &Velocity,
            Option<&PlayerSleep>,
            &mut PlayerHealth,
        ),
        With<Player>,
    >,
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
    (mut bed_uses, mut item_uses, mut windows, mut strikes): (
        MessageWriter<BedUse>,
        MessageWriter<ItemUse>,
        MessageWriter<WindowOpen>,
        MessageWriter<crate::player::interaction::attack::PlayerStrike>,
    ),
    mut workbench: ResMut<ActiveWorkbench>,
    mut item_rng: Local<ItemRng>,
) {
    for PlayerAction {
        player: player_entity,
        action,
    } in actions.read().copied()
    {
        let Ok((transform, size, mut hotbar, mut inventory, velocity, sleep, mut health)) =
            players.get_mut(player_entity)
        else {
            continue;
        };
        // `isMovementBlocked`: a sleeping player's hands are still.
        if sleep.is_some_and(|sleep| sleep.sleeping) {
            continue;
        }
        match action {
            Action::DropItem => {
                if let Some(stack) = hotbar.take_selected(1) {
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
            }
            Action::UseEntity {
                target: pointed,
                attack: left_click,
                look,
            } => {
                match pointed {
                    // The blow lands in `strike_players`, which can reach
                    // both players at once.
                    Pointed::Player(target) => {
                        if left_click {
                            strikes.write(crate::player::interaction::attack::PlayerStrike {
                                attacker: player_entity,
                                target,
                            });
                        }
                    }
                    Pointed::Mob(target) if left_click => {
                        attack(
                            &mut commands,
                            &mut item_rng,
                            &mut mobs,
                            target,
                            transform.translation,
                            velocity.0.y < 0.0,
                            &mut hotbar,
                        );
                        // `EntityMob.attackEntityFrom`: whoever hit it is
                        // the one it turns on.
                        if let Ok(mut struck) = mobs.get_mut(target)
                            && struck.living.chasing
                        {
                            struck.living.target = Some(player_entity);
                        }
                    }
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
                                player: player_entity,
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
                            break_cart(
                                &mut commands,
                                &mut item_rng,
                                target,
                                &cart,
                                position,
                                cargo,
                            );
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
                                    windows.write(WindowOpen {
                                        player: player_entity,
                                        window: Window::CartChest { cart: target, cell },
                                    });
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
            }
            Action::StartDig { hit } => {
                // `BlockTNT.onBlockClicked`: flint and steel marks the block,
                // and breaking it then lights the fuse instead of dropping it.
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
            }
            Action::Break { hit } => {
                // The block may have gone since the player's hit was taken, and
                // bedrock goes for nobody.
                if hit.block.is_breakable()
                    && chunks.block_at(hit.x, hit.y, hit.z) == Some(hit.block)
                {
                    apply_break(
                        &mut commands,
                        &mut item_rng,
                        &mut chunks,
                        &mut streaming,
                        &mut persistence,
                        &mut block_ticks,
                        &mut particles,
                        &mut hotbar,
                        hit,
                    );
                }
            }
            Action::Use {
                hit,
                origin: view_origin,
                look,
                click: right_click,
            } => {
                // `blockActivated` on a container opens its window, and the
                // held item is not used.
                if right_click && let Some(hit) = hit {
                    let position = IVec3::new(hit.x, hit.y, hit.z);
                    let window = if hit.block.is_furnace() {
                        Some(Window::Furnace { position })
                    } else if hit.block.is_chest() || hit.block == Block::Dispenser {
                        let Some(group) = chunks.container_group_at(hit.x, hit.y, hit.z) else {
                            continue;
                        };
                        // Only a chest is blocked by a cube on top.
                        let blocked = !group.dispenser
                            && [Some(group.first), group.second].into_iter().flatten().any(
                                |(x, y, z)| {
                                    chunks
                                        .block_at(x, y + 1, z)
                                        .is_some_and(Block::is_opaque_cube)
                                },
                            );
                        if blocked {
                            continue;
                        }
                        Some(Window::Chest { position, group })
                    } else if hit.block == Block::CraftingTable {
                        Some(Window::Workbench { position })
                    } else {
                        None
                    };
                    if let Some(window) = window {
                        // Do not let stale player-grid contents leak into a
                        // new session if an earlier interface was interrupted
                        // before its close system ran.
                        close_crafting_session(
                            &mut commands,
                            transform,
                            &mut item_rng,
                            &mut hotbar,
                            &mut inventory,
                            &mut workbench,
                        );
                        if let Window::Workbench { position } = window {
                            workbench.position = Some((position.x, position.y, position.z));
                        }
                        windows.write(WindowOpen {
                            player: player_entity,
                            window,
                        });
                        continue;
                    }
                }
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
                            player: player_entity,
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
                        player: player_entity,
                        eye: transform.translation,
                        look,
                    });
                } else if hotbar
                    .selected_stack()
                    .is_some_and(|stack| stack.item() == Item::Boat)
                {
                    // `ItemBoat.onItemRightClick`: a boat on the block or the water
                    // in view, within five blocks. One set on a snow layer rests on
                    // the block under it.
                    if let Some(hit) =
                        raycast_blocks_or_liquid(&chunks, view_origin, look, BOAT_REACH)
                    {
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
                        pick_up_fluid(&mut chunks, view_origin, look)
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
                            ItemStack::with_data(stack.item(), stack.count() - 1, stack.data())
                                .ok();
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
                            ItemStack::with_data(stack.item(), stack.count() - 1, stack.data())
                                .ok();
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
                    } else if stack.item() == Item::Sign
                        && let Some((cell, previous, metadata)) =
                            place_sign(&mut chunks, hit, transform.rotation * Vec3::NEG_Z)
                    {
                        let selected = hotbar.selected;
                        hotbar.slots[selected] =
                            ItemStack::with_data(stack.item(), stack.count() - 1, stack.data())
                                .ok();
                        push_event(
                            &mut block_ticks,
                            BlockEvent::Changed {
                                position: cell,
                                previous,
                                metadata,
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
                    } else if stack.item() == Item::Bed
                        && let Some(cells) = place_bed(
                            &mut chunks,
                            hit,
                            furnace_facing_toward_player(transform.rotation * Vec3::NEG_Z),
                        )
                    {
                        let selected = hotbar.selected;
                        hotbar.slots[selected] =
                            ItemStack::with_data(stack.item(), stack.count() - 1, stack.data())
                                .ok();
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
                            ItemStack::with_data(stack.item(), stack.count() - 1, stack.data())
                                .ok();
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
                                    let jitter = Vec3::new(
                                        item_rng.unit(),
                                        item_rng.unit(),
                                        item_rng.unit(),
                                    );
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
        }
    }
}

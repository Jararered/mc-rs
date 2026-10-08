//! Chat command parsing, help, validation, and execution.

use bevy::prelude::*;

use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::entity::mobs::Mob;
use crate::entity::mobs::spawn;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::player::MAX_PLAYER_HEALTH;
use crate::player::Player;
use crate::player::PlayerHealth;
use crate::player::PlayerInterpolation;
use crate::random::ItemRng;
use crate::rendering::textures::BlockMaterial;
use crate::rendering::textures::LineRasterSupported;
use crate::rendering::textures::MeshWireframe;
use crate::rendering::textures::configure_mesh_wireframe;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::DAY_LENGTH;
use crate::world::tick::WorldTick;

use super::ChatHistory;
use super::ChatSubmission;
use super::commands::*;
use super::registry::CommandRegistry;

use bevy::ecs::system::SystemParam;

#[derive(SystemParam)]
pub(super) struct CommandContext<'w, 's> {
    commands: Commands<'w, 's>,
    chat: ResMut<'w, ChatHistory>,
    registry: Res<'w, CommandRegistry>,
    clock: ResMut<'w, WorldTick>,
    player: Query<
        'w,
        's,
        (
            &'static mut Transform,
            &'static mut Velocity,
            &'static mut CollisionState,
            &'static mut PlayerInterpolation,
            &'static mut Hotbar,
            &'static mut Inventory,
            &'static EntitySize,
            &'static mut PlayerHealth,
        ),
        With<Player>,
    >,
    chunks: ResMut<'w, WorldChunks>,
    streaming: Option<ResMut<'w, WorldStreaming>>,
    persistence: Option<ResMut<'w, WorldPersistence>>,
    ticks: Option<ResMut<'w, BlockTicks>>,
    rng: Local<'s, ItemRng>,
    wireframe: Option<ResMut<'w, MeshWireframe>>,
    block_materials: Option<ResMut<'w, Assets<BlockMaterial>>>,
    line_raster: Option<Res<'w, LineRasterSupported>>,
    weather: Option<ResMut<'w, crate::world::weather::WorldWeather>>,
}

pub(super) fn submit_chat(
    mut submissions: MessageReader<ChatSubmission>,
    mut context: CommandContext,
) {
    for submission in submissions.read() {
        context.execute(&submission.0);
    }
}

impl CommandContext<'_, '_> {
    fn execute(&mut self, message: &str) {
        let Self {
            commands,
            chat,
            registry,
            clock,
            player,
            chunks,
            streaming,
            persistence,
            ticks,
            rng,
            wireframe,
            block_materials,
            line_raster,
            weather,
        } = self;
        if !message.starts_with('/') {
            chat.push(format!("<Player> {message}"));
            return;
        }
        let command = match registry.parse(message) {
            Ok(command) => command,
            Err(error) => {
                chat.push(error);
                return;
            }
        };
        match command {
            ChatCommand::HelpCommand(filter) => {
                for line in registry.help(filter.as_deref()) {
                    chat.push(line);
                }
                return;
            }
            ChatCommand::TimeSetCommand(_) | ChatCommand::TimeAddCommand(_) => {
                let previous = clock.world_time();
                let time = match command {
                    ChatCommand::TimeSetCommand(time) => time,
                    ChatCommand::TimeAddCommand(delta) => {
                        let Some(time) = previous.checked_add(delta) else {
                            chat.push("Time would exceed the u64 range");
                            return;
                        };
                        time
                    }
                    _ => unreachable!(),
                };
                if let Some(ticks) = ticks.as_deref_mut() {
                    ticks.rebase_time(previous, time);
                }
                clock.set_world_time(time);
                // This frame's ticks were counted before the jump. Do not replay
                // them against the new time (setting 0 could otherwise wrap).
                clock.idle();
                if let Some(storage) = persistence.as_deref().and_then(WorldPersistence::storage) {
                    storage.set_world_time(time);
                }
                chat.push(format!(
                    "Set time to {time} (daytime {})",
                    time % DAY_LENGTH
                ));
                return;
            }
            ChatCommand::TimeQueryCommand(query) => {
                let time = clock.world_time();
                let (name, value) = match query {
                    TimeQueryType::Daytime => ("Daytime", time % DAY_LENGTH),
                    TimeQueryType::Gametime => ("World time", time),
                    TimeQueryType::Day => ("Day", time / DAY_LENGTH),
                };
                chat.push(format!("{name}: {value}"));
                return;
            }
            _ => {}
        }
        if let ChatCommand::WireframeCommand { enabled, block } = command {
            let (Some(mode), Some(materials), Some(raster)) = (
                wireframe.as_deref_mut(),
                block_materials.as_deref_mut(),
                line_raster.as_deref(),
            ) else {
                chat.push("Wireframe is unavailable");
                return;
            };
            match configure_mesh_wireframe(mode, materials, enabled, block, raster.0) {
                Ok(feedback) => chat.push(feedback),
                Err(error) => chat.push(error),
            }
            return;
        }
        if let ChatCommand::WeatherCommand {
            raining,
            thundering,
        } = command
        {
            let Some(weather) = weather.as_deref_mut() else {
                chat.push("Weather is unavailable");
                return;
            };
            weather.raining = raining;
            weather.thundering = thundering;
            weather.rain_time = if raining { 12_000 } else { 168_000 };
            weather.thunder_time = if thundering { 12_000 } else { 168_000 };
            weather.rain_strength = if raining { 1.0 } else { 0.0 };
            weather.thunder_strength = if thundering { 1.0 } else { 0.0 };
            if let Some(storage) = persistence.as_deref().and_then(WorldPersistence::storage) {
                storage.set_weather(weather);
            }
            chat.push(format!(
                "Weather: {}",
                if thundering {
                    "thunder"
                } else if raining {
                    "rain"
                } else {
                    "clear"
                }
            ));
            return;
        }
        let Ok((
            mut transform,
            mut velocity,
            mut collision,
            mut interpolation,
            mut hotbar,
            mut inventory,
            size,
            mut health,
        )) = player.single_mut()
        else {
            chat.push("Player is unavailable");
            return;
        };
        let feedback = match command {
            ChatCommand::HelpCommand(_)
            | ChatCommand::TimeSetCommand(_)
            | ChatCommand::TimeAddCommand(_)
            | ChatCommand::TimeQueryCommand(_)
            | ChatCommand::WireframeCommand { .. }
            | ChatCommand::WeatherCommand { .. } => {
                unreachable!("handled before the player lookup")
            }
            ChatCommand::GiveCommand { item, amount } => {
                let overflow = give_to_inventory(item, amount, &mut hotbar, &mut inventory);
                let dropped: u32 = overflow.iter().map(|stack| u32::from(stack.count())).sum();
                let cell = (transform.translation - Vec3::Y * size.y_offset)
                    .floor()
                    .as_ivec3();
                for stack in overflow {
                    spawn_block_drop(commands, rng, cell, stack);
                }
                if dropped > 0 {
                    if let Some(persistence) = persistence.as_deref_mut() {
                        let pos = transform.translation;
                        persistence.mark_dirty(ChunkPosition::from_world(pos.x, pos.z));
                    }
                    format!(
                        "Gave {amount} of item {} ({dropped} dropped nearby)",
                        item.as_u16()
                    )
                } else {
                    format!("Gave {amount} of item {}", item.as_u16())
                }
            }
            ChatCommand::TeleportCommand(destination) => {
                transform.translation = destination;
                interpolation.previous_position = destination;
                velocity.0 = Vec3::ZERO;
                *collision = CollisionState::default();
                format!(
                    "Teleported to {}, {}, {}",
                    destination.x, destination.y, destination.z
                )
            }
            ChatCommand::HealCommand => {
                if health.current == 0 {
                    "You are dead".to_owned()
                } else {
                    health.current = MAX_PLAYER_HEALTH;
                    "Restored health".to_owned()
                }
            }
            ChatCommand::SummonCommand(kind) => {
                let feet =
                    transform.translation - Vec3::Y * size.y_offset + *transform.forward() * 2.0;
                if !chunks.contains(ChunkPosition::from_world(feet.x, feet.z)) {
                    "Cannot summon into an unloaded chunk".to_owned()
                } else {
                    spawn(commands, Mob::new(kind, clock.world_time()), feet);
                    format!("Summoned {}", kind.name())
                }
            }
            ChatCommand::SetBlockCommand { position, block } => {
                // Chunk replacement clears container storage. Spill it first,
                // just as player block removal does, rather than deleting items.
                let furnace_drops = if block.is_furnace() {
                    Vec::new()
                } else {
                    chunks
                        .furnace_at(position.x, position.y, position.z)
                        .map(|furnace| furnace.slots.into_iter().flatten().collect())
                        .unwrap_or_default()
                };
                let chest_drops = if block.is_chest() {
                    Vec::new()
                } else {
                    chunks
                        .chest_at(position.x, position.y, position.z)
                        .map(|chest| chest.slots.into_iter().flatten().collect())
                        .unwrap_or_default()
                };
                let dispenser_drops = if block == crate::block::blocks::Block::Dispenser {
                    Vec::new()
                } else {
                    chunks
                        .dispenser_at(position.x, position.y, position.z)
                        .map(|dispenser| dispenser.slots.into_iter().flatten().collect())
                        .unwrap_or_default()
                };
                let change = match set_loaded_block(chunks, position, block) {
                    Ok(change) => change,
                    Err(error) => {
                        chat.push(error);
                        return;
                    }
                };
                if let Some((previous, metadata)) = change {
                    for stack in furnace_drops {
                        spawn_block_drop(commands, rng, position, stack);
                    }
                    spawn_chest_drops(commands, rng, position, chest_drops);
                    spawn_chest_drops(commands, rng, position, dispenser_drops);
                    if let Some(ticks) = ticks.as_deref_mut() {
                        ticks.block_changed(position, previous, metadata);
                    }
                    if let Some(streaming) = streaming.as_deref_mut() {
                        streaming.request_block_update(position.x, position.y, position.z);
                    }
                    if let Some(persistence) = persistence.as_deref_mut() {
                        persistence.mark_dirty(ChunkPosition::from_block(position.x, position.z));
                    }
                    format!(
                        "Set block at {} {} {} to {}",
                        position.x,
                        position.y,
                        position.z,
                        block.as_u8()
                    )
                } else {
                    format!("Block already has ID {}", block.as_u8())
                }
            }
        };
        chat.push(feedback);
    }
}

//! Chat command parsing, help, validation, and execution.

use bevy::prelude::*;

use crate::block::id::Id;
use crate::entity::CollisionState;
use crate::entity::EntitySize;
use crate::entity::Velocity;
use crate::entity::drops::items::spawn_block_drop;
use crate::entity::drops::items::spawn_chest_drops;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::ItemId;
use crate::item::ItemStack;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::random::ItemRng;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::textures::BlockMaterial;
use crate::world::textures::LineRasterSupported;
use crate::world::textures::MeshWireframe;
use crate::world::textures::configure_mesh_wireframe;
use crate::world::tick::DAY_LENGTH;
use crate::world::tick::WorldTick;

use super::ChatState;

// Avoid unbounded entity spawning for erroneous amounts (especially unstackable items).
const MAX_GIVE: u32 = 4096;

const COMMAND_HELP: &[(&str, &str)] = &[
    ("help", "/help [command]"),
    ("time", "/time set <tick|day|night|noon|midnight>"),
    ("time", "/time <day|night|noon|midnight|tick>"),
    ("time", "/time add <ticks>"),
    ("time", "/time query [daytime|gametime|day]"),
    ("give", "/give <item id> <amount>"),
    ("tp", "/tp <x> <y> <z>"),
    ("setblock", "/setblock <x> <y> <z> <block id>"),
    ("wireframe", "/wireframe on|off | /wireframe set <block id>"),
];

/// Parsed commands stay independent of UI and ECS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChatCommand {
    Help(Option<&'static str>),
    TimeSet(u64),
    TimeAdd(u64),
    TimeQuery(TimeQuery),
    Give {
        item: ItemId,
        amount: u32,
    },
    Teleport(Vec3),
    SetBlock {
        position: IVec3,
        block: Id,
    },
    /// `block` is set only by `/wireframe set`. `on` and `off` clear it.
    Wireframe {
        enabled: bool,
        block: Option<Id>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeQuery {
    Daytime,
    Gametime,
    Day,
}

pub fn parse_command(text: &str) -> Result<ChatCommand, String> {
    let mut parts = text.split_whitespace();
    let name = parts.next().unwrap_or("");
    let args: Vec<_> = parts.collect();
    if name == "/help" {
        return match args.as_slice() {
            [] => Ok(ChatCommand::Help(None)),
            [command] => COMMAND_HELP
                .iter()
                .find(|(name, _)| *name == command.trim_start_matches('/'))
                .map(|(name, _)| ChatCommand::Help(Some(name)))
                .ok_or_else(|| format!("Unknown command: {command}. Try /help")),
            _ => Err("Usage: /help [command]".into()),
        };
    }
    if name == "/time" {
        return parse_time(&args);
    }
    if name == "/wireframe" {
        return parse_wireframe(&args);
    }
    let usage = match name {
        "/give" => "/give <item id> <amount>",
        "/tp" => "/tp <x> <y> <z>",
        "/setblock" => "/setblock <x> <y> <z> <block id>",
        _ => return Err(format!("Unknown command: {name}. Try /help")),
    };
    if args.len()
        != match name {
            "/give" => 2,
            "/tp" => 3,
            _ => 4,
        }
    {
        return Err(format!("Usage: {usage}"));
    }
    match name {
        "/give" => {
            let raw = args[0].parse::<u16>().map_err(|_| "Invalid item ID")?;
            let item = ItemId::from_u16(raw)
                .filter(|id| id.properties().is_some())
                .ok_or_else(|| format!("Unknown item ID: {raw}"))?;
            let amount = args[1].parse::<u32>().map_err(|_| "Invalid amount")?;
            if !(1..=MAX_GIVE).contains(&amount) {
                return Err(format!("Amount must be between 1 and {MAX_GIVE}"));
            }
            Ok(ChatCommand::Give { item, amount })
        }
        "/tp" => {
            let coords = args
                .iter()
                .map(|s| s.parse::<f32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Coordinates must be numbers")?;
            if coords
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 30_000_000.0)
            {
                return Err("Coordinates must be finite and within 30 million blocks".into());
            }
            Ok(ChatCommand::Teleport(Vec3::new(
                coords[0], coords[1], coords[2],
            )))
        }
        _ => {
            let values = args[..3]
                .iter()
                .map(|s| s.parse::<i32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Block coordinates must be integers")?;
            let position = IVec3::new(values[0], values[1], values[2]);
            if position.x.unsigned_abs() > 30_000_000
                || position.z.unsigned_abs() > 30_000_000
                || position.y < 0
                || position.y >= CHUNK_HEIGHT as i32
            {
                return Err(format!(
                    "Block coordinates are outside the world (height 0..{})",
                    CHUNK_HEIGHT - 1
                ));
            }
            let block = parse_block_id(args[3])?;
            Ok(ChatCommand::SetBlock { position, block })
        }
    }
}

fn parse_time(args: &[&str]) -> Result<ChatCommand, String> {
    match args {
        ["query"] | ["query", "daytime"] => Ok(ChatCommand::TimeQuery(TimeQuery::Daytime)),
        ["query", "gametime"] => Ok(ChatCommand::TimeQuery(TimeQuery::Gametime)),
        ["query", "day"] => Ok(ChatCommand::TimeQuery(TimeQuery::Day)),
        ["add", ticks] => ticks
            .parse::<u64>()
            .map(ChatCommand::TimeAdd)
            .map_err(|_| "Ticks must be a nonnegative integer within the u64 range".into()),
        ["set", time] | [time] if !matches!(*time, "set" | "add") => {
            let ticks = match *time {
                "day" => 1_000,
                "noon" => 6_000,
                "night" => 13_000,
                "midnight" => 18_000,
                _ => time.parse::<u64>().map_err(|_| {
                    "Time must be day, night, noon, midnight, or a nonnegative integer"
                })?,
            };
            Ok(ChatCommand::TimeSet(ticks))
        }
        _ => Err("Usage: /time set <tick|day|night> | /time add <ticks> | /time query [daytime|gametime|day]".into()),
    }
}

fn parse_wireframe(args: &[&str]) -> Result<ChatCommand, String> {
    const USAGE: &str = "Usage: /wireframe on|off | /wireframe set <block id>";
    match args {
        ["on"] => Ok(ChatCommand::Wireframe {
            enabled: true,
            block: None,
        }),
        ["off"] => Ok(ChatCommand::Wireframe {
            enabled: false,
            block: None,
        }),
        ["set", raw] => {
            let block = parse_block_id(raw)?;
            if block == Id::Air {
                return Err("Cannot show a wireframe of air".into());
            }
            Ok(ChatCommand::Wireframe {
                enabled: true,
                block: Some(block),
            })
        }
        _ => Err(USAGE.into()),
    }
}

/// Beta block IDs a player can type. Compact state IDs at 200 and above stay internal.
fn parse_block_id(raw_text: &str) -> Result<Id, String> {
    let raw = raw_text.parse::<u8>().map_err(|_| "Invalid block ID")?;
    (raw <= 96)
        .then(|| Id::from_u8(raw))
        .flatten()
        .filter(|id| id.in_world())
        .ok_or_else(|| format!("Unsupported in-world block ID: {raw}"))
}

/// Insert valid stacks into the player inventory; return stacks that must be
/// dropped near the player instead of silently deleting overflow.
pub fn give_to_inventory(
    item: ItemId,
    amount: u32,
    hotbar: &mut Hotbar,
    inventory: &mut Inventory,
) -> Vec<ItemStack> {
    let max = item.properties().expect("validated item ID").max_stack_size;
    let mut left = amount;
    let mut overflow = Vec::new();
    while left > 0 {
        let count = left.min(u32::from(max)) as u8;
        let stack = ItemStack::new(item, count).expect("registered item with legal stack size");
        if let Some(remainder) = inventory.insert(hotbar, stack) {
            overflow.push(remainder);
        }
        left -= u32::from(count);
    }
    overflow
}

/// Edit an existing cell without spawning a chunk; return its old block and
/// metadata only when it really changes (for block-tick notifications).
pub fn set_loaded_block(
    chunks: &mut WorldChunks,
    position: IVec3,
    block: Id,
) -> Result<Option<(Id, u8)>, &'static str> {
    let previous = chunks
        .block_at(position.x, position.y, position.z)
        .ok_or("Target chunk is not loaded or block is outside the world")?;
    if previous == block {
        return Ok(None);
    }
    let metadata = chunks.metadata_at(position.x, position.y, position.z);
    chunks.set_block(position.x, position.y, position.z, block);
    Ok(Some((previous, metadata)))
}

pub(super) fn submit_chat(
    mut commands: Commands,
    mut chat: ResMut<ChatState>,
    mut clock: ResMut<WorldTick>,
    mut player: Query<
        (
            &mut Transform,
            &mut Velocity,
            &mut CollisionState,
            &mut PlayerInterpolation,
            &mut Hotbar,
            &mut Inventory,
            &EntitySize,
        ),
        With<Player>,
    >,
    mut chunks: ResMut<WorldChunks>,
    mut streaming: Option<ResMut<WorldStreaming>>,
    mut persistence: Option<ResMut<WorldPersistence>>,
    mut ticks: Option<ResMut<BlockTicks>>,
    mut rng: Local<ItemRng>,
    wireframe: Option<ResMut<MeshWireframe>>,
    block_materials: Option<ResMut<Assets<BlockMaterial>>>,
    line_raster: Option<Res<LineRasterSupported>>,
) {
    let Some(message) = chat.submitted.take() else {
        return;
    };
    if !message.starts_with('/') {
        chat.push(format!("<Player> {message}"));
        return;
    }
    let command = match parse_command(&message) {
        Ok(command) => command,
        Err(error) => {
            chat.push(error);
            return;
        }
    };
    match command {
        ChatCommand::Help(filter) => {
            for &(name, usage) in COMMAND_HELP {
                if filter.is_none_or(|filter| filter == name) {
                    chat.push(usage);
                }
            }
            return;
        }
        ChatCommand::TimeSet(_) | ChatCommand::TimeAdd(_) => {
            let previous = clock.world_time();
            let time = match command {
                ChatCommand::TimeSet(time) => time,
                ChatCommand::TimeAdd(delta) => {
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
        ChatCommand::TimeQuery(query) => {
            let time = clock.world_time();
            let (name, value) = match query {
                TimeQuery::Daytime => ("Daytime", time % DAY_LENGTH),
                TimeQuery::Gametime => ("World time", time),
                TimeQuery::Day => ("Day", time / DAY_LENGTH),
            };
            chat.push(format!("{name}: {value}"));
            return;
        }
        _ => {}
    }
    if let ChatCommand::Wireframe { enabled, block } = command {
        let (Some(mut mode), Some(mut materials), Some(raster)) =
            (wireframe, block_materials, line_raster)
        else {
            chat.push("Wireframe is unavailable");
            return;
        };
        match configure_mesh_wireframe(&mut mode, &mut materials, enabled, block, raster.0) {
            Ok(feedback) => chat.push(feedback),
            Err(error) => chat.push(error),
        }
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
    )) = player.single_mut()
    else {
        chat.push("Player is unavailable");
        return;
    };
    let feedback = match command {
        ChatCommand::Help(_)
        | ChatCommand::TimeSet(_)
        | ChatCommand::TimeAdd(_)
        | ChatCommand::TimeQuery(_)
        | ChatCommand::Wireframe { .. } => unreachable!("handled before the player lookup"),
        ChatCommand::Give { item, amount } => {
            let overflow = give_to_inventory(item, amount, &mut hotbar, &mut inventory);
            let dropped: u32 = overflow.iter().map(|stack| u32::from(stack.count())).sum();
            let cell = (transform.translation - Vec3::Y * size.y_offset)
                .floor()
                .as_ivec3();
            for stack in overflow {
                spawn_block_drop(&mut commands, &mut rng, cell, stack);
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
        ChatCommand::Teleport(destination) => {
            transform.translation = destination;
            interpolation.previous_position = destination;
            velocity.0 = Vec3::ZERO;
            *collision = CollisionState::default();
            format!(
                "Teleported to {}, {}, {}",
                destination.x, destination.y, destination.z
            )
        }
        ChatCommand::SetBlock { position, block } => {
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
            let change = match set_loaded_block(&mut chunks, position, block) {
                Ok(change) => change,
                Err(error) => {
                    chat.push(error);
                    return;
                }
            };
            if let Some((previous, metadata)) = change {
                for stack in furnace_drops {
                    spawn_block_drop(&mut commands, &mut rng, position, stack);
                }
                spawn_chest_drops(&mut commands, &mut rng, position, chest_drops);
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

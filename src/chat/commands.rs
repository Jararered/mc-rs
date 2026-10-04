//! Chat command definitions, parsing, validation, and inventory/block helpers.

use bevy::prelude::*;

use crate::block::blocks::Block;
use crate::entity::mobs::MobType;
use crate::inventory::Hotbar;
use crate::inventory::Inventory;
use crate::item::Item;
use crate::item::ItemStack;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::WorldChunks;

use super::registry::CommandParseError;
use super::registry::CommandRegistry;

// Avoid unbounded entity spawning for erroneous amounts (especially unstackable items).
const MAX_GIVE: u32 = 4096;

/// Parsed commands stay independent of UI and ECS.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatCommand {
    HelpCommand(Option<String>),
    TimeSetCommand(u64),
    TimeAddCommand(u64),
    TimeQueryCommand(TimeQueryType),
    GiveCommand {
        item: Item,
        amount: u32,
    },
    TeleportCommand(Vec3),
    SummonCommand(MobType),
    WeatherCommand {
        raining: bool,
        thundering: bool,
    },
    SetBlockCommand {
        position: IVec3,
        block: Block,
    },
    /// `block` is set only by `/wireframe set`. `on` and `off` clear it.
    WireframeCommand {
        enabled: bool,
        block: Option<Block>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeQueryType {
    Daytime,
    Gametime,
    Day,
}

pub(super) fn register_builtin_commands(registry: &mut CommandRegistry) {
    registry
        .register(
            "summon",
            "Summon a creature at the player.",
            ["/summon <mob>"],
            parse_summon,
        )
        .expect("valid summon command");
    registry
        .register(
            "weather",
            "Set overworld weather.",
            ["/weather clear|rain|thunder"],
            parse_weather,
        )
        .expect("valid weather command");
    registry
        .register(
            "help",
            "List commands or explain a command.",
            ["/help [command]"],
            parse_help,
        )
        .expect("valid help command");
    registry
        .register(
            "time",
            "Set, advance, or query world time.",
            [
                "/time set <tick|day|night|noon|midnight>",
                "/time <day|night|noon|midnight|tick>",
                "/time add <ticks>",
                "/time query [daytime|gametime|day]",
            ],
            parse_time,
        )
        .expect("valid time command");
    registry
        .register(
            "give",
            "Give items, dropping inventory overflow nearby.",
            ["/give <item id> <amount>"],
            parse_give,
        )
        .expect("valid give command");
    registry
        .register(
            "tp",
            "Teleport to world coordinates.",
            ["/tp <x> <y> <z>"],
            parse_teleport,
        )
        .expect("valid tp command");
    registry
        .register(
            "setblock",
            "Replace a block in a loaded chunk.",
            ["/setblock <x> <y> <z> <block id>"],
            parse_setblock,
        )
        .expect("valid setblock command");
    registry
        .register(
            "wireframe",
            "Toggle wireframes or select a block type to outline.",
            ["/wireframe on|off", "/wireframe set <block id>"],
            parse_wireframe,
        )
        .expect("valid wireframe command");
}

fn parse_summon(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    let [name] = args else {
        return Err(CommandParseError::Usage);
    };
    MobType::parse(name)
        .map(ChatCommand::SummonCommand)
        .ok_or_else(|| format!("Unknown mob: {name}").into())
}

fn parse_weather(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    match args {
        ["clear"] => Ok(ChatCommand::WeatherCommand {
            raining: false,
            thundering: false,
        }),
        ["rain"] => Ok(ChatCommand::WeatherCommand {
            raining: true,
            thundering: false,
        }),
        ["thunder"] => Ok(ChatCommand::WeatherCommand {
            raining: true,
            thundering: true,
        }),
        _ => Err(CommandParseError::Usage),
    }
}

fn parse_help(registry: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    match args {
        [] => Ok(ChatCommand::HelpCommand(None)),
        [command] => registry
            .get(command.trim_start_matches('/'))
            .map(|entry| ChatCommand::HelpCommand(Some(entry.name().to_owned())))
            .ok_or_else(|| format!("Unknown command: {command}. Try /help").into()),
        _ => Err(CommandParseError::Usage),
    }
}

fn parse_give(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    let [raw, amount] = args else {
        return Err(CommandParseError::Usage);
    };
    let raw = raw.parse::<u16>().map_err(|_| "Invalid item ID")?;
    let item = Item::from_u16(raw)
        .filter(|id| id.properties().is_some())
        .ok_or_else(|| format!("Unknown item ID: {raw}"))?;
    let amount = amount.parse::<u32>().map_err(|_| "Invalid amount")?;
    if !(1..=MAX_GIVE).contains(&amount) {
        return Err(format!("Amount must be between 1 and {MAX_GIVE}").into());
    }
    Ok(ChatCommand::GiveCommand { item, amount })
}

fn parse_teleport(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    if args.len() != 3 {
        return Err(CommandParseError::Usage);
    }
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
    Ok(ChatCommand::TeleportCommand(Vec3::new(
        coords[0], coords[1], coords[2],
    )))
}

fn parse_setblock(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    if args.len() != 4 {
        return Err(CommandParseError::Usage);
    }
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
        )
        .into());
    }
    let block = parse_block_id(args[3])?;
    Ok(ChatCommand::SetBlockCommand { position, block })
}

fn parse_time(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    match args {
        ["query"] | ["query", "daytime"] => {
            Ok(ChatCommand::TimeQueryCommand(TimeQueryType::Daytime))
        }
        ["query", "gametime"] => Ok(ChatCommand::TimeQueryCommand(TimeQueryType::Gametime)),
        ["query", "day"] => Ok(ChatCommand::TimeQueryCommand(TimeQueryType::Day)),
        ["add", ticks] => ticks
            .parse::<u64>()
            .map(ChatCommand::TimeAddCommand)
            .map_err(|_| "Ticks must be a nonnegative integer within the u64 range".into()),
        ["set", time] | [time] if !matches!(*time, "set" | "add") => {
            let ticks = match *time {
                "day" => 1_000,
                "noon" => 6_000,
                "night" => 13_000,
                "midnight" => 18_000,
                _ => time.parse::<u64>().map_err(
                    |_| "Time must be day, night, noon, midnight, or a nonnegative integer",
                )?,
            };
            Ok(ChatCommand::TimeSetCommand(ticks))
        }
        _ => Err(CommandParseError::Usage),
    }
}

fn parse_wireframe(_: &CommandRegistry, args: &[&str]) -> Result<ChatCommand, CommandParseError> {
    match args {
        ["on"] => Ok(ChatCommand::WireframeCommand {
            enabled: true,
            block: None,
        }),
        ["off"] => Ok(ChatCommand::WireframeCommand {
            enabled: false,
            block: None,
        }),
        ["set", raw] => {
            let block = parse_block_id(raw)?;
            if block == Block::Air {
                return Err("Cannot show a wireframe of air".into());
            }
            Ok(ChatCommand::WireframeCommand {
                enabled: true,
                block: Some(block),
            })
        }
        _ => Err(CommandParseError::Usage),
    }
}

/// Beta block IDs a player can type. Compact state IDs at 200 and above stay internal.
fn parse_block_id(raw_text: &str) -> Result<Block, String> {
    let raw = raw_text.parse::<u8>().map_err(|_| "Invalid block ID")?;
    (raw <= 96)
        .then(|| Block::from_u8(raw))
        .flatten()
        .filter(|id| id.in_world())
        .ok_or_else(|| format!("Unsupported in-world block ID: {raw}"))
}

/// Insert valid stacks into the player inventory; return stacks that must be
/// dropped near the player instead of silently deleting overflow.
pub fn give_to_inventory(
    item: Item,
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
    block: Block,
) -> Result<Option<(Block, u8)>, &'static str> {
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

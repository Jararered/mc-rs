//! Local Beta-style chat input, message overlay, and single-player commands.

use std::collections::VecDeque;

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::text::FontSmoothing;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::state::AppScreen;
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
use crate::physics::PhysicsSet;
use crate::player::Player;
use crate::player::PlayerInterpolation;
use crate::random::ItemRng;
use crate::ui::InventoryScreen;
use crate::ui::icons::overlay::GUI_SCALE;
use crate::ui::icons::overlay::UiFont;
use crate::world::block_ticks::BlockTicks;
use crate::world::chunk::CHUNK_HEIGHT;
use crate::world::chunk::ChunkPosition;
use crate::world::chunk::WorldChunks;
use crate::world::persistence::WorldPersistence;
use crate::world::streaming::WorldStreaming;
use crate::world::tick::WorldTick;

const INPUT_LIMIT: usize = 100;
const HISTORY_LIMIT: usize = 50;
const MESSAGE_LINE_LIMIT: usize = 50;
const VISIBLE_CLOSED: usize = 10;
const VISIBLE_OPEN: usize = 20;
const FADE_TICKS: u32 = 200;
// Avoid unbounded entity spawning for erroneous amounts (especially unstackable items).
const MAX_GIVE: u32 = 4096;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatState>()
            .init_resource::<InventoryScreen>()
            .add_systems(OnEnter(AppScreen::Playing), spawn_chat)
            .add_systems(OnExit(AppScreen::Playing), despawn_chat)
            .add_systems(
                Update,
                (read_chat_input, submit_chat, render_chat)
                    .chain()
                    .before(PhysicsSet::ApplyInput)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Clone, Debug)]
struct ChatLine {
    text: String,
    age: u32,
}

/// UI focus state. `suppress_controls` stays set through the frame chat closes,
/// so the closing Escape/Enter cannot also activate a gameplay shortcut.
#[derive(Resource, Default)]
pub struct ChatState {
    pub open: bool,
    pub suppress_controls: bool,
    input: String,
    submitted: Option<String>,
    lines: VecDeque<ChatLine>,
    blink: u32,
}

impl ChatState {
    fn push(&mut self, text: impl Into<String>) {
        let text = text.into();
        // Beta's font renderer wraps chat to a 320-GUI-pixel column. Split
        // long messages into fixed rows so their backgrounds never overlap.
        let mut rest = text.as_str();
        while !rest.is_empty() {
            let end = rest
                .char_indices()
                .nth(MESSAGE_LINE_LIMIT)
                .map_or(rest.len(), |(byte, _)| byte);
            let split = if end < rest.len() {
                rest[..end]
                    .rfind(' ')
                    .filter(|&space| space > 0)
                    .map_or(end, |space| space + 1)
            } else {
                end
            };
            self.lines.push_front(ChatLine {
                text: rest[..split].trim_end().into(),
                age: 0,
            });
            rest = &rest[split..];
        }
        self.lines.truncate(HISTORY_LIMIT);
    }
}

#[derive(Component)]
struct ChatRoot;
#[derive(Component)]
struct ChatInput;
#[derive(Component)]
struct ChatMessage(usize);

fn spawn_chat(mut commands: Commands, font: Res<UiFont>, mut chat: ResMut<ChatState>) {
    *chat = ChatState::default();
    let font = TextFont::from_font_size(8.0 * GUI_SCALE)
        .with_font(font.minecraft.clone())
        .with_font_smoothing(FontSmoothing::None);
    commands
        .spawn((
            ChatRoot,
            GlobalZIndex(10),
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            for index in 0..VISIBLE_OPEN {
                root.spawn((
                    ChatMessage(index),
                    Text::new(""),
                    TextLayout::no_wrap(),
                    font.clone(),
                    TextColor(Color::NONE),
                    BackgroundColor(Color::NONE),
                    Visibility::Hidden,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(4.0),
                        bottom: px((48.0 + index as f32 * 9.0) * GUI_SCALE),
                        max_width: px(320.0 * GUI_SCALE),
                        ..default()
                    },
                ));
            }
            root.spawn((
                ChatInput,
                Text::new(""),
                TextLayout::no_wrap(),
                font,
                TextColor(Color::srgb_u8(224, 224, 224)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(2.0 * GUI_SCALE),
                    right: px(2.0 * GUI_SCALE),
                    bottom: px(2.0 * GUI_SCALE),
                    height: px(12.0 * GUI_SCALE),
                    overflow: Overflow::clip(),
                    ..default()
                },
            ));
        });
}

fn despawn_chat(
    mut commands: Commands,
    roots: Query<Entity, With<ChatRoot>>,
    mut chat: ResMut<ChatState>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    *chat = ChatState::default();
}

fn read_chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut chat: ResMut<ChatState>,
    inventory: Res<InventoryScreen>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    tick: Res<WorldTick>,
) {
    chat.suppress_controls = chat.open;
    chat.blink = chat.blink.wrapping_add(tick.ticks_this_frame());
    for line in &mut chat.lines {
        line.age = line.age.saturating_add(tick.ticks_this_frame());
    }
    let Ok((window, mut cursor)) = windows.single_mut() else {
        keyboard.read().for_each(drop);
        return;
    };
    if !chat.open {
        // Drain the opening key's text: '/' is already prefilled and 't' must
        // not appear in the new input. Keyboard messages persist across frames.
        keyboard.read().for_each(drop);
        if !inventory.open && window.focused && cursor.grab_mode == CursorGrabMode::Locked {
            if keys.just_pressed(KeyCode::KeyT) || keys.just_pressed(KeyCode::Slash) {
                chat.open = true;
                chat.suppress_controls = true;
                chat.input = if keys.just_pressed(KeyCode::Slash) {
                    "/".into()
                } else {
                    String::new()
                };
                chat.blink = 0;
                cursor.grab_mode = CursorGrabMode::None;
                cursor.visible = true;
            }
        }
        return;
    }

    if !window.focused {
        keyboard.read().for_each(drop);
        return;
    }
    for event in keyboard.read() {
        if event.state != ButtonState::Pressed
            || event.repeat
                && matches!(
                    event.key_code,
                    KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape
                )
        {
            continue;
        }
        match event.key_code {
            KeyCode::Escape => {
                chat.input.clear();
                chat.open = false;
                break;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let text = chat.input.trim();
                if !text.is_empty() {
                    chat.submitted = Some(text.to_owned());
                }
                chat.input.clear();
                chat.open = false;
                break;
            }
            KeyCode::Backspace => {
                chat.input.pop();
            }
            _ => {
                if !keys.pressed(KeyCode::ControlLeft)
                    && !keys.pressed(KeyCode::ControlRight)
                    && !keys.pressed(KeyCode::SuperLeft)
                    && !keys.pressed(KeyCode::SuperRight)
                    && let Some(text) = &event.text
                {
                    for character in text.chars().filter(|c| !c.is_control()) {
                        if chat.input.chars().count() >= INPUT_LIMIT {
                            break;
                        }
                        chat.input.push(character);
                    }
                }
            }
        }
    }
    if !chat.open {
        cursor.visible = false;
        if window.focused {
            cursor.grab_mode = CursorGrabMode::Locked;
        }
    }
}

/// The numeric command subset intentionally stays independent of UI and ECS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChatCommand {
    Give { item: ItemId, amount: u32 },
    Teleport(Vec3),
    SetBlock { position: IVec3, block: Id },
}

pub fn parse_command(text: &str) -> Result<ChatCommand, String> {
    let mut parts = text.split_whitespace();
    let name = parts.next().unwrap_or("");
    let args: Vec<_> = parts.collect();
    let usage = match name {
        "/give" => "/give <item id> <amount>",
        "/tp" => "/tp <x> <y> <z>",
        "/setblock" => "/setblock <x> <y> <z> <block id>",
        _ => return Err(format!("Unknown command: {name}")),
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
            let raw = args[3].parse::<u8>().map_err(|_| "Invalid block ID")?;
            // Internal compact state IDs (200+) aren't user-facing Beta IDs.
            let block = (raw <= 96)
                .then(|| Id::from_u8(raw))
                .flatten()
                .filter(|id| id.in_world())
                .ok_or_else(|| format!("Unsupported in-world block ID: {raw}"))?;
            Ok(ChatCommand::SetBlock { position, block })
        }
    }
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

fn submit_chat(
    mut commands: Commands,
    mut chat: ResMut<ChatState>,
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

fn render_chat(
    chat: Res<ChatState>,
    mut input: Query<(&mut Text, &mut Visibility), (With<ChatInput>, Without<ChatMessage>)>,
    mut messages: Query<
        (
            &ChatMessage,
            &mut Text,
            &mut TextColor,
            &mut BackgroundColor,
            &mut Visibility,
        ),
        Without<ChatInput>,
    >,
) {
    if let Ok((mut text, mut visibility)) = input.single_mut() {
        *visibility = if chat.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if chat.open {
            let cursor = if chat.blink / 6 % 2 == 0 { "_" } else { "" };
            **text = format!("> {}{cursor}", chat.input);
        }
    }
    for (index, mut text, mut color, mut background, mut visibility) in &mut messages {
        let Some(line) = chat
            .lines
            .get(index.0)
            .filter(|line| chat.open || index.0 < VISIBLE_CLOSED && line.age < FADE_TICKS)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let opacity = if chat.open {
            1.0
        } else {
            let remaining = (FADE_TICKS - line.age) as f32 / FADE_TICKS as f32;
            (remaining * 10.0).min(1.0).powi(2)
        };
        **text = line.text.clone();
        color.0 = Color::srgba(1.0, 1.0, 1.0, opacity);
        background.0 = Color::srgba(0.0, 0.0, 0.0, opacity * 0.5);
        *visibility = Visibility::Visible;
    }
}

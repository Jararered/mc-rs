//! Local Beta-style chat input and message overlay.

pub mod commands;
pub mod registry;

use std::collections::VecDeque;

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::text::FontSmoothing;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::state::AppScreen;
use crate::physics::PhysicsSet;
use crate::ui::InventoryScreen;
use crate::ui::icons::overlay::GUI_SCALE;
use crate::ui::icons::overlay::UiFont;
use crate::world::tick::WorldTick;

use self::commands::submit_chat;

const INPUT_LIMIT: usize = 100;
const HISTORY_LIMIT: usize = 50;
const MESSAGE_LINE_LIMIT: usize = 50;
const VISIBLE_CLOSED: usize = 10;
const VISIBLE_OPEN: usize = 20;
const FADE_TICKS: u32 = 200;

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatState>()
            .init_resource::<registry::CommandRegistry>()
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

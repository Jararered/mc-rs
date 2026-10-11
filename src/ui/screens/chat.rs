//! Local Beta-style chat input and message overlay.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::text::FontSmoothing;
use bevy::text::LineHeight;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;
use bevy::window::PrimaryWindow;

use crate::app::settings::GameSettings;
use crate::app::state::AppScreen;
use crate::chat::ChatFocus;
use crate::chat::ChatHistory;
use crate::chat::ChatSet;
use crate::chat::ChatSubmission;
use crate::chat::registry::CommandRegistry;
use crate::chat::registry::Suggestions;
use crate::inventory::session::InventorySession;
use crate::ui::icons::overlay::UiFont;
use crate::world::tick::WorldTick;

const INPUT_LIMIT: usize = 100;
const HISTORY_LIMIT: usize = 50;
const MESSAGE_LINE_LIMIT: usize = 50;
const VISIBLE_CLOSED: usize = 10;
const VISIBLE_OPEN: usize = 20;
const FADE_TICKS: u32 = 200;
const SUGGESTION_ROWS: usize = 6;

pub struct ChatUiPlugin;

impl Plugin for ChatUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatState>()
            .init_resource::<GameSettings>()
            .init_resource::<ChatFocus>()
            .init_resource::<InventorySession>()
            .init_resource::<CommandRegistry>()
            .add_systems(OnEnter(AppScreen::Playing), spawn_chat)
            .add_systems(OnExit(AppScreen::Playing), despawn_chat)
            .add_systems(
                Update,
                read_chat_input
                    .in_set(ChatSet::Input)
                    .run_if(in_state(AppScreen::Playing)),
            )
            .add_systems(
                Update,
                (render_chat, render_suggestions)
                    .in_set(ChatSet::Presentation)
                    .run_if(in_state(AppScreen::Playing)),
            );
    }
}

#[derive(Resource, Default)]
struct ChatState {
    input: String,
    blink: u32,
    suggestions: Suggestions,
    selected: usize,
    /// First suggestion shown; follows `selected` through the visible rows.
    scroll: usize,
}

impl ChatState {
    fn refresh_suggestions(&mut self, registry: &CommandRegistry) {
        self.suggestions = registry.suggestions(&self.input);
        self.selected = 0;
        self.scroll = 0;
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        if index < self.scroll {
            self.scroll = index;
        } else if index >= self.scroll + SUGGESTION_ROWS {
            self.scroll = index + 1 - SUGGESTION_ROWS;
        }
    }

    /// Replace the word being typed, and add a space when the result takes
    /// another suggested argument so its list opens straight away.
    fn accept_suggestion(&mut self, registry: &CommandRegistry) {
        let Some(item) = self.suggestions.items.get(self.selected) else {
            return;
        };
        let mut input = format!("{}{item}", &self.input[..self.suggestions.start]);
        if !registry.suggestions(&format!("{input} ")).items.is_empty() {
            input.push(' ');
        }
        if input.chars().count() <= INPUT_LIMIT {
            self.input = input;
            self.refresh_suggestions(registry);
        }
    }
}

/// Beta's 320-GUI-pixel column, including Unicode-safe word wrapping.
fn wrap_message(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
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
        lines.push(rest[..split].trim_end().into());
        rest = &rest[split..];
    }
    lines
}

#[derive(Component)]
struct ChatRoot;
#[derive(Component)]
struct ChatInput;
#[derive(Component)]
struct ChatMessage(usize);
#[derive(Component)]
struct ChatSuggestions;
#[derive(Component)]
struct ChatSuggestion(usize);

fn spawn_chat(
    mut commands: Commands,
    font: Res<UiFont>,
    settings: Res<GameSettings>,
    mut chat: ResMut<ChatState>,
    mut focus: ResMut<ChatFocus>,
) {
    *chat = ChatState::default();
    *focus = ChatFocus::default();
    let scale = settings.gui_scale;
    let font = TextFont::from_font_size(8.0 * scale)
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
                        bottom: px((48.0 + index as f32 * 9.0) * scale),
                        max_width: px(320.0 * scale),
                        ..default()
                    },
                ));
            }
            root.spawn((
                ChatSuggestions,
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(2.0 * scale),
                    bottom: px(14.0 * scale),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(2.0 * scale), px(1.0 * scale)),
                    ..default()
                },
            ))
            .with_children(|list| {
                for index in 0..SUGGESTION_ROWS {
                    list.spawn((
                        ChatSuggestion(index),
                        Text::new(""),
                        TextLayout::no_wrap(),
                        font.clone(),
                        TextColor(Color::NONE),
                        Node {
                            height: px(9.0 * scale),
                            display: Display::None,
                            ..default()
                        },
                    ));
                }
            });
            root.spawn((
                ChatInput,
                Text::new(""),
                TextLayout::no_wrap(),
                font,
                LineHeight::Px(8.0 * scale),
                TextColor(Color::srgb_u8(224, 224, 224)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                Visibility::Hidden,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(2.0 * scale),
                    right: px(2.0 * scale),
                    bottom: px(2.0 * scale),
                    height: px(12.0 * scale),
                    // `GuiIngame` draws the bar at (2, h-14) and the text at
                    // (4, h-12): two pixels in from the left and down from the top.
                    padding: UiRect::new(px(2.0 * scale), Val::ZERO, px(2.0 * scale), Val::ZERO),
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
    mut focus: ResMut<ChatFocus>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    *chat = ChatState::default();
    *focus = ChatFocus::default();
}

fn read_chat_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut submissions: MessageWriter<ChatSubmission>,
    mut focus: ResMut<ChatFocus>,
    mut chat: ResMut<ChatState>,
    inventory: Res<InventorySession>,
    registry: Res<CommandRegistry>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    tick: Res<WorldTick>,
) {
    focus.suppress_controls = focus.open;
    chat.blink = chat.blink.wrapping_add(tick.ticks_this_frame());
    let Ok((window, mut cursor)) = windows.single_mut() else {
        keyboard.read().for_each(drop);
        return;
    };
    if !focus.open {
        // Drain the opening key's text: '/' is already prefilled and 't' must
        // not appear in the new input. Keyboard messages persist across frames.
        keyboard.read().for_each(drop);
        if !inventory.open && window.focused && cursor.grab_mode == CursorGrabMode::Locked {
            if keys.just_pressed(KeyCode::KeyT) || keys.just_pressed(KeyCode::Slash) {
                focus.open = true;
                focus.suppress_controls = true;
                chat.input = if keys.just_pressed(KeyCode::Slash) {
                    "/".into()
                } else {
                    String::new()
                };
                chat.blink = 0;
                chat.refresh_suggestions(&registry);
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
                focus.open = false;
                break;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let text = chat.input.trim();
                if !text.is_empty() {
                    submissions.write(ChatSubmission::new(text));
                }
                chat.input.clear();
                focus.open = false;
                break;
            }
            KeyCode::Backspace => {
                chat.input.pop();
                chat.refresh_suggestions(&registry);
            }
            KeyCode::ArrowDown | KeyCode::ArrowUp => {
                let count = chat.suggestions.items.len();
                if count > 0 {
                    let step = if event.key_code == KeyCode::ArrowDown {
                        1
                    } else {
                        count - 1
                    };
                    let index = (chat.selected + step) % count;
                    chat.select(index);
                }
            }
            KeyCode::Tab => chat.accept_suggestion(&registry),
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
                    chat.refresh_suggestions(&registry);
                }
            }
        }
    }
    if !focus.open {
        chat.refresh_suggestions(&registry);
        cursor.visible = false;
        if window.focused {
            cursor.grab_mode = CursorGrabMode::Locked;
        }
    }
}

fn render_chat(
    chat: Res<ChatState>,
    focus: Res<ChatFocus>,
    history: Res<ChatHistory>,
    mut rows: Local<Vec<(String, u32)>>,
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
        *visibility = if focus.open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if focus.open {
            let cursor = if chat.blink / 6 % 2 == 0 { "_" } else { "" };
            **text = format!("> {}{cursor}", chat.input);
        }
    }
    if history.is_changed() {
        rows.clear();
        for message in history.messages() {
            let wrapped = wrap_message(&message.text);
            for line in wrapped.into_iter().rev() {
                rows.push((line, message.age_ticks));
                if rows.len() == HISTORY_LIMIT {
                    break;
                }
            }
            if rows.len() == HISTORY_LIMIT {
                break;
            }
        }
    }
    for (index, mut text, mut color, mut background, mut visibility) in &mut messages {
        let Some((line, age)) = rows
            .get(index.0)
            .filter(|(_, age)| focus.open || index.0 < VISIBLE_CLOSED && *age < FADE_TICKS)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let opacity = if focus.open {
            1.0
        } else {
            let remaining = (FADE_TICKS - age) as f32 / FADE_TICKS as f32;
            (remaining * 10.0).min(1.0).powi(2)
        };
        **text = line.clone();
        color.0 = Color::srgba(1.0, 1.0, 1.0, opacity);
        background.0 = Color::srgba(0.0, 0.0, 0.0, opacity * 0.5);
        *visibility = Visibility::Visible;
    }
}

fn render_suggestions(
    chat: Res<ChatState>,
    focus: Res<ChatFocus>,
    mut list: Query<&mut Visibility, With<ChatSuggestions>>,
    mut rows: Query<(&ChatSuggestion, &mut Text, &mut TextColor, &mut Node)>,
) {
    let items = &chat.suggestions.items;
    let Ok(mut visibility) = list.single_mut() else {
        return;
    };
    *visibility = if focus.open && !items.is_empty() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (row, mut text, mut color, mut node) in &mut rows {
        let index = chat.scroll + row.0;
        let item = items.get(index).filter(|_| focus.open);
        let display = if item.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let Some(item) = item else {
            continue;
        };
        if **text != *item {
            **text = item.clone();
        }
        color.0 = if index == chat.selected {
            Color::srgb_u8(255, 255, 85)
        } else {
            Color::srgb_u8(170, 170, 170)
        };
    }
}

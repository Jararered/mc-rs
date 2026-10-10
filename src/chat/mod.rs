//! Chat submissions, history, and local command execution.
//!
//! Producers send `ChatSubmission`; command feedback and ordinary chat enter
//! `ChatHistory`. This plugin runs without a window, fonts, UI nodes, or a GPU.
//! A client adds `ChatUiPlugin` to provide keyboard input and an overlay.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::app::state::AppScreen;
use crate::physics::PhysicsSet;
use crate::world::tick::WorldTick;

pub mod commands;
mod execution;
pub mod registry;

const HISTORY_LIMIT: usize = 50;

#[derive(Message, Debug, Clone)]
pub struct ChatSubmission {
    pub text: String,
    /// The player who typed it. `None` is this client's own player.
    pub sender: Option<Entity>,
}

impl ChatSubmission {
    /// A line typed by this client's player.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            sender: None,
        }
    }

    /// A line typed by `sender`.
    pub fn from_player(sender: Entity, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            sender: Some(sender),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub text: String,
    pub age_ticks: u32,
}

/// Whole messages, newest first. Line wrapping and fading belong to the UI.
#[derive(Resource, Default)]
pub struct ChatHistory {
    messages: VecDeque<ChatMessage>,
}

impl ChatHistory {
    pub fn push(&mut self, text: impl Into<String>) {
        self.messages.push_front(ChatMessage {
            text: text.into(),
            age_ticks: 0,
        });
        self.messages.truncate(HISTORY_LIMIT);
    }

    pub fn messages(&self) -> impl Iterator<Item = &ChatMessage> {
        self.messages.iter()
    }
}

/// Input focus exposed independently of any screen widgets. Suppression
/// lasts through the closing frame so Enter/Escape cannot activate gameplay.
#[derive(Resource, Default)]
pub struct ChatFocus {
    pub open: bool,
    pub suppress_controls: bool,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChatSet {
    Input,
    Dispatch,
    Presentation,
}

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChatHistory>()
            .init_resource::<registry::CommandRegistry>()
            .add_message::<ChatSubmission>()
            .add_systems(OnEnter(AppScreen::Playing), reset_chat)
            .add_systems(OnExit(AppScreen::Playing), reset_chat)
            .configure_sets(
                Update,
                (ChatSet::Input, ChatSet::Dispatch, ChatSet::Presentation)
                    .chain()
                    .before(PhysicsSet::ApplyInput),
            )
            .add_systems(
                Update,
                (age_messages, execution::submit_chat)
                    .chain()
                    .in_set(ChatSet::Dispatch)
                    .run_if(chat_active),
            );
    }
}

fn chat_active(screen: Option<Res<State<AppScreen>>>) -> bool {
    screen.is_none_or(|screen| *screen.get() == AppScreen::Playing)
}

fn age_messages(tick: Res<WorldTick>, mut history: ResMut<ChatHistory>) {
    if tick.ticks_this_frame() == 0 {
        return;
    }
    for message in &mut history.messages {
        message.age_ticks = message.age_ticks.saturating_add(tick.ticks_this_frame());
    }
}

fn reset_chat(mut history: ResMut<ChatHistory>, mut submissions: ResMut<Messages<ChatSubmission>>) {
    *history = ChatHistory::default();
    submissions.clear();
}

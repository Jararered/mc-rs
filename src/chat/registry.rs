//! Owned command definitions shared by dispatch, usage errors, and help.

use bevy::prelude::Resource;

use super::commands::ChatCommand;
use super::commands::register_builtin_commands;

/// Return `Usage` for an unsupported argument shape; the registry supplies syntax.
pub enum CommandParseError {
    Usage,
    Invalid(String),
}

impl From<String> for CommandParseError {
    fn from(message: String) -> Self {
        Self::Invalid(message)
    }
}

impl From<&str> for CommandParseError {
    fn from(message: &str) -> Self {
        Self::Invalid(message.to_owned())
    }
}

pub type CommandParser = fn(&CommandRegistry, &[&str]) -> Result<ChatCommand, CommandParseError>;

/// Candidates for the argument after `args`. The registry filters them by what is typed.
pub type CommandCompleter = fn(&CommandRegistry, &[&str]) -> Vec<String>;

pub struct CommandDefinition {
    name: String,
    description: String,
    usages: Vec<String>,
    parser: CommandParser,
    completer: Option<CommandCompleter>,
}

/// Completions for the last word of a chat input. `start` is the byte offset
/// of that word, so accepting an item replaces `input[start..]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    pub start: usize,
    pub items: Vec<String>,
}

impl CommandDefinition {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn usages(&self) -> &[String] {
        &self.usages
    }
}

/// Initialized once by `ChatPlugin`. Registration order is the help display order.
#[derive(Resource)]
pub struct CommandRegistry {
    entries: Vec<CommandDefinition>,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        let mut registry = Self {
            entries: Vec::new(),
        };
        register_builtin_commands(&mut registry);
        registry
    }
}

impl CommandRegistry {
    /// Every command needs help metadata and a parser. Invalid or duplicate
    /// definitions leave the registry unchanged.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        usages: impl IntoIterator<Item = impl Into<String>>,
        parser: CommandParser,
    ) -> Result<(), String> {
        self.insert(name, description, usages, parser, None)
    }

    /// `register` with argument suggestions for the chat input.
    pub fn register_with_completions(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        usages: impl IntoIterator<Item = impl Into<String>>,
        parser: CommandParser,
        completer: CommandCompleter,
    ) -> Result<(), String> {
        self.insert(name, description, usages, parser, Some(completer))
    }

    fn insert(
        &mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        usages: impl IntoIterator<Item = impl Into<String>>,
        parser: CommandParser,
        completer: Option<CommandCompleter>,
    ) -> Result<(), String> {
        let name = name.into();
        let description = description.into();
        let usages: Vec<String> = usages.into_iter().map(Into::into).collect();
        if name.is_empty() || name.contains('/') || name.chars().any(char::is_whitespace) {
            return Err("Command names must be nonempty words without a slash".into());
        }
        if description.trim().is_empty()
            || usages.is_empty()
            || usages
                .iter()
                .any(|usage| usage.split_whitespace().next() != Some(format!("/{name}").as_str()))
        {
            return Err(
                "Commands require a description and usage forms starting with their name".into(),
            );
        }
        if self.get(&name).is_some() {
            return Err(format!("Command already registered: {name}"));
        }
        self.entries.push(CommandDefinition {
            name,
            description,
            usages,
            parser,
            completer,
        });
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CommandDefinition> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn parse(&self, text: &str) -> Result<ChatCommand, String> {
        let mut parts = text.split_whitespace();
        let name = parts.next().unwrap_or("");
        let entry = name
            .strip_prefix('/')
            .and_then(|name| self.get(name))
            .ok_or_else(|| format!("Unknown command: {name}. Try /help"))?;
        let args: Vec<_> = parts.collect();
        (entry.parser)(self, &args).map_err(|error| match error {
            CommandParseError::Usage => format!("Usage: {}", entry.usages.join(" | ")),
            CommandParseError::Invalid(message) => message,
        })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    /// What could complete the word being typed: command names until the first
    /// space, then the command's next argument. Ordinary chat has none.
    pub fn suggestions(&self, input: &str) -> Suggestions {
        let Some(body) = input.strip_prefix('/') else {
            return Suggestions::default();
        };
        let start = input.rfind(char::is_whitespace).map_or(0, |space| {
            space + input[space..].chars().next().map_or(1, char::len_utf8)
        });
        if start == 0 {
            let mut items: Vec<String> = self
                .names()
                .filter(|name| starts_with_ignore_case(name, body))
                .map(|name| format!("/{name}"))
                .collect();
            items.sort();
            return Suggestions { start, items };
        }
        let mut words = input[..start].split_whitespace();
        let Some(entry) = words
            .next()
            .and_then(|name| self.get(name.trim_start_matches('/')))
        else {
            return Suggestions::default();
        };
        let Some(completer) = entry.completer else {
            return Suggestions::default();
        };
        let args: Vec<_> = words.collect();
        let partial = &input[start..];
        let mut items = completer(self, &args);
        items.retain(|item| starts_with_ignore_case(item, partial));
        Suggestions { start, items }
    }

    /// General help lists syntax; targeted help also explains the command.
    pub fn help(&self, filter: Option<&str>) -> Vec<String> {
        let mut lines = Vec::new();
        for entry in &self.entries {
            if filter.is_none_or(|name| name == entry.name) {
                if filter.is_some() {
                    lines.push(entry.description.clone());
                }
                lines.extend(entry.usages.iter().cloned());
            }
        }
        lines
    }
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

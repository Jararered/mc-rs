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

pub struct CommandDefinition {
    name: String,
    description: String,
    usages: Vec<String>,
    parser: CommandParser,
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

//! Slash command registry (`core/slash-commands.ts`).

use pi_js_compat::js_json::JsString;
use serde::{Deserialize, Serialize};

use crate::config::package_config;
use crate::core::source_info::SourceInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlashCommandSource {
    Extension,
    Prompt,
    Skill,
}

impl SlashCommandSource {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Extension => "extension",
            Self::Prompt => "prompt",
            Self::Skill => "skill",
        }
    }
}

/// A slash command contributed by an extension, prompt template, or skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandInfo {
    pub name: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<JsString>,
    pub source: SlashCommandSource,
    pub source_info: SourceInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinSlashCommand {
    pub name: &'static str,
    pub description: JsString,
    pub argument_hint: Option<&'static str>,
}

impl BuiltinSlashCommand {
    fn new(name: &'static str, description: &'static str, argument_hint: Option<&'static str>) -> Self {
        Self {
            name,
            description: description.into(),
            argument_hint,
        }
    }
}

/// The built-in command list in declaration order.
pub fn builtin_slash_commands() -> Vec<BuiltinSlashCommand> {
    let app_name = &package_config().app_name;
    vec![
        BuiltinSlashCommand::new("settings", "Open settings menu", None),
        BuiltinSlashCommand::new("model", "Select model (opens selector UI)", None),
        BuiltinSlashCommand::new("scoped-models", "Enable/disable models for Ctrl+P cycling", None),
        BuiltinSlashCommand::new(
            "export",
            "Export session (HTML default, or specify path: .html/.jsonl)",
            None,
        ),
        BuiltinSlashCommand::new("import", "Import and resume a session from a JSONL file", None),
        BuiltinSlashCommand::new("share", "Share session as a secret GitHub gist", None),
        BuiltinSlashCommand::new("copy", "Copy last agent message to clipboard", None),
        BuiltinSlashCommand::new("cd", "Move the current session to another working directory", None),
        BuiltinSlashCommand::new("name", "Set session display name", None),
        BuiltinSlashCommand::new("unname", "Remove session display name", None),
        BuiltinSlashCommand::new("session", "Show session info and stats", None),
        BuiltinSlashCommand::new("tools", "Show available tools", None),
        BuiltinSlashCommand::new("extensions", "Show loaded extensions", None),
        BuiltinSlashCommand::new("changelog", "Show changelog entries", None),
        BuiltinSlashCommand::new("hotkeys", "Show all keyboard shortcuts", None),
        BuiltinSlashCommand::new("fork", "Create a new fork from a previous user message", None),
        BuiltinSlashCommand::new("clone", "Duplicate the current session at the current position", None),
        BuiltinSlashCommand::new("tree", "Navigate session tree (switch branches)", None),
        BuiltinSlashCommand::new("trust", "Save project trust decision for future sessions", None),
        BuiltinSlashCommand::new("login", "Configure provider authentication", None),
        BuiltinSlashCommand::new("logout", "Remove provider authentication", None),
        BuiltinSlashCommand::new("new", "Start a new session", None),
        BuiltinSlashCommand::new(
            "continue",
            "Continue from the current transcript without adding a user message",
            None,
        ),
        BuiltinSlashCommand::new("compact", "Manually compact the session context", None),
        BuiltinSlashCommand::new("resume", "Resume a different session", None),
        BuiltinSlashCommand::new("agents", "Open agent switcher", None),
        BuiltinSlashCommand::new(
            "reload",
            "Reload keybindings, extensions, skills, prompts, and themes",
            None,
        ),
        BuiltinSlashCommand {
            name: "quit",
            description: pi_js_compat::js_format!("Quit {}", app_name),
            argument_hint: None,
        },
    ]
}

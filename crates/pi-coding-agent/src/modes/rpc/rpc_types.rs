//! RPC protocol wire types (`modes/rpc/rpc-types.ts`).
//!
//! Commands arrive as JSON lines on stdin; responses, extension UI requests and
//! session events leave as JSON lines on stdout. Field order follows the
//! TypeScript object literals so serialized lines stay byte-identical.

pub use pi_agent_core::harness::QueueMode;
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::{ImageContent, Model};
use pi_js_compat::js_json::{JsString, JsValue};
use serde::{Deserialize, Serialize};

pub use crate::core::extensions::types::{NotifyType, StreamingBehavior, WidgetPlacement};
use crate::core::slash_commands::{SlashCommandInfo, SlashCommandSource};

/// `RpcSlashCommand` has the same shape as the shared slash-command record.
pub type RpcSlashCommand = SlashCommandInfo;
/// `RpcSlashCommand["source"]` is the shared slash-command source.
pub type RpcSlashCommandSource = SlashCommandSource;

/// An RPC command body, i.e. `RpcCommand` without the correlation id.
///
/// `DistributiveOmit<RpcCommand, "id">` in TypeScript: the client sends these
/// bodies and appends `id` after the body fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum RpcCommandBody {
    // Prompting
    Prompt {
        message: JsString,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        streaming_behavior: Option<StreamingBehavior>,
    },
    Steer {
        message: JsString,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
    },
    FollowUp {
        message: JsString,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
    },
    Abort,
    Interrupt,
    NewSession {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },

    // State
    GetState,

    // Model
    SetModel {
        provider: String,
        model_id: String,
    },
    CycleModel,
    GetAvailableModels,

    // Thinking
    SetThinkingLevel {
        level: ThinkingLevel,
    },
    CycleThinkingLevel,
    GetAvailableThinkingLevels,

    // Queue modes
    SetSteeringMode {
        mode: QueueMode,
    },
    SetFollowUpMode {
        mode: QueueMode,
    },

    // Compaction
    Compact {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<JsString>,
    },
    SetAutoCompaction {
        enabled: bool,
    },

    // Retry
    SetAutoRetry {
        enabled: bool,
    },
    AbortRetry,

    // Bash
    Bash {
        command: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclude_from_context: Option<bool>,
    },
    AbortBash,

    // Session
    GetSessionStats,
    ExportHtml {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_path: Option<String>,
    },
    SwitchSession {
        session_path: String,
    },
    Fork {
        entry_id: String,
    },
    Clone,
    GetForkMessages,
    GetEntries {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since: Option<String>,
    },
    GetTree,
    GetLastAssistantText,
    SetSessionName {
        name: JsString,
    },

    // Messages
    GetMessages,

    // Commands
    GetCommands,
}

impl RpcCommandBody {
    /// The TypeScript `command.type` discriminator.
    pub const fn command_type(&self) -> &'static str {
        match self {
            Self::Prompt { .. } => "prompt",
            Self::Steer { .. } => "steer",
            Self::FollowUp { .. } => "follow_up",
            Self::Abort => "abort",
            Self::Interrupt => "interrupt",
            Self::NewSession { .. } => "new_session",
            Self::GetState => "get_state",
            Self::SetModel { .. } => "set_model",
            Self::CycleModel => "cycle_model",
            Self::GetAvailableModels => "get_available_models",
            Self::SetThinkingLevel { .. } => "set_thinking_level",
            Self::CycleThinkingLevel => "cycle_thinking_level",
            Self::GetAvailableThinkingLevels => "get_available_thinking_levels",
            Self::SetSteeringMode { .. } => "set_steering_mode",
            Self::SetFollowUpMode { .. } => "set_follow_up_mode",
            Self::Compact { .. } => "compact",
            Self::SetAutoCompaction { .. } => "set_auto_compaction",
            Self::SetAutoRetry { .. } => "set_auto_retry",
            Self::AbortRetry => "abort_retry",
            Self::Bash { .. } => "bash",
            Self::AbortBash => "abort_bash",
            Self::GetSessionStats => "get_session_stats",
            Self::ExportHtml { .. } => "export_html",
            Self::SwitchSession { .. } => "switch_session",
            Self::Fork { .. } => "fork",
            Self::Clone => "clone",
            Self::GetForkMessages => "get_fork_messages",
            Self::GetEntries { .. } => "get_entries",
            Self::GetTree => "get_tree",
            Self::GetLastAssistantText => "get_last_assistant_text",
            Self::SetSessionName { .. } => "set_session_name",
            Self::GetMessages => "get_messages",
            Self::GetCommands => "get_commands",
        }
    }
}

/// A command line with its optional correlation id.
///
/// Serialization emits the body fields first and `id` last, matching the
/// client's `{ ...command, id }` object literal. `id` is omitted when absent,
/// matching `JSON.stringify` dropping `undefined`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcCommand {
    #[serde(flatten)]
    pub body: RpcCommandBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl RpcCommand {
    pub const fn new(body: RpcCommandBody, id: Option<String>) -> Self {
        Self { body, id }
    }
}

/// Session state returned by `get_state` (`RpcSessionState`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSessionState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Model>,
    pub thinking_level: ThinkingLevel,
    pub is_streaming: bool,
    pub is_compacting: bool,
    pub steering_mode: QueueMode,
    pub follow_up_mode: QueueMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_file: Option<String>,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<JsString>,
    pub auto_compaction_enabled: bool,
    pub message_count: u64,
    pub pending_message_count: u64,
}

/// `type` discriminator of an RPC response line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcResponseTag {
    #[default]
    #[serde(rename = "response")]
    Response,
}

/// One stdout response line.
///
/// The TypeScript type is a discriminated union over `command`; the wire shape
/// only carries an opaque `data` payload, so the Rust envelope keeps it as JSON
/// and typed client accessors deserialize the payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: RpcResponseTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
}

impl RpcResponse {
    /// `success(id, command)` without a `data` field.
    pub fn success(id: Option<String>, command: impl Into<String>) -> Self {
        Self {
            id,
            kind: RpcResponseTag::Response,
            command: Some(command.into()),
            success: true,
            data: None,
            error: None,
        }
    }

    /// `success(id, command, data)`; `data` may be `Value::Null` to emit an explicit `null`.
    pub fn success_with_data(id: Option<String>, command: impl Into<String>, data: JsValue) -> Self {
        Self {
            id,
            kind: RpcResponseTag::Response,
            command: Some(command.into()),
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// `error(id, command, message)`.
    pub fn error(id: Option<String>, command: impl Into<String>, message: impl Into<JsString>) -> Self {
        Self::error_optional(id, Some(command.into()), message)
    }

    /// `error(id, command, message)` with an absent `command` (unknown or
    /// non-string `type`), which `JSON.stringify` omits.
    pub fn error_optional(id: Option<String>, command: Option<String>, message: impl Into<JsString>) -> Self {
        Self {
            id,
            kind: RpcResponseTag::Response,
            command,
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Returns whether `name` is part of the RPC command union.
pub fn is_known_command_type(name: &str) -> bool {
    COMMAND_TYPES.contains(&name)
}

/// Every `RpcCommand["type"]` discriminator in declaration order.
pub const COMMAND_TYPES: [&str; 33] = [
    "prompt",
    "steer",
    "follow_up",
    "abort",
    "interrupt",
    "new_session",
    "get_state",
    "set_model",
    "cycle_model",
    "get_available_models",
    "set_thinking_level",
    "cycle_thinking_level",
    "get_available_thinking_levels",
    "set_steering_mode",
    "set_follow_up_mode",
    "compact",
    "set_auto_compaction",
    "set_auto_retry",
    "abort_retry",
    "bash",
    "abort_bash",
    "get_session_stats",
    "export_html",
    "switch_session",
    "fork",
    "clone",
    "get_fork_messages",
    "get_entries",
    "get_tree",
    "get_last_assistant_text",
    "set_session_name",
    "get_messages",
    "get_commands",
];

/// `type` discriminator of an extension UI request line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionUiRequestTag {
    #[default]
    #[serde(rename = "extension_ui_request")]
    ExtensionUiRequest,
}

/// Extension UI request payload, tagged by `method`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RpcExtensionUiRequestPayload {
    Select {
        title: String,
        options: Vec<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        timeout: Option<f64>,
    },
    Confirm {
        title: String,
        message: String,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        timeout: Option<f64>,
    },
    Input {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            serialize_with = "pi_js_compat::number::serialize_option"
        )]
        timeout: Option<f64>,
    },
    Editor {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefill: Option<String>,
    },
    Notify {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notify_type: Option<NotifyType>,
    },
    SetStatus {
        status_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status_text: Option<String>,
    },
    SetWidget {
        widget_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widget_lines: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        widget_placement: Option<WidgetPlacement>,
    },
    SetTitle {
        title: String,
    },
    #[serde(rename = "set_editor_text")]
    SetEditorText {
        text: String,
    },
}

/// An extension UI request line (`RpcExtensionUIRequest`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcExtensionUiRequest {
    #[serde(rename = "type")]
    pub kind: ExtensionUiRequestTag,
    pub id: String,
    #[serde(flatten)]
    pub payload: RpcExtensionUiRequestPayload,
}

/// `type` discriminator of an extension UI response line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtensionUiResponseTag {
    #[default]
    #[serde(rename = "extension_ui_response")]
    ExtensionUiResponse,
}

/// Extension UI response payload: exactly one of `value`, `confirmed`, `cancelled`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcExtensionUiResponsePayload {
    Value { value: String },
    Confirmed { confirmed: bool },
    Cancelled { cancelled: bool },
}

/// A client answer to an extension UI request (`RpcExtensionUIResponse`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcExtensionUiResponse {
    #[serde(rename = "type")]
    pub kind: ExtensionUiResponseTag,
    pub id: String,
    #[serde(flatten)]
    pub payload: RpcExtensionUiResponsePayload,
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn rpc_prompt_and_error_keep_unpaired_utf16_on_wire() {
        let raw = pi_js_compat::js_json::parse_lossless(r#"{"type":"prompt","message":"\ud800"}"#).unwrap();
        let command: RpcCommand = pi_js_compat::js_json::from_value(&raw).unwrap();
        let RpcCommandBody::Prompt { message, .. } = command.body else {
            panic!("prompt")
        };
        assert_eq!(message.units(), &[0xd800]);
        let response = RpcResponse::error(None, "prompt", message);
        let wire = crate::modes::rpc::serialize_json_line(&response).unwrap();
        assert!(wire.contains(r#""error":"\ud800""#));
    }
}

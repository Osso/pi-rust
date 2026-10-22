//! `channel_post` tool (`core/tools/channel-post.ts`).

use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_control_db::{
    PostSharedChannelMessageInput, RuntimeMailboxAddress, advance_shared_channel_cursor, post_shared_channel_message,
};
use crate::core::session_manager::lock_session_manager;
use crate::modes::interactive::theme::{Theme, ThemeColor};

pub const NAME: &str = "channel_post";
pub const LABEL: &str = "channel_post";
pub const DESCRIPTION: &str = "Append one action-required coordination broadcast to the global shared channel. Idle sessions read new channel messages from control.sqlite using per-session cursors.";
pub const PROMPT_SNIPPET: &str = "Post an action-required broadcast to the global shared channel";
pub const PROMPT_GUIDELINES: [&str; 7] = [
    "Post only when other sessions must take a concrete action now. Treat channel_post as a low-volume broadcast bus, not a conversation; valid cases are an already-shared checkout collision, blocker, or release, a blocking dependency, or an installed Pi runtime replacement.",
    "Include the exact affected shared path or installed artifact and required action. Keep each post to one short sentence when possible, two maximum. Omit task descriptions, planned file inventories, implementation scope, rationale, progress, and deployment narration unless required for safe action.",
    "Do not post diagnostics, tests, acknowledgements, restatements, reviews, or status updates. This includes experiments, delivery or echo/tag tests, liveness probes, architecture requests, evidence gathering, falsification checks, untouched or no-overlap reports, no-action updates, and conversational replies.",
    "Do not post isolated-worktree ownership or release; isolation already prevents conflicts. Correct a prior post only when the required action changed.",
    "Prefer send_agent_message for targeted coordination. If independent main sessions lack a direct mailbox path, use one minimal targeted channel post rather than a conversation.",
    "For urgent targeted wakeups, use broadcast with explicit filters instead.",
    "For Pi runtime replacements, identify the installed artifact and tell affected sessions to call restart_self.",
];

#[derive(Debug, Deserialize)]
struct ChannelPostParams {
    message: JsString,
}

/// `createChannelPostToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChannelPostTool;

impl ChannelPostTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static Value {
    static PARAMETERS: OnceLock<Value> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "One short action-required broadcast naming the exact affected shared path or installed artifact and required action."
                }
            },
            "required": ["message"]
        })
    })
}

impl ContextTool<ExtensionContext> for ChannelPostTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> &str {
        LABEL
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let agent_id = context
                .multi_agent_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            let requires_agent_id = context
                .multi_agent_requires_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            if agent_id.is_some() || requires_agent_id {
                return Err(ToolError::new("channel_post is only available from main sessions"));
            }
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("channel_post requires a control database path"))?;
            let params: ChannelPostParams =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let message = params.message.trim();
            if message.is_empty() {
                return Err(ToolError::new("channel_post requires a non-empty message"));
            }
            let session_manager = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let session_id = lock_session_manager(&session_manager).get_session_id().to_owned();
            let sender = RuntimeMailboxAddress {
                agent_id: agent_id.map(Into::into),
                session_id: session_id.into(),
            };
            let message_id = post_shared_channel_message(
                &control_db_path,
                &PostSharedChannelMessageInput {
                    body: message.to_owned(),
                    sender: sender.clone(),
                },
            )
            .map_err(|error| ToolError::new(error.to_string()))?;
            // The sender already knows what it wrote; skip self-echo on the next idle drain.
            advance_shared_channel_cursor(&control_db_path, &sender, message_id)
                .map_err(|error| ToolError::new(error.to_string()))?;
            Ok(text_result(
                format!("Posted shared channel message {message_id}."),
                json!({ "messageId": message_id }),
            ))
        })
    }
}

fn format_call(theme: &Theme) -> JsString {
    theme.fg(ThemeColor::ToolTitle, &theme.bold("channel_post"))
}

/// `renderCall` for the channel_post tool.
pub fn render_call(_args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_call(theme))
}

/// `renderResult` for the channel_post tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `create_channel_post_tool_definition()`.
pub fn create_channel_post_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::context_tool_definition(Arc::new(ChannelPostTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

//! `channel_post` tool (`core/tools/channel-post.ts`).

use std::sync::{Arc, OnceLock};

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use pi_tui::tui::ComponentRef;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::result::text_result;
use crate::core::extensions::types::{ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::core::session_control_db::{
    PostSharedChannelMessageInput, RuntimeMailboxAddress, advance_shared_channel_cursor, post_shared_channel_message,
};
use crate::modes::interactive::theme::{Theme, ThemeColor};

pub const NAME: &str = "channel_post";
pub const LABEL: &str = "channel_post";
pub const DESCRIPTION: &str = "Append one message to the global shared channel. Idle sessions read new channel messages from control.sqlite using per-session cursors.";
pub const PROMPT_SNIPPET: &str = "Post a message to the global shared channel";
pub const PROMPT_GUIDELINES: [&str; 2] = [
    "Use channel_post for soft cross-session coordination; idle sessions catch up from their cursor.",
    "Do not use channel_post for urgent targeted wakeups; use broadcast with explicit filters for that.",
];

#[derive(Debug, Deserialize)]
struct ChannelPostParams {
    message: String,
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
                    "description": "Message body to append to the global shared session channel."
                }
            },
            "required": ["message"]
        })
    })
}

impl AgentHarnessTool<ExtensionContext> for ChannelPostTool {
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
        params: Value,
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
                serde_json::from_value(params).map_err(|error| ToolError::new(error.to_string()))?;
            let message = js_trim(&params.message);
            if message.is_empty() {
                return Err(ToolError::new("channel_post requires a non-empty message"));
            }
            let session_id = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?
                .get_session_id()
                .to_owned();
            let sender = RuntimeMailboxAddress { agent_id, session_id };
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

fn format_call(theme: &Theme) -> String {
    theme.fg(ThemeColor::ToolTitle, &theme.bold("channel_post"))
}

/// `renderCall` for the channel_post tool.
pub fn render_call(_args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
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
    let mut definition = crate::core::extensions::wrapper::harness_tool_definition(Arc::new(ChannelPostTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

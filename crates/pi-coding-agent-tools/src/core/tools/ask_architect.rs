//! `ask_architect` tool (`core/tools/ask-architect.ts`).

use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use std::sync::{Arc, OnceLock};

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_tui::tui::ComponentRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::result::text_result;
use crate::core::extensions::types::{ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::core::extensions::wrapper::context_tool_definition;
use crate::core::session_control_db::{PostArchitectRequestInput, post_architect_request};
use crate::modes::interactive::theme::{Theme, ThemeColor};

pub const NAME: &str = "ask_architect";
pub const LABEL: &str = "ask_architect";
pub const DESCRIPTION: &str = "Queue a durable request for the resident Architect.";
pub const PROMPT_SNIPPET: &str = "Ask the resident Architect";
pub const PROMPT_GUIDELINES: [&str; 3] = [
    "Use this tool for direct Architect requests; do not use channel_post for Architect requests.",
    "Set project_path to the project directory, such as the active worktree, when it differs from the session cwd.",
    "Requests remain queued across Architect restarts until processed.",
];

#[derive(Debug, Deserialize)]
pub struct AskArchitectToolInput {
    pub message: JsString,
    pub project_path: Option<JsString>,
}

/// `AskArchitectToolDetails`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskArchitectToolDetails {
    pub request_id: i64,
    pub sender_session_id: JsString,
}

/// `createAskArchitectToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AskArchitectTool;

impl AskArchitectTool {
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
                    "description": "Question or diagnostic request for the resident Architect."
                },
                "project_path": {
                    "type": "string",
                    "description": "Absolute project directory, such as the active worktree. Defaults to the session cwd."
                }
            },
            "required": ["message"]
        })
    })
}

fn resolve_project_path(project_path: Option<&JsString>, session_cwd: &JsString) -> Result<JsString, ToolError> {
    let Some(project_path) = project_path else {
        return Ok(session_cwd.to_owned());
    };
    if project_path.trim().is_empty() || !crate::utils::paths::node_is_absolute(project_path) {
        return Err(ToolError::new(
            "ask_architect project_path must be a non-empty absolute path",
        ));
    }
    let canonical_path = pi_js_compat::node_fs::realpath_sync_native(project_path)
        .map_err(|_| ToolError::new("ask_architect project_path must reference an existing directory"))?;
    if !std::path::Path::new(&encode_node_utf8(&canonical_path)).is_dir() {
        return Err(ToolError::new("ask_architect project_path must reference a directory"));
    }
    Ok(canonical_path)
}

impl ContextTool<Option<ExtensionContext>> for AskArchitectTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &Value {
        parameters()
    }

    fn label(&self) -> pi_js_compat::js_json::JsString {
        LABEL.into()
    }

    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        Some(ToolExecutionMode::Sequential)
    }

    fn execute(
        &self,
        _tool_call_id: String,
        params: JsValue,
        _signal: Option<AbortSignal>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: Option<ExtensionContext>,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let context = context.ok_or_else(|| ToolError::new("ask_architect requires a control database path"))?;
            let agent_id = context
                .multi_agent_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            let requires_agent_id = context
                .multi_agent_requires_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            if agent_id.is_some() || requires_agent_id {
                return Err(ToolError::new("ask_architect is only available from main sessions"));
            }
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?
                .ok_or_else(|| ToolError::new("ask_architect requires a control database path"))?;
            let session_manager = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let sender_session_id = session_manager.get_session_id().to_owned();
            if sender_session_id.is_empty() {
                return Err(ToolError::new("ask_architect requires a persisted session id"));
            }
            let params: AskArchitectToolInput =
                pi_js_compat::js_json::from_value(&params).map_err(|error| ToolError::new(error.to_string()))?;
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let project_cwd = resolve_project_path(params.project_path.as_ref(), &cwd)?;
            let request_id = post_architect_request(
                &control_db_path,
                &PostArchitectRequestInput {
                    sender_session_id: sender_session_id.to_owned(),
                    project_cwd,
                    body: params.message,
                },
            )
            .map_err(|error| ToolError::new(error.to_string()))?;
            Ok(text_result(
                format!("Architect request queued: {request_id}"),
                js_json::to_value(&AskArchitectToolDetails {
                    request_id,
                    sender_session_id: sender_session_id.clone(),
                })
                .expect("architect request details must serialize"),
            ))
        })
    }
}

/// `renderCall` for the ask_architect tool.
pub fn render_call(_args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, theme.fg(ThemeColor::ToolTitle, theme.bold(NAME)))
}

/// `renderResult` for the ask_architect tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `createAskArchitectToolDefinition()`.
pub fn create_ask_architect_tool_definition() -> ToolDefinition {
    let mut definition = context_tool_definition(Arc::new(AskArchitectTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(crate::core::extensions::types::infallible_render_call(render_call));
    definition.render_result = Some(crate::core::extensions::types::infallible_render_result(render_result));
    definition
}

//! `change_working_directory` tool (`core/tools/change-working-directory.ts`).

use std::path::Path;
use std::sync::{Arc, OnceLock};

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use pi_tui::tui::ComponentRef;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;

use super::result::terminating_text_result;
use super::resume_session::{find_session_file_by_id, resolve_session_path};
use crate::core::extensions::types::{
    ExtensionContext, ToolDefinition, ToolRenderContext, ToolRenderResultOptions, infallible_render_call,
    infallible_render_result,
};
use crate::core::session_manager::lock_session_manager;
use crate::core::session_manager::{SessionManager, read_session_header};
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::paths::{PathInputOptions, resolve_path};

pub const NAME: &str = "change_working_directory";
pub const LABEL: &str = "change_working_directory";
pub const DESCRIPTION: &str = "Change the current Pi session working directory without switching session identity. Pass a directory path, or a Pi session ID to adopt that session's recorded cwd. The referenced session is not modified.";
pub const PROMPT_SNIPPET: &str =
    "Change the current session working directory by path or another session's recorded cwd";
pub const PROMPT_GUIDELINES: [&str; 2] = [
    "Use change_working_directory when subsequent relative tool operations should run from a different project directory.",
    "Pass exactly one of path or id. An id selects only that session's recorded cwd; it does not resume or modify the referenced session.",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChangeWorkingDirectoryTarget {
    Path(String),
    Id(String),
}

struct ResolvedTarget {
    cwd: String,
    source: &'static str,
    session_id: Option<String>,
}

/// `createChangeWorkingDirectoryToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ChangeWorkingDirectoryTool;

impl ChangeWorkingDirectoryTool {
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
                "path": {
                    "type": "string",
                    "description": "Directory path to adopt as the current session cwd."
                },
                "id": {
                    "type": "string",
                    "description": "Pi session ID whose recorded cwd should be adopted."
                }
            }
        })
    })
}

fn read_non_empty_target(record: &Map<String, Value>, key: &str) -> Result<Option<String>, ToolError> {
    let Some(value) = record.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(text) = value.as_str() else {
        return Err(ToolError::new(format!(
            "change_working_directory {key} must be a non-empty string"
        )));
    };
    if js_trim(text).is_empty() {
        return Err(ToolError::new(format!(
            "change_working_directory {key} must be a non-empty string"
        )));
    }
    Ok(Some(text.to_owned()))
}

fn normalize_target(params: &Value) -> Result<ChangeWorkingDirectoryTarget, ToolError> {
    let Some(record) = params.as_object() else {
        return Err(ToolError::new(
            "change_working_directory requires exactly one of path or id",
        ));
    };
    let path = read_non_empty_target(record, "path")?;
    let id = read_non_empty_target(record, "id")?.map(|id| js_trim(&id).to_owned());
    match (path, id) {
        (Some(path), None) => Ok(ChangeWorkingDirectoryTarget::Path(path)),
        (None, Some(id)) => Ok(ChangeWorkingDirectoryTarget::Id(id)),
        _ => Err(ToolError::new(
            "change_working_directory requires exactly one of path or id",
        )),
    }
}

fn assert_existing_directory(cwd: &str) -> Result<(), ToolError> {
    let path = Path::new(cwd);
    if !path.exists() {
        return Err(ToolError::new(format!("Directory does not exist: {cwd}")));
    }
    if !path.is_dir() {
        return Err(ToolError::new(format!("Not a directory: {cwd}")));
    }
    Ok(())
}

fn load_target_cwd(
    target: &ChangeWorkingDirectoryTarget,
    cwd: &str,
    session: &SessionManager,
    control_db_path: Option<&str>,
) -> Result<ResolvedTarget, ToolError> {
    match target {
        ChangeWorkingDirectoryTarget::Path(path) => {
            let cwd = resolve_path(path, cwd, &PathInputOptions::default())
                .map_err(|error| ToolError::new(error.to_string()))?;
            assert_existing_directory(&cwd)?;
            Ok(ResolvedTarget {
                cwd,
                source: "path",
                session_id: None,
            })
        }
        ChangeWorkingDirectoryTarget::Id(id) => {
            let session_path = find_session_file_by_id(id, session, control_db_path)?;
            if let Some(current_session_file) = session.get_session_file()
                && resolve_session_path(current_session_file) == resolve_session_path(&session_path)
            {
                return Err(ToolError::new(
                    "current session id cannot be used as a working directory target",
                ));
            }
            let cwd = read_session_header(&session_path)
                .and_then(|header| header.cwd)
                .filter(|cwd| !cwd.is_empty())
                .ok_or_else(|| {
                    ToolError::new(format!("Session does not record a working directory: {session_path}"))
                })?;
            assert_existing_directory(&cwd)?;
            Ok(ResolvedTarget {
                cwd,
                source: "session",
                session_id: Some(id.clone()),
            })
        }
    }
}

impl AgentHarnessTool<ExtensionContext> for ChangeWorkingDirectoryTool {
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
        tool_call_id: String,
        params: Value,
        _signal: Option<CancellationToken>,
        _on_update: Option<AgentToolUpdateCallback>,
        context: ExtensionContext,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            let target = normalize_target(&params)?;
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?;
            let previous_cwd = cwd.clone();
            let resolved = load_target_cwd(
                &target,
                &cwd,
                &lock_session_manager(&session),
                control_db_path.as_deref(),
            )?;
            context
                .relocate_after_tool_result(&tool_call_id, &resolved.cwd)
                .map_err(|error| ToolError::new(error.message))?;
            let mut details = Map::new();
            details.insert("previousCwd".to_owned(), Value::String(previous_cwd));
            details.insert("cwd".to_owned(), Value::String(resolved.cwd.clone()));
            details.insert("source".to_owned(), Value::String(resolved.source.to_owned()));
            if let Some(session_id) = resolved.session_id {
                details.insert("sessionId".to_owned(), Value::String(session_id));
            }
            Ok(terminating_text_result(
                format!("Changed working directory to {}", resolved.cwd),
                Value::Object(details),
            ))
        })
    }
}

fn format_call(args: &Value, theme: &Theme) -> String {
    let target = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(|path| format!("path {path}"))
        .or_else(|| {
            args.get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(|id| format!("session {id}"))
        })
        .unwrap_or_else(|| "target".to_owned());
    format!(
        "{} {}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("change_working_directory")),
        theme.fg(ThemeColor::Accent, &target)
    )
}

/// `renderCall` for the change_working_directory tool.
pub fn render_call(args: &Value, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_call(args, theme))
}

/// `renderResult` for the change_working_directory tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `createChangeWorkingDirectoryToolDefinition()`.
pub fn create_change_working_directory_tool_definition() -> ToolDefinition {
    let mut definition =
        crate::core::extensions::wrapper::harness_tool_definition(Arc::new(ChangeWorkingDirectoryTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

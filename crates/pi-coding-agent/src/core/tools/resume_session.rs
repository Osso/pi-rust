//! `resume_session` tool (`core/tools/resume-session.ts`).

use pi_js_compat::js_json::JsValue;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use pi_agent_core::{
    AgentHarnessTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;
use tokio_util::sync::CancellationToken;

use std::sync::Arc;

use super::result::terminating_text_result;
use crate::core::extensions::types::{
    ExtensionContext, SwitchSessionOptions, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
    UserMessageContent, infallible_render_call, infallible_render_result,
};
use crate::core::session_control_db::{
    SessionMetadata, find_active_session_metadata_by_id, find_active_session_metadata_by_name,
};
use crate::core::session_manager::{SessionInfo, SessionManager, lock_session_manager};
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::paths::{PathInputOptions, resolve_path};

pub const NAME: &str = "resume_session";
pub const LABEL: &str = "resume_session";
pub const DESCRIPTION: &str = "Switch/resume the current main Pi session to another session file. This replaces the current supervisor context; only use when the user explicitly asks to resume or switch sessions. Optionally sends caller-provided instructions after the target session is active.";
pub const PROMPT_SNIPPET: &str =
    "Switch/resume the current main Pi session, optionally with instructions for the resumed session";
pub const PROMPT_GUIDELINES: [&str; 2] = [
    "Use resume_session only when the user explicitly asks to resume or switch the main session; it replaces the current supervisor context.",
    "Pass exactly one of path, id, or name. Use starter_prompt for caller-provided instructions to send after the target session is active.",
];

#[derive(Debug, Clone, Default)]
struct ResumeSessionParams {
    id: Option<String>,
    name: Option<String>,
    path: Option<String>,
    starter_prompt: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvableSession {
    id: String,
    name: Option<String>,
    path: String,
}

/// `createResumeSessionToolDefinition()`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResumeSessionTool;

impl ResumeSessionTool {
    pub fn new() -> Self {
        Self
    }
}

pub fn parameters() -> &'static SchemaValue {
    static PARAMETERS: OnceLock<SchemaValue> = OnceLock::new();
    PARAMETERS.get_or_init(|| {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to a Pi .jsonl session file to resume."
                },
                "id": {
                    "type": "string",
                    "description": "Exact or unique prefix of a Pi session id to resume."
                },
                "name": {
                    "type": "string",
                    "description": "Unique named Pi session to resume."
                },
                "starter_prompt": {
                    "type": "string",
                    "description": "Optional caller-provided instructions to send after the target session is resumed."
                }
            }
        })
    })
}

fn read_non_empty_string(record: &Map, key: &str) -> Result<Option<JsString>, ToolError> {
    let Some(value) = record.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(text) = value.as_string() else {
        return Err(ToolError::new(format!(
            "resume_session {key} must be a non-empty string"
        )));
    };
    if text.trim().is_empty() {
        return Err(ToolError::new(format!(
            "resume_session {key} must be a non-empty string"
        )));
    }
    Ok(Some(text.trim().to_owned()))
}

fn normalize_resume_session_params(params: &JsValue) -> Result<ResumeSessionParams, ToolError> {
    let Some(record) = params.as_object() else {
        return Err(ToolError::new("resume_session requires { path }, { id }, or { name }"));
    };
    let id = read_non_empty_string(record, "id")?.as_ref().map(encode_node_utf8);
    let name = read_non_empty_string(record, "name")?.as_ref().map(encode_node_utf8);
    let path = read_non_empty_string(record, "path")?.as_ref().map(encode_node_utf8);
    let target_count = [id.is_some(), name.is_some(), path.is_some()]
        .into_iter()
        .filter(|present| *present)
        .count();
    if target_count != 1 {
        return Err(ToolError::new(
            "resume_session requires exactly one of path, id, or name",
        ));
    }
    Ok(ResumeSessionParams {
        id,
        name,
        path,
        starter_prompt: read_non_empty_string(record, "starter_prompt")?,
    })
}

fn assert_resume_session_path(path: &str) -> Result<String, ToolError> {
    let candidate = Path::new(path);
    if !candidate.exists() || !candidate.is_file() {
        return Err(ToolError::new(format!("Session file does not exist: {path}")));
    }
    if !path.ends_with(".jsonl") {
        return Err(ToolError::new(format!("Session file must be a .jsonl file: {path}")));
    }
    Ok(path.to_owned())
}

fn find_unique_session_match<'a>(
    matched: Vec<&'a ResolvableSession>,
    label: &str,
) -> Result<&'a ResolvableSession, ToolError> {
    match matched.len() {
        0 => Err(ToolError::new(format!("No session found matching {label}"))),
        1 => Ok(matched[0]),
        _ => Err(ToolError::new(format!("Ambiguous session match for {label}"))),
    }
}

fn process_cwd() -> String {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn resolve_session_path(path: &str) -> String {
    resolve_path(path, &process_cwd(), &PathInputOptions::default()).unwrap_or_else(|_| path.to_owned())
}

fn merge_by_path(groups: Vec<Vec<SessionInfo>>) -> Vec<SessionInfo> {
    let mut order = Vec::new();
    let mut by_path: HashMap<String, SessionInfo> = HashMap::new();
    for session in groups.into_iter().flatten() {
        if !by_path.contains_key(&session.path) {
            order.push(session.path.clone());
        }
        by_path.insert(session.path.clone(), session);
    }
    order.into_iter().filter_map(|path| by_path.remove(&path)).collect()
}

fn list_local_and_session_dir_sessions(
    session: &SessionManager,
    control_db_path: Option<&str>,
) -> Result<Vec<SessionInfo>, ToolError> {
    let local = SessionManager::list(
        session.get_cwd(),
        Some(session.get_session_dir()),
        None,
        control_db_path,
    )
    .map_err(|error| ToolError::new(error.to_string()))?;
    let session_dir = SessionManager::list_all(Some(session.get_session_dir()), None, control_db_path)
        .map_err(|error| ToolError::new(error.to_string()))?;
    Ok(merge_by_path(vec![local, session_dir]))
}

fn list_all_resolvable_sessions(
    session: &SessionManager,
    control_db_path: Option<&str>,
) -> Result<Vec<SessionInfo>, ToolError> {
    let scoped = list_local_and_session_dir_sessions(session, control_db_path)?;
    let default_sessions =
        SessionManager::list_all(None, None, control_db_path).map_err(|error| ToolError::new(error.to_string()))?;
    Ok(merge_by_path(vec![scoped, default_sessions]))
}

fn resolvable_session_from_info(session: &SessionInfo) -> ResolvableSession {
    ResolvableSession {
        id: session.id.clone(),
        name: session.name.clone(),
        path: session.path.clone(),
    }
}

fn resolvable_sessions(sessions: Vec<SessionInfo>) -> Vec<ResolvableSession> {
    sessions.iter().map(resolvable_session_from_info).collect()
}

fn resolvable_session_from_metadata(metadata: &SessionMetadata) -> ResolvableSession {
    ResolvableSession {
        id: metadata.id.clone(),
        name: metadata.name.clone(),
        path: metadata.session_path.clone(),
    }
}

fn sessions_in_current_directory(sessions: Vec<ResolvableSession>, session: &SessionManager) -> Vec<ResolvableSession> {
    let session_dir = resolve_session_path(session.get_session_dir());
    let prefix = format!("{session_dir}/");
    sessions
        .into_iter()
        .filter(|session| resolve_session_path(&session.path).starts_with(&prefix))
        .collect()
}

fn find_metadata_sessions(
    params: &ResumeSessionParams,
    control_db_path: Option<&str>,
) -> Result<Vec<ResolvableSession>, ToolError> {
    let Some(control_db_path) = control_db_path else {
        return Ok(Vec::new());
    };
    let metadata = match &params.name {
        Some(name) => find_active_session_metadata_by_name(control_db_path, name),
        None => find_active_session_metadata_by_id(control_db_path, params.id.as_deref().unwrap_or("")),
    }
    .map_err(|error| ToolError::new(error.to_string()))?;
    Ok(metadata.iter().map(resolvable_session_from_metadata).collect())
}

fn find_resume_session_by_id(sessions: &[ResolvableSession], id: &str) -> Result<Option<ResolvableSession>, ToolError> {
    let exact_matches: Vec<&ResolvableSession> = sessions.iter().filter(|session| session.id == id).collect();
    match exact_matches.len() {
        1 => return Ok(Some(exact_matches[0].clone())),
        count if count > 1 => {
            return Err(ToolError::new(format!("Ambiguous session match for id '{id}'")));
        }
        _ => {}
    }
    let prefix_matches: Vec<&ResolvableSession> =
        sessions.iter().filter(|session| session.id.starts_with(id)).collect();
    match prefix_matches.len() {
        1 => Ok(Some(prefix_matches[0].clone())),
        count if count > 1 => Err(ToolError::new(format!("Ambiguous session match for id '{id}'"))),
        _ => Ok(None),
    }
}

fn unique_name_match(sessions: Vec<&ResolvableSession>, label: &str) -> Result<ResolvableSession, ToolError> {
    find_unique_session_match(sessions, label).cloned()
}

fn resolve_resume_session_file(
    params: &ResumeSessionParams,
    cwd: &str,
    session: &SessionManager,
    control_db_path: Option<&str>,
) -> Result<String, ToolError> {
    if let Some(path) = &params.path {
        let resolved =
            resolve_path(path, cwd, &PathInputOptions::default()).map_err(|error| ToolError::new(error.to_string()))?;
        return assert_resume_session_path(&resolved);
    }

    if let Some(name) = &params.name {
        let label = format!("name '{name}'");
        let metadata_sessions = find_metadata_sessions(params, control_db_path)?;
        let scoped_metadata = sessions_in_current_directory(metadata_sessions.clone(), session);
        if !scoped_metadata.is_empty() {
            return assert_resume_session_path(&unique_name_match(scoped_metadata.iter().collect(), &label)?.path);
        }
        if !metadata_sessions.is_empty() {
            return assert_resume_session_path(&unique_name_match(metadata_sessions.iter().collect(), &label)?.path);
        }
        let scoped_matches: Vec<ResolvableSession> =
            resolvable_sessions(list_local_and_session_dir_sessions(session, control_db_path)?)
                .into_iter()
                .filter(|session| session.name.as_deref() == Some(name.as_str()))
                .collect();
        if !scoped_matches.is_empty() {
            return assert_resume_session_path(&unique_name_match(scoped_matches.iter().collect(), &label)?.path);
        }
        let all_matches: Vec<ResolvableSession> =
            resolvable_sessions(list_all_resolvable_sessions(session, control_db_path)?)
                .into_iter()
                .filter(|session| session.name.as_deref() == Some(name.as_str()))
                .collect();
        return assert_resume_session_path(&unique_name_match(all_matches.iter().collect(), &label)?.path);
    }

    let Some(id) = &params.id else {
        return Err(ToolError::new("resume_session requires { path }, { id }, or { name }"));
    };
    find_session_file_by_id(id, session, control_db_path)
}

/// `findSessionFileById`.
pub fn find_session_file_by_id(
    id: &str,
    session: &SessionManager,
    control_db_path: Option<&str>,
) -> Result<String, ToolError> {
    let params = ResumeSessionParams {
        id: Some(id.to_owned()),
        ..Default::default()
    };
    let metadata_sessions = find_metadata_sessions(&params, control_db_path)?;
    let scoped_metadata_match =
        find_resume_session_by_id(&sessions_in_current_directory(metadata_sessions.clone(), session), id)?;
    if let Some(matched) = scoped_metadata_match {
        return assert_resume_session_path(&matched.path);
    }
    if let Some(matched) = find_resume_session_by_id(&metadata_sessions, id)? {
        return assert_resume_session_path(&matched.path);
    }
    if let Some(matched) = find_resume_session_by_id(
        &resolvable_sessions(list_local_and_session_dir_sessions(session, control_db_path)?),
        id,
    )? {
        return assert_resume_session_path(&matched.path);
    }
    let all_sessions = resolvable_sessions(list_all_resolvable_sessions(session, control_db_path)?);
    let Some(matched) = find_resume_session_by_id(&all_sessions, id)? else {
        return Err(ToolError::new(format!("No session found matching id '{id}'")));
    };
    assert_resume_session_path(&matched.path)
}

fn assert_resume_target_is_different(session_path: &str, session: &SessionManager) -> Result<(), ToolError> {
    if let Some(current_session_path) = session.get_session_file()
        && resolve_session_path(session_path) == resolve_session_path(current_session_path)
    {
        return Err(ToolError::new("resume_session cannot resume the current session"));
    }
    Ok(())
}

impl AgentHarnessTool<ExtensionContext> for ResumeSessionTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> &str {
        DESCRIPTION
    }

    fn parameters(&self) -> &SchemaValue {
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
                return Err(ToolError::new(
                    "resume_session is only available from the main supervisor session",
                ));
            }
            let cwd = context.cwd().map_err(|error| ToolError::new(error.message))?;
            let session = context
                .session_manager()
                .map_err(|error| ToolError::new(error.message))?;
            let control_db_path = context
                .control_db_path()
                .map_err(|error| ToolError::new(error.message))?;
            let resume_params = normalize_resume_session_params(&params)?;
            let session_path = {
                let manager = lock_session_manager(&session);
                let session_path =
                    resolve_resume_session_file(&resume_params, &cwd, &manager, control_db_path.as_deref())?;
                assert_resume_target_is_different(&session_path, &manager)?;
                session_path
            };
            let starter_prompt = resume_params.starter_prompt.clone();
            let options = starter_prompt.clone().map(|prompt| SwitchSessionOptions {
                with_session: Some(Arc::new(move |replaced| {
                    let prompt = prompt.clone();
                    Box::pin(async move {
                        (replaced.send_user_message)(UserMessageContent::Text(prompt.into()), None).await
                    })
                })),
            });
            let result = context
                .switch_session(session_path.clone(), options)
                .await
                .map_err(|error| ToolError::new(error.message))?;
            let details = json!({
                "cancelled": result.cancelled,
                "resumed": !result.cancelled,
                "sessionPath": session_path
            });
            let action = if result.cancelled {
                "Resume cancelled"
            } else {
                "Resumed session"
            };
            let starter = if starter_prompt.is_some() && !result.cancelled {
                " and sent instructions"
            } else {
                ""
            };
            Ok(terminating_text_result(
                format!("{action}: {session_path}{starter}"),
                details,
            ))
        })
    }
}

fn format_resume_target(args: &JsValue) -> String {
    if let Some(path) = args
        .get("path")
        .and_then(Value::as_string)
        .filter(|path| !path.is_empty())
    {
        return format!("path {}", encode_node_utf8(path));
    }
    if let Some(id) = args.get("id").and_then(Value::as_string).filter(|id| !id.is_empty()) {
        return format!("id {}", encode_node_utf8(id));
    }
    if let Some(name) = args
        .get("name")
        .and_then(Value::as_string)
        .filter(|name| !name.is_empty())
    {
        return format!("name {}", encode_node_utf8(name));
    }
    "target".to_owned()
}

fn format_resume_session_call(args: &JsValue, theme: &Theme) -> String {
    let target = format_resume_target(args);
    let starter = args
        .get("starter_prompt")
        .and_then(Value::as_string)
        .filter(|prompt| !prompt.is_empty())
        .map_or_else(String::new, |_| theme.fg(ThemeColor::Warning, " + instructions"));
    format!(
        "{} {}{starter}",
        theme.fg(ThemeColor::ToolTitle, &theme.bold("resume_session")),
        theme.fg(ThemeColor::Accent, &target)
    )
}

/// `renderCall` for the resume_session tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> ComponentRef {
    super::render_text_component(context, format_resume_session_call(args, theme))
}

/// `renderResult` for the resume_session tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> ComponentRef {
    super::render_text_component(context, super::render_tool_output(result, theme))
}

/// `createResumeSessionToolDefinition()`.
pub fn create_resume_session_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::harness_tool_definition(Arc::new(ResumeSessionTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.to_owned());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).to_owned()).collect());
    definition.render_call = Some(infallible_render_call(render_call));
    definition.render_result = Some(infallible_render_result(render_result));
    definition
}

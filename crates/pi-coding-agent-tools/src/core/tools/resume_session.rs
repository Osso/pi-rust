//! `resume_session` tool (`core/tools/resume-session.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::core::extensions::types::ContextTool;
use pi_agent_core::{
    AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError, ToolExecutionMode,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use pi_tui::tui::ComponentRef;
use serde_json::Value as SchemaValue;

use std::sync::Arc;

use super::result::terminating_text_result;
use crate::core::extensions::types::{
    ExtensionContext, HandlerError, SwitchSessionOptions, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
    UserMessageContent, WithSessionCallback,
};
use crate::core::session_archive_storage::is_archived_session_file;
use crate::core::session_control_db::{
    SessionMetadata, find_active_session_metadata_by_id, find_active_session_metadata_by_name,
};
use crate::core::session_manager::{SessionInfo, SessionManager};
use crate::core::session_transcript::read_session_transcript_path;
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::paths::{PathInputOptions, process_cwd, resolve_path};

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
    id: Option<JsString>,
    name: Option<JsString>,
    path: Option<JsString>,
    starter_prompt: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvableSession {
    id: JsString,
    name: Option<JsString>,
    path: JsString,
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
        let message = if params.as_array().is_some() {
            "resume_session requires exactly one of path, id, or name"
        } else {
            "resume_session requires { path }, { id }, or { name }"
        };
        return Err(ToolError::new(message));
    };
    let id = read_non_empty_string(record, "id")?;
    let name = read_non_empty_string(record, "name")?;
    let path = read_non_empty_string(record, "path")?;
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

fn assert_resume_session_path(path: &JsString) -> Result<JsString, ToolError> {
    let path = read_session_transcript_path(path);
    let encoded = encode_node_utf8(&path);
    if !Path::new(&encoded).exists() {
        return Err(ToolError::new(js_format!("Session file does not exist: {}", path)));
    }
    let metadata = std::fs::metadata(&encoded)
        .map_err(|error| ToolError::new(pi_js_compat::node_fs::node_fs_error_record(&error, "stat", &path).message))?;
    if !metadata.is_file() {
        return Err(ToolError::new(js_format!("Session file does not exist: {}", path)));
    }
    if !path.ends_with(".jsonl") && !is_archived_session_file(&path) {
        return Err(ToolError::new(js_format!(
            "Session file must be a .jsonl or .jsonl.zst file: {}",
            path
        )));
    }
    Ok(path)
}

fn find_unique_session_match<'a>(
    matched: Vec<&'a ResolvableSession>,
    label: &JsString,
) -> Result<&'a ResolvableSession, ToolError> {
    match matched.len() {
        0 => Err(ToolError::new(JsString::from("No session found matching ") + label)),
        1 => Ok(matched[0]),
        _ => Err(ToolError::new(JsString::from("Ambiguous session match for ") + label)),
    }
}

pub(crate) fn resolve_session_path(path: &JsString) -> Result<JsString, ToolError> {
    let cwd = process_cwd().map_err(|error| ToolError::new(error.message()))?;
    resolve_path(path, cwd, &PathInputOptions::default()).map_err(|error| ToolError::new(error.message()))
}

fn merge_by_path(groups: Vec<Vec<SessionInfo>>) -> Vec<SessionInfo> {
    let mut order = Vec::new();
    let mut by_path: HashMap<JsString, SessionInfo> = HashMap::new();
    for session in groups.into_iter().flatten() {
        if !by_path.contains_key(&session.path) {
            order.push(session.path.clone());
        }
        by_path.insert(session.path.clone(), session);
    }
    order.into_iter().filter_map(|path| by_path.remove(&path)).collect()
}

fn list_local_and_session_dir_sessions(
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
    control_db_path: Option<&JsString>,
) -> Result<Vec<SessionInfo>, ToolError> {
    let local = SessionManager::list(
        &session.get_cwd(),
        Some(&session.get_session_dir()),
        None,
        control_db_path,
    )
    .map_err(|error| ToolError::new(error.message()))?;
    let session_dir = SessionManager::list_all(Some(&session.get_session_dir()), None, control_db_path)
        .map_err(|error| ToolError::new(error.message()))?;
    Ok(merge_by_path(vec![local, session_dir]))
}

fn list_all_resolvable_sessions(
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
    control_db_path: Option<&JsString>,
) -> Result<Vec<SessionInfo>, ToolError> {
    let scoped = list_local_and_session_dir_sessions(session, control_db_path)?;
    let default_sessions =
        SessionManager::list_all(None, None, control_db_path).map_err(|error| ToolError::new(error.message()))?;
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

fn sessions_in_current_directory(
    sessions: Vec<ResolvableSession>,
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
) -> Result<Vec<ResolvableSession>, ToolError> {
    let prefix = resolve_session_path(&session.get_session_dir())? + "/";
    let mut scoped = Vec::new();
    for session in sessions {
        if resolve_session_path(&session.path)?.starts_with(&prefix) {
            scoped.push(session);
        }
    }
    Ok(scoped)
}

fn find_metadata_sessions(
    params: &ResumeSessionParams,
    control_db_path: Option<&JsString>,
) -> Result<Vec<ResolvableSession>, ToolError> {
    let Some(control_db_path) = control_db_path else {
        return Ok(Vec::new());
    };
    let metadata = match &params.name {
        Some(name) => find_active_session_metadata_by_name(control_db_path, name),
        None => find_active_session_metadata_by_id(control_db_path, params.id.as_ref().unwrap_or(&JsString::default())),
    }
    .map_err(|error| ToolError::new(error.message()))?;
    Ok(metadata.iter().map(resolvable_session_from_metadata).collect())
}

fn find_resume_session_by_id(
    sessions: &[ResolvableSession],
    id: &JsString,
) -> Result<Option<ResolvableSession>, ToolError> {
    let exact_matches: Vec<&ResolvableSession> = sessions.iter().filter(|session| session.id == *id).collect();
    match exact_matches.len() {
        1 => return Ok(Some(exact_matches[0].clone())),
        count if count > 1 => {
            return Err(ToolError::new(js_format!("Ambiguous session match for id '{}'", id)));
        }
        _ => {}
    }
    let prefix_matches: Vec<&ResolvableSession> =
        sessions.iter().filter(|session| session.id.starts_with(id)).collect();
    match prefix_matches.len() {
        1 => Ok(Some(prefix_matches[0].clone())),
        count if count > 1 => Err(ToolError::new(js_format!("Ambiguous session match for id '{}'", id))),
        _ => Ok(None),
    }
}

fn unique_name_match(sessions: Vec<&ResolvableSession>, label: &JsString) -> Result<ResolvableSession, ToolError> {
    find_unique_session_match(sessions, label).cloned()
}

fn resolve_resume_session_file(
    params: &ResumeSessionParams,
    cwd: &JsString,
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
    control_db_path: Option<&JsString>,
) -> Result<JsString, ToolError> {
    if let Some(path) = &params.path {
        let resolved =
            resolve_path(path, cwd, &PathInputOptions::default()).map_err(|error| ToolError::new(error.message()))?;
        return assert_resume_session_path(&resolved);
    }

    if let Some(name) = &params.name {
        let label = JsString::from("name '") + name + "'";
        let metadata_sessions = find_metadata_sessions(params, control_db_path)?;
        let scoped_metadata = sessions_in_current_directory(metadata_sessions.clone(), session)?;
        if !scoped_metadata.is_empty() {
            return assert_resume_session_path(&unique_name_match(scoped_metadata.iter().collect(), &label)?.path);
        }
        if !metadata_sessions.is_empty() {
            return assert_resume_session_path(&unique_name_match(metadata_sessions.iter().collect(), &label)?.path);
        }
        let scoped_matches: Vec<ResolvableSession> =
            resolvable_sessions(list_local_and_session_dir_sessions(session, control_db_path)?)
                .into_iter()
                .filter(|session| session.name.as_ref() == Some(name))
                .collect();
        if !scoped_matches.is_empty() {
            return assert_resume_session_path(&unique_name_match(scoped_matches.iter().collect(), &label)?.path);
        }
        let all_matches: Vec<ResolvableSession> =
            resolvable_sessions(list_all_resolvable_sessions(session, control_db_path)?)
                .into_iter()
                .filter(|session| session.name.as_ref() == Some(name))
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
    id: &JsString,
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
    control_db_path: Option<&JsString>,
) -> Result<JsString, ToolError> {
    let params = ResumeSessionParams {
        id: Some(id.to_owned()),
        ..Default::default()
    };
    let metadata_sessions = find_metadata_sessions(&params, control_db_path)?;
    let scoped_metadata_match =
        find_resume_session_by_id(&sessions_in_current_directory(metadata_sessions.clone(), session)?, id)?;
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
        return Err(ToolError::new(js_format!("No session found matching id '{}'", id)));
    };
    assert_resume_session_path(&matched.path)
}

fn assert_resume_target_is_different(
    session_path: &JsString,
    session: &crate::core::readonly_session_manager::ReadonlySessionManager,
) -> Result<(), ToolError> {
    if let Some(current_session_path) = session.get_session_file()
        && resolve_session_path(session_path)? == resolve_session_path(&current_session_path)?
    {
        return Err(ToolError::new("resume_session cannot resume the current session"));
    }
    Ok(())
}

fn create_switch_options(starter_prompt: Option<JsString>) -> SwitchSessionOptions {
    let with_session = starter_prompt.map(|prompt| {
        let callback: WithSessionCallback = Arc::new(move |replaced| {
            let prompt = prompt.clone();
            Box::pin(async move { (replaced.send_user_message)(UserMessageContent::Text(prompt), None).await })
        });
        callback
    });
    SwitchSessionOptions { with_session }
}

impl ContextTool<Option<ExtensionContext>> for ResumeSessionTool {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> pi_js_compat::js_json::JsString {
        DESCRIPTION.into()
    }

    fn parameters(&self) -> &SchemaValue {
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
            let context =
                context.ok_or_else(|| ToolError::new("resume_session is not available in this session mode"))?;
            let agent_id = context
                .multi_agent_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            let requires_agent_id = context
                .multi_agent_requires_agent_id()
                .map_err(|error| ToolError::new(error.message))?;
            if agent_id.as_ref().is_some_and(|id| !id.is_empty()) || requires_agent_id {
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
                let manager = session;
                let session_path =
                    resolve_resume_session_file(&resume_params, &cwd, &manager, control_db_path.as_ref())?;
                assert_resume_target_is_different(&session_path, &manager)?;
                session_path
            };
            let starter_prompt = resume_params.starter_prompt.clone();
            let options = create_switch_options(starter_prompt.clone());
            let result = context
                .switch_session(session_path.clone(), Some(options))
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
                js_format!("{}: {}{}", action, session_path, starter),
                details,
            ))
        })
    }
}

fn format_resume_target(args: &JsValue) -> JsString {
    if let Some(path) = args
        .get("path")
        .and_then(Value::as_string)
        .filter(|path| !path.is_empty())
    {
        return js_format!("path {}", path);
    }
    if let Some(id) = args.get("id").and_then(Value::as_string).filter(|id| !id.is_empty()) {
        return js_format!("id {}", id);
    }
    if let Some(name) = args
        .get("name")
        .and_then(Value::as_string)
        .filter(|name| !name.is_empty())
    {
        return js_format!("name {}", name);
    }
    "target".into()
}

fn format_resume_session_call(args: &JsValue, theme: &Theme) -> JsString {
    let target = format_resume_target(args);
    let starter = args
        .get("starter_prompt")
        .and_then(Value::as_string)
        .filter(|prompt| !prompt.is_empty())
        .map_or_else(JsString::default, |_| theme.fg(ThemeColor::Warning, " + instructions"));
    js_format!(
        "{} {}{}",
        theme.fg(ThemeColor::ToolTitle, theme.bold("resume_session")),
        theme.fg(ThemeColor::Accent, &target),
        starter
    )
}

/// `renderCall` for the resume_session tool.
pub fn render_call(args: &JsValue, theme: &Theme, context: &ToolRenderContext) -> Result<ComponentRef, HandlerError> {
    super::render_utils::set_text_component(format_resume_session_call(args, theme), context)
}

/// `renderResult` for the resume_session tool.
pub fn render_result(
    result: &AgentToolResult,
    _options: &ToolRenderResultOptions,
    theme: &Theme,
    context: &ToolRenderContext,
) -> Result<ComponentRef, HandlerError> {
    super::render_utils::set_text_component(super::render_tool_output(result, theme), context)
}

/// `createResumeSessionToolDefinition()`.
pub fn create_resume_session_tool_definition() -> ToolDefinition {
    let mut definition = crate::core::extensions::wrapper::context_tool_definition(Arc::new(ResumeSessionTool::new()));
    definition.prompt_snippet = Some(PROMPT_SNIPPET.into());
    definition.prompt_guidelines = Some(PROMPT_GUIDELINES.iter().map(|line| (*line).into()).collect());
    definition.render_call = Some(Arc::new(render_call));
    definition.render_result = Some(Arc::new(render_result));
    definition
}

#[cfg(test)]
mod lossless_name_tests {
    use super::*;

    #[test]
    fn path_and_id_targets_keep_original_units() {
        let raw = JsString::from_utf16(vec![0xd800]);
        let id = normalize_resume_session_params(&json::parse_lossless(r#"{"id":" id-\ud800 "}"#).unwrap()).unwrap();
        assert_eq!(id.id, Some(JsString::from("id-") + &raw));
        let path =
            normalize_resume_session_params(&json::parse_lossless(r#"{"path":" /session-\ud800.jsonl "}"#).unwrap())
                .unwrap();
        assert_eq!(path.path, Some(JsString::from("/session-") + raw + ".jsonl"));
    }

    #[test]
    fn name_lookup_and_not_found_message_keep_original_units() {
        let input = json::parse_lossless(r#"{"name":" \ud800\udfff\ud800 "}"#).unwrap();
        let params = normalize_resume_session_params(&input).unwrap();
        assert_eq!(params.name.as_ref().unwrap().units(), &[0xd800, 0xdfff, 0xd800]);
        let session = crate::core::readonly_session_manager::create_readonly_session_manager(std::sync::Arc::new(
            std::sync::Mutex::new(SessionManager::in_memory(None, None).unwrap()),
        ));
        let error = resolve_resume_session_file(&params, &"/tmp".into(), &session, None).unwrap_err();
        assert!(error.message.contains(params.name.unwrap()));
    }
}
